import { act, render, screen, waitFor, within } from "@testing-library/react";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../App";
import { MockApi } from "../api/MockApi";
import type { Task } from "../api/types";
import { mascotState, runningElsewhere } from "./timeline";

vi.mock("../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

const SLOW = { timeout: 5000 };

/** The app over the sample data; `hedy ask` writes to `api` behind its back. */
function setup(hash = "#") {
  const api = MockApi.withSampleData();
  api.delayMs = 0;
  history.replaceState(null, "", hash);
  render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
  return api;
}

const sidebar = () => screen.getByRole("complementary", { name: "Proyectos y cuentas" });

/** The user comes back to the window. */
const focusWindow = () => act(() => void window.dispatchEvent(new Event("focus")));

describe("Conversations asked from the terminal (hedy ask)", () => {
  afterEach(() => {
    history.replaceState(null, "", "#");
    localStorage.clear();
  });

  it("shows a conversation stored by another process when the window gets the focus", async () => {
    const api = setup();
    await screen.findByRole("heading", { name: /¡Hola, Alex!/ }, SLOW);
    expect(within(sidebar()).queryByRole("button", { name: "¿Qué es un monad?" })).not.toBeInTheDocument();

    api.storeOutsideConversation("¿Qué es un monad?", [
      { kind: "message", text: "Un patrón para encadenar pasos." },
      { kind: "finished", ok: true, summary: "", durationMs: 1000 },
    ]);
    await focusWindow();
    expect(await within(sidebar()).findByRole("button", { name: "¿Qué es un monad?" })).toBeInTheDocument();
  });

  /** Opens `chat` and waits for its history so far (the question, in the heading and the timeline). */
  async function open(api: MockApi, chat: Task) {
    api.delayMs = 0;
    history.replaceState(null, "", `#c=${chat.id}`);
    render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
    expect(await screen.findByRole("heading", { name: chat.prompt, level: 1 }, SLOW)).toBeInTheDocument();
    await waitFor(() => expect(screen.getAllByText(chat.prompt).length).toBeGreaterThan(1));
  }

  it("follows an open conversation while the terminal answers it, with no other trigger", async () => {
    const api = MockApi.withSampleData();
    const chat = api.storeOutsideConversation("¿Qué es un functor?", [], "running");
    await open(api, chat);
    expect(screen.queryByText("Algo que se puede mapear.")).not.toBeInTheDocument();

    api.updateOutside(
      chat.id,
      [
        { kind: "message", text: "Algo que se puede mapear." },
        { kind: "finished", ok: true, summary: "", durationMs: 1000 },
      ],
      "review",
    );
    // Read again every few seconds while it runs elsewhere.
    expect(await screen.findByText("Algo que se puede mapear.")).toBeInTheDocument();
  });

  it("reads an answered conversation again on focus, for a follow-up asked in the terminal", async () => {
    const api = MockApi.withSampleData();
    const chat = api.storeOutsideConversation("¿Qué es un monoide?", [
      { kind: "message", text: "Un conjunto con una operación." },
      { kind: "finished", ok: true, summary: "", durationMs: 1000 },
    ]);
    await open(api, chat);
    expect(await screen.findByText("Un conjunto con una operación.")).toBeInTheDocument();

    // `hedy ask -c "¿un ejemplo?"`, already answered.
    api.updateOutside(chat.id, [
      { kind: "userMessage", text: "¿Un ejemplo?" },
      { kind: "message", text: "Los enteros con la suma." },
      { kind: "finished", ok: true, summary: "", durationMs: 1000 },
    ]);
    await focusWindow();
    expect(await screen.findByText("Los enteros con la suma.")).toBeInTheDocument();
  });

  it("a conversation running elsewhere looks running, not asleep", () => {
    const base = { conversation: true, live: false, status: "running" } as Task;
    expect(runningElsewhere(base)).toBe(true);
    expect(mascotState(base, [])).toBe("working");
    // Answered, or with an agent in this app: not elsewhere.
    expect(runningElsewhere({ ...base, status: "review" })).toBe(false);
    expect(runningElsewhere({ ...base, live: true })).toBe(false);
    // A project task without an agent keeps its old look.
    const task = { ...base, conversation: false };
    expect(runningElsewhere(task)).toBe(false);
    expect(mascotState(task, [])).toBe("sleeping");
  });
});
