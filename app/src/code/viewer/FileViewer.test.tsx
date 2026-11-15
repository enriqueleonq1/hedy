import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { CodeCopy, FileHistory } from "../../api/codeTypes";
import { useComposerInsert } from "../../screens/workspace/composerBridge";
import { lineMarksFromHunks } from "../editor/lineMarks";
import { renderCode } from "../testing";
import { FileViewer, type OpenFile } from "./FileViewer";
import { mergeHistory } from "./HistoryDrawer";

const S5: CodeCopy = { kind: "task", task: "s5" };
const HEDY: CodeCopy = { kind: "folder", project: "/code/hedy" };

function Composer({ onInsert }: { onInsert: (text: string) => void }) {
  useComposerInsert(onInsert);
  return null;
}

function setup(copy: CodeCopy, file: Omit<OpenFile, "seq">) {
  const handlers = {
    onChange: vi.fn(),
    onRevealFolder: vi.fn(),
    onShowDiff: vi.fn(),
    onOpenTask: vi.fn(),
    onBack: vi.fn(),
    onClose: vi.fn(),
  };
  const inserted = vi.fn();
  const view = renderCode(
    <>
      <Composer onInsert={inserted} />
      <FileViewer copy={copy} info={undefined} file={{ ...file, seq: 1 }} {...handlers} />
    </>,
  );
  return { ...view, ...handlers, inserted, user: userEvent.setup() };
}

