import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { EditorView } from "@codemirror/view";
import { useEffect, type ReactElement } from "react";
import { describe, expect, it, vi } from "vitest";

import type { CodeCopy } from "../../api/codeTypes";
import type { Task } from "../../api/types";
import { useFileNavigator, type FileNavigator } from "../../files/navigator";
import { SidePanel } from "../SidePanel";
import { renderCode, sampleApi } from "../testing";
import { fileLock } from "./editability";
import { useUnsavedChanges, type UnsavedChanges } from "./unsaved";

vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

const PROJECT = "/code/hedy";
const S5: CodeCopy = { kind: "task", task: "s5" };
const FILE = "facturas/impuestos.py";

let nav: FileNavigator | null = null;
let unsaved: UnsavedChanges | null = null;
function Handles() {
  const n = useFileNavigator();
  const u = useUnsavedChanges();
  useEffect(() => {
    nav = n;
    unsaved = u;
  });
  return null;
}

async function setup(patch: Partial<Task> = {}) {
  const api = sampleApi();
  const s5 = (await api.tasks(PROJECT)).find((t) => t.id === "s5") as Task;
  const panel = (task: Task): ReactElement => (
    <div className="workspace">
      <Handles />
      <SidePanel project={PROJECT} task={task} review={<p>Revisión</p>} preview={<p>Vista previa</p>} />
    </div>
  );
  const task = { ...s5, ...patch };
  const view = renderCode(panel(task), api);
  return { ...view, task, setTask: (next: Task) => view.rerender(panel(next)), user: userEvent.setup() };
}

/** Opens a file of task s5's copy through the navigator, like any link. */
async function open(path = FILE, extra: { edit?: boolean; line?: number } = {}) {
  act(() => nav?.open({ copy: S5, path, ...extra }));
  return screen.findByRole("textbox", { name: `Contenido de ${path.split("/").pop()}` });
}

/** CodeMirror's view, to type as the user would (jsdom has no real text input). */
function editorView(): EditorView {
  const el = document.querySelector(".cm-editor");
  const view = el && EditorView.findFromDOM(el as HTMLElement);
  if (!view) throw new Error("no editor");
  return view;
}

function typeAtStart(text: string) {
  act(() => editorView().dispatch({ changes: { from: 0, insert: text } }));
}

const viewer = () => document.querySelector(".viewer") as HTMLElement;
const press = (key: string) => fireEvent.keyDown(viewer(), { key, ctrlKey: true });

async function diskText(api: ReturnType<typeof sampleApi>, path = FILE) {
  const content = await api.readFile(S5, path);
  return content.kind === "text" ? content.text : null;
}

