import { act, renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { StrictMode, type ReactNode } from "react";

import { ApiProvider } from "../api/context";
import { MockApi } from "../api/MockApi";
import type { Task } from "../api/types";
import { ToastProvider } from "../components/ui/Toast";
import { translator } from "../i18n";
import { formatReviewMessage, ReviewCommentsProvider, useReviewComments, type ReviewComment } from "./comments";

const TASK = { kind: "task" as const, task: "s5" };

async function setup(taskId = "s5", project = "/code/hedy", strict = false) {
  const api = MockApi.withSampleData().withSampleActivity();
  api.delayMs = 0;
  const found = (await api.tasks(project)).find((t) => t.id === taskId) as Task;
  // The running sample task has no worktree in the sample data: lend it one.
  const sample = (await api.tasks("/code/hedy")).find((t) => t.id === "s5") as Task;
  const task = { ...found, workspace: found.workspace ?? sample.workspace };
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const tree = (children: ReactNode) => (
    <QueryClientProvider client={client}>
      <ApiProvider api={api}>
        <ToastProvider>
          <ReviewCommentsProvider task={task}>{children}</ReviewCommentsProvider>
        </ToastProvider>
      </ApiProvider>
    </QueryClientProvider>
  );
  const wrapper = ({ children }: { children: ReactNode }) => (strict ? <StrictMode>{tree(children)}</StrictMode> : tree(children));
  const hook = renderHook(() => useReviewComments(), { wrapper });
  return { api, task, ...hook };
}

function comment(extra: Partial<ReviewComment>): ReviewComment {
  return {
    id: "c",
    copy: TASK,
    path: "facturas/impuestos.py",
    line: 16,
    endLine: null,
    side: "new",
    text: "¿Y si la cantidad es cero?",
    code: "    neto = precio * cantidad",
    createdAt: 0,
    sentAt: null,
    ...extra,
  };
}

describe("review comments", () => {
  afterEach(() => localStorage.clear());

  it("adds a first comment once, even when React runs the update twice", async () => {
    const { result } = await setup("s5", "/code/hedy", true);
    act(() => {
      result.current.add({ copy: TASK, path: "facturas/impuestos.py", line: 16, side: "new", text: "Una vez", code: "x", source: "k1" });
    });
    expect(result.current.comments).toHaveLength(1);
    expect(result.current.comments[0]?.source).toBe("k1");
    const stored: unknown[] = JSON.parse(localStorage.getItem("hedy.review.comments.task:s5") ?? "[]");
    expect(stored).toHaveLength(1);
  });

  it("adds, edits and removes comments, kept per task across reloads", async () => {
    const { result, unmount } = await setup();
    let id = "";
    act(() => {
      id = result.current.add({ copy: TASK, path: "facturas/impuestos.py", line: 16, endLine: 18, side: "new", text: " Redondea por línea ", code: "x" }).id;
    });
    expect(result.current.comments).toHaveLength(1);
    expect(result.current.pending[0]).toMatchObject({ line: 16, endLine: 18, text: "Redondea por línea", sentAt: null });

    act(() => result.current.update(id, "Usa quantize"));
    expect(result.current.comments[0]?.text).toBe("Usa quantize");
    unmount();

    // A reload: the comments come back from the browser storage.
    const again = await setup();
    expect(again.result.current.comments.map((c) => c.text)).toEqual(["Usa quantize"]);
    act(() => again.result.current.remove(id));
    expect(again.result.current.comments).toEqual([]);
    expect(localStorage.getItem("hedy.review.comments.task:s5")).toBeNull();
  });

  it("quotes the lines from the copy when the caller does not pass them", async () => {
    const { result } = await setup();
    act(() => {
      result.current.add({ copy: TASK, path: "facturas/impuestos.py", line: 16, side: "new", text: "¿cero?" });
    });
    await waitFor(() => expect(result.current.comments[0]?.code).toBe("    neto = precio * cantidad"));
  });

  it("formats one follow-up: intro, then path:line, quoted code and the comment", () => {
    const t = translator("es");
    const text = formatReviewMessage(
      [comment({}), comment({ path: "facturas/totales.py", line: 3, endLine: 5, side: "old", code: "a\nb\nc", text: "Esto se perdió" })],
      t,
    );
    expect(text).toBe(
      [
        "Revisé tus cambios y te dejé 2 comentarios. Atiéndelos y cuéntame qué cambiaste.",
        "1. `facturas/impuestos.py:16`\n```\n    neto = precio * cantidad\n```\n¿Y si la cantidad es cero?",
        "2. `facturas/totales.py:3-5` (versión anterior a tus cambios)\n```\na\nb\nc\n```\nEsto se perdió",
      ].join("\n\n"),
    );
    expect(formatReviewMessage([comment({ code: "x = ```y```" })], translator("en"))).toContain(
      "I reviewed your changes and left one comment.",
    );
    // A fence the quoted code cannot close.
    expect(formatReviewMessage([comment({ code: "x = ```y```" })], t)).toContain("````\nx = ```y```\n````");
  });

  it("sends every pending comment as one follow-up and keeps them as sent", async () => {
    const { api, result } = await setup();
    const queue = vi.spyOn(api, "queueMessage");
    act(() => {
      result.current.add({ copy: TASK, path: "facturas/impuestos.py", line: 16, side: "new", text: "Uno", code: "a" });
      result.current.add({ copy: TASK, path: "facturas/totales.py", line: 4, side: "new", text: "Dos", code: "b" });
    });
    expect(result.current.canSend).toBe(true);
    let sent = false;
    await act(async () => {
      sent = await result.current.sendToAi();
    });
    expect(sent).toBe(true);
    expect(queue).toHaveBeenCalledTimes(1);
    expect(queue.mock.calls[0]?.[0]).toBe("s5");
    expect(queue.mock.calls[0]?.[1]).toContain("1. `facturas/impuestos.py:16`");
    expect(queue.mock.calls[0]?.[1]).toContain("2. `facturas/totales.py:4`");
    expect(result.current.pending).toEqual([]);
    expect(result.current.comments.every((c) => c.sentAt !== null)).toBe(true);
    // Nothing left to send.
    await act(async () => {
      sent = await result.current.sendToAi();
    });
    expect(sent).toBe(false);
  });

  it("queues the follow-up while the agent works, like the composer", async () => {
    const { api, result } = await setup("s4", "/code/tienda");
    const copy = { kind: "task" as const, task: "s4" };
    act(() => {
      result.current.add({ copy, path: "carrito.ts", line: 2, side: "new", text: "Revisa esto", code: "x" });
      result.current.add({ copy, path: "carrito.ts", line: 9, side: "new", text: "Y esto", code: "y" });
    });
    const [first] = result.current.pending;
    await act(async () => {
      await result.current.sendToAi(first ? [first.id] : []);
    });
    const queue = await api.queued("s4");
    expect(queue.at(-1)?.text).toContain("Revisa esto");
    expect(queue.at(-1)?.text).not.toContain("Y esto");
    expect(result.current.pending.map((c) => c.text)).toEqual(["Y esto"]);
  });

  it("selects a file to show in the review", async () => {
    const { result } = await setup();
    expect(result.current.selectedFile).toBeNull();
    act(() => result.current.selectFile("facturas/totales.py", 4));
    expect(result.current).toMatchObject({ selectedFile: "facturas/totales.py", selectedLine: 4, selectSeq: 1 });
  });
});