describe("FileViewer", () => {
  it("shows a text file in the editor with clickable breadcrumbs and toolbar actions", async () => {
    const { api, onRevealFolder, onShowDiff, user } = setup(S5, { path: "facturas/impuestos.py", line: 16 });
    const editor = await screen.findByRole("textbox", { name: "Contenido de impuestos.py" });
    expect(editor).toHaveTextContent("def iva_de_linea");
    expect(editor).toHaveAttribute("aria-readonly", "true");
    const crumbs = screen.getByRole("navigation", { name: "Ruta del archivo" });
    expect(within(crumbs).getByText("impuestos.py")).toHaveAttribute("aria-current", "page");
    await user.click(within(crumbs).getByRole("button", { name: "facturas" }));
    expect(onRevealFolder).toHaveBeenCalledWith("facturas");
    await user.click(screen.getByRole("button", { name: "Ver cambios" }));
    expect(onShowDiff).toHaveBeenCalledWith("facturas/impuestos.py");
    await user.click(screen.getByRole("button", { name: "Abrir en VS Code en esta línea" }));
    await waitFor(() => expect(api.code.opened.at(-1)?.path).toMatch(/facturas\/impuestos\.py$/));
    // The changed lines are marked in the gutter.
    await waitFor(() => expect(document.querySelectorAll(".cm-change--added, .cm-change--modified").length).toBeGreaterThan(0));
  });

  it("asks for the version before the AI and offers to go back", async () => {
    const { onChange, user } = setup(S5, { path: "facturas/impuestos.py", rev: "base" });
    expect(await screen.findByText(/Antes de la IA · como estaba en/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Volver a la versión actual" }));
    const next = onChange.mock.calls.at(-1)?.[0];
    expect(next).toMatchObject({ path: "facturas/impuestos.py" });
    expect(next.rev).toBeUndefined();
  });

  it("designs the protected and binary states", async () => {
    const locked = setup(HEDY, { path: ".env.example" });
    expect(await screen.findByText("Archivo protegido")).toBeInTheDocument();
    await locked.user.click(screen.getByRole("button", { name: "Abrir en VS Code" }));
    await waitFor(() => expect(locked.api.code.opened.at(-1)?.path).toMatch(/\.env\.example$/));
    locked.unmount();

    setup(HEDY, { path: "static/fuentes/Inter.woff2" });
    expect(await screen.findByText("Archivo binario")).toBeInTheDocument();
  });

  it("designs the too large, image and missing states", async () => {
    const large = setup(HEDY, { path: "data/clientes_demo.csv" });
    expect(await screen.findByText("Demasiado grande para mostrarlo aquí")).toBeInTheDocument();
    large.unmount();
    const image = setup(HEDY, { path: "static/logo.png" });
    expect(await screen.findByRole("img", { name: "logo.png" })).toBeInTheDocument();
    image.unmount();
    setup(HEDY, { path: "no/existe.py" });
    expect(await screen.findByText("No está en esta copia")).toBeInTheDocument();
  });

  it("asks the AI about the right-clicked lines and copies their reference", async () => {
    const { inserted, user } = setup(S5, { path: "facturas/impuestos.py" });
    const editor = await screen.findByRole("textbox", { name: "Contenido de impuestos.py" });
    fireEvent.contextMenu(editor);
    const menu = await screen.findByRole("menu", { name: /Acciones de la línea \d+/ });
    expect(within(menu).getByRole("menuitem", { name: "Comentar en la revisión" })).toBeEnabled();
    await user.click(within(menu).getByRole("menuitem", { name: "Preguntar a la IA sobre esto" }));
    expect(inserted).toHaveBeenCalledWith(expect.stringMatching(/^facturas\/impuestos\.py:\d+ $/));
  });

  it("opens the history with commits and tasks, and compares a commit inline", async () => {
    const { user } = setup(S5, { path: "facturas/impuestos.py" });
    await user.click(await screen.findByRole("button", { name: "Historial" }));
    const drawer = await screen.findByRole("complementary", { name: "Historial" });
    const compare = await within(drawer).findAllByRole("button", { name: /^Comparar .* con la versión actual$/ });
    await user.click(compare[0]!);
    expect(await screen.findByText(/^Cambios desde .* hasta la versión actual$/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Cerrar la comparación" }));
    expect(await screen.findByRole("textbox", { name: "Contenido de impuestos.py" })).toBeInTheDocument();
  });
});

describe("viewer models", () => {
  it("marks added, modified and deleted lines from hunks", () => {
    const marks = lineMarksFromHunks([
      {
        header: "@@",
        lines: [
          { kind: "context", oldLine: 1, newLine: 1, text: "a" },
          { kind: "removed", oldLine: 2, newLine: null, text: "b" },
          { kind: "added", oldLine: null, newLine: 2, text: "B" },
          { kind: "added", oldLine: null, newLine: 3, text: "C" },
          { kind: "context", oldLine: 3, newLine: 4, text: "d" },
          { kind: "added", oldLine: null, newLine: 5, text: "e" },
          { kind: "context", oldLine: 4, newLine: 6, text: "f" },
          { kind: "removed", oldLine: 5, newLine: null, text: "g" },
          { kind: "context", oldLine: 6, newLine: 7, text: "h" },
        ],
      },
    ]);
    expect(Object.fromEntries(marks)).toEqual({ 2: "modified", 3: "modified", 5: "added", 7: "deleted" });
  });

  it("merges commits and tasks newest first, an accepted task on its commit", () => {
    const history: FileHistory = {
      commits: [
        { hash: "c2", short: "c2", author: "Ana", date: 200, subject: "Dos", path: "a.py", task: "t1" },
        { hash: "c1", short: "c1", author: "Ana", date: 100, subject: "Uno", path: "a.py", task: null },
      ],
      tasks: [
        { task: "t1", title: "Hecha", status: "accepted", at: 200, commit: "c2", open: false },
        { task: "t2", title: "Abierta", status: "review", at: 300, commit: null, open: true },
      ],
    };
    const merged = mergeHistory(history);
    expect(merged.map((e) => (e.kind === "commit" ? e.commit.hash : e.task.task))).toEqual(["t2", "c2", "c1"]);
    expect(merged[1]?.kind === "commit" && merged[1].task?.title).toBe("Hecha");
  });
});