describe("Editing in the viewer", () => {
  it("turns editing on from the pencil, saves with Ctrl+S and refreshes the task's review", async () => {
    const { api, user } = await setup();
    const editor = await open();
    expect(editor).toHaveAttribute("aria-readonly", "true");
    await user.click(await screen.findByRole("button", { name: /^Editar \(/ }));
    await waitFor(() => expect(editor).toHaveAttribute("aria-readonly", "false"));
    expect(screen.getByRole("button", { name: /^Dejar de editar/ })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByText(/tus cambios se suman a los suyos/)).toBeInTheDocument();

    typeAtStart("# Revisado a mano\n");
    expect(await screen.findByText("Cambios sin guardar")).toBeInTheDocument();
    const crumbs = screen.getByRole("navigation", { name: "Ruta del archivo" });
    expect(within(crumbs).getByTitle("Cambios sin guardar")).toBeInTheDocument();
    // The explorer's row carries the dot too.
    expect(within(screen.getByRole("tree", { name: "Archivos" })).getByRole("treeitem", { name: /impuestos\.py.*Cambios sin guardar/ })).toBeInTheDocument();

    press("s");
    expect(await screen.findByText("Guardado")).toBeInTheDocument();
    expect(await diskText(api)).toMatch(/^# Revisado a mano\n"""Impuestos/);
    // The user's edit is part of the task's changes now.
    const diff = await api.taskDiff("s5");
    const file = diff.find((f) => f.path === FILE);
    expect(file?.hunks.flatMap((h) => h.lines).some((l) => l.kind === "added" && l.text === "# Revisado a mano")).toBe(true);
    expect(screen.queryByText("Cambios sin guardar")).toBeNull();
    // Still editing: the history and caret stay.
    expect(editor).toHaveAttribute("aria-readonly", "false");
  });

  it("toggles with Ctrl+E, starts with a double click, and discards back to the file", async () => {
    const { api, user } = await setup();
    const editor = await open();
    press("e");
    await waitFor(() => expect(editor).toHaveAttribute("aria-readonly", "false"));
    press("e");
    await waitFor(() => expect(editor).toHaveAttribute("aria-readonly", "true"));
    fireEvent.doubleClick(editor);
    await waitFor(() => expect(editor).toHaveAttribute("aria-readonly", "false"));

    const before = await diskText(api);
    typeAtStart("borrar esto\n");
    await user.click(await screen.findByRole("button", { name: "Descartar cambios" }));
    expect(editorView().state.doc.toString()).toBe(before);
    expect(screen.queryByText("Cambios sin guardar")).toBeNull();
    expect(await diskText(api)).toBe(before);
  });

  it("does not overwrite a file that changed on disk: reload, compare or overwrite", async () => {
    const { api, user } = await setup();
    await open(FILE, { edit: true });
    await waitFor(() => expect(screen.getByRole("textbox", { name: "Contenido de impuestos.py" })).toHaveAttribute("aria-readonly", "false"));
    typeAtStart("# mío\n");
    // Meanwhile someone else (the agent) saves the file.
    api.code.setText(S5, FILE, "IVA = 0\n");
    await user.click(await screen.findByRole("button", { name: "Guardar" }));

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("El archivo cambió mientras lo editabas.");
    expect(await diskText(api)).toBe("IVA = 0\n");
    await user.click(within(alert).getByRole("button", { name: "Comparar" }));
    expect(await screen.findByText("− en el disco · + tus cambios")).toBeInTheDocument();
    await user.click(within(alert).getByRole("button", { name: "Ocultar comparación" }));
    await user.click(within(alert).getByRole("button", { name: "Sobrescribir" }));
    await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
    expect(await diskText(api)).toMatch(/^# mío\n"""Impuestos/);

    // Again, now reloading the version on disk.
    typeAtStart("# otra vez\n");
    api.code.setText(S5, FILE, "IVA = 1\n");
    press("s");
    await user.click(await screen.findByRole("button", { name: "Recargar" }));
    expect(editorView().state.doc.toString()).toBe("IVA = 1\n");
    expect(screen.queryByText("Cambios sin guardar")).toBeNull();
  });

  it("asks before leaving unsaved changes: another file, closing it, closing the window", async () => {
    const { api, user } = await setup();
    await open(FILE, { edit: true });
    await waitFor(() => expect(screen.getByRole("textbox", { name: "Contenido de impuestos.py" })).toHaveAttribute("aria-readonly", "false"));
    typeAtStart("# pendiente\n");
    await screen.findByText("Cambios sin guardar");
    expect(unsaved?.dirty).toBe(true);

    // Another file: Cancelar keeps this one.
    act(() => nav?.open({ copy: S5, path: "README.md" }));
    let dialog = await screen.findByRole("dialog", { name: "¿Guardar los cambios de impuestos.py?" });
    await user.click(within(dialog).getByRole("button", { name: "Cancelar" }));
    expect(screen.getByRole("textbox", { name: "Contenido de impuestos.py" })).toBeInTheDocument();

    // Closing the window asks too.
    const closing = api.requestClose();
    dialog = await screen.findByRole("dialog", { name: "¿Guardar los cambios de impuestos.py?" });
    await user.click(within(dialog).getByRole("button", { name: "Cancelar" }));
    expect(await closing).toBe(false);

    // Closing the file: Guardar saves, then it closes.
    await user.click(screen.getByRole("button", { name: "Cerrar el archivo" }));
    dialog = await screen.findByRole("dialog", { name: "¿Guardar los cambios de impuestos.py?" });
    await user.click(within(dialog).getByRole("button", { name: "Guardar" }));
    await waitFor(() => expect(screen.queryByRole("textbox", { name: "Contenido de impuestos.py" })).toBeNull());
    expect(await diskText(api)).toMatch(/^# pendiente\n/);
    expect(unsaved?.dirty).toBe(false);
    expect(await api.requestClose()).toBe(true);

    // Descartar: the other file opens and nothing is written.
    await open(FILE, { edit: true });
    await waitFor(() => expect(screen.getByRole("textbox", { name: "Contenido de impuestos.py" })).toHaveAttribute("aria-readonly", "false"));
    typeAtStart("# tirar\n");
    await screen.findByText("Cambios sin guardar");
    act(() => nav?.open({ copy: S5, path: "README.md" }));
    dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Descartar" }));
    expect(await screen.findByRole("textbox", { name: "Contenido de README.md" })).toBeInTheDocument();
    expect(await diskText(api)).not.toMatch(/# tirar/);
  });

  it("locks while the agent works on the copy and unlocks live when it finishes", async () => {
    const { task, setTask, user } = await setup({ status: "running" });
    const editor = await open();
    await user.click(await screen.findByRole("button", { name: /^Editar \(/ }));
    expect(await screen.findByText("La IA está trabajando en esta copia; podrás editar cuando termine")).toBeInTheDocument();
    expect(editor).toHaveAttribute("aria-readonly", "true");
    setTask({ ...task, status: "review" });
    await waitFor(() => expect(screen.queryByText(/La IA está trabajando en esta copia/)).toBeNull());
    await waitFor(() => expect(screen.getByRole("textbox", { name: "Contenido de impuestos.py" })).toHaveAttribute("aria-readonly", "false"));
  });

  it("adds the blame gutter only while blame is on, so it takes no width otherwise", async () => {
    const { user } = await setup();
    await open();
    expect(document.querySelector(".cm-blameGutter")).toBeNull();
    const blame = screen.getByRole("button", { name: "Blame: quién cambió cada línea" });
    await user.click(blame);
    await waitFor(() => expect(document.querySelector(".cm-blameGutter")).not.toBeNull());
    await user.click(blame);
    await waitFor(() => expect(document.querySelector(".cm-blameGutter")).toBeNull());
  });

  it("explains why a snapshot, a protected file or another revision cannot be edited", async () => {
    const info = { live: false, agentWorking: false } as Parameters<typeof fileLock>[0];
    const text = { kind: "text", text: "x", size: 1, version: "v" } as const;
    expect(fileLock(info, text, {})).toBe("snapshot");
    const live = { ...info, live: true } as Parameters<typeof fileLock>[0];
    expect(fileLock(live, text, {})).toBeNull();
    expect(fileLock(live, text, { rev: "base" })).toBe("revision");
    expect(fileLock(live, { kind: "locked" }, {})).toBe("protected");
    expect(fileLock(live, { kind: "binary", size: 3 }, {})).toBe("notText");
    expect(fileLock(live, text, { comparing: true })).toBe("compare");
    expect(fileLock({ ...live, agentWorking: true } as Parameters<typeof fileLock>[0], text, {})).toBe("working");

    // In the viewer: an accepted task's snapshot says so when the pencil is clicked.
    const { user } = await setup();
    act(() => nav?.open({ copy: S5, path: FILE, rev: "base" }));
    await screen.findByText(/Antes de la IA · como estaba en/);
    await user.click(screen.getByRole("button", { name: /Es otra versión del archivo/ }));
    expect(await screen.findByText("Es otra versión del archivo: vuelve a la actual para editar")).toBeInTheDocument();
  });
});
