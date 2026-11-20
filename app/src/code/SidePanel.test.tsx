import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useEffect } from "react";
import { describe, expect, it, vi } from "vitest";

import type { Task } from "../api/types";
import { useFileNavigator, type FileNavigator } from "../files/navigator";
import { relativeToCopy } from "./copies";
import { panelShortcut } from "./keys";
import { defaultPanelWidth, MIN_CHAT, MIN_PANEL, SidePanel } from "./SidePanel";
import { renderCode, sampleApi } from "./testing";

vi.mock("../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

const PROJECT = "/code/hedy";

/** Lets a test call the navigator like the rest of the app does. */
let navigator: FileNavigator | null = null;
function NavigatorHandle() {
  const nav = useFileNavigator();
  useEffect(() => {
    navigator = nav;
  });
  return null;
}

async function setup(taskId: string | null, options: { onRequestOpen?: () => void } = {}) {
  const api = sampleApi();
  const tasks = await api.tasks(PROJECT);
  const task = (tasks.find((t) => t.id === taskId) ?? null) as Task | null;
  const view = renderCode(
    <div className="workspace">
      <NavigatorHandle />
      <SidePanel
        project={PROJECT}
        task={task}
        onRequestOpen={options.onRequestOpen}
        review={<p>Contenido de la revisión</p>}
        preview={<p>Vista previa del proyecto</p>}
      />
    </div>,
    api,
  );
  return { ...view, task, user: userEvent.setup() };
}

describe("SidePanel", () => {
  it("shows four tabs, Cambios first with the task's count, and each view from its tab", async () => {
    const { api, user } = await setup("s5");
    const count = (await api.taskDiff("s5")).length;
    expect(count).toBeGreaterThan(0);
    const changes = await screen.findByRole("tab", { name: `Cambios ${count}` });
    expect(screen.getAllByRole("tab").map((tab) => tab.textContent)).toEqual([`Cambios ${count}`, "Archivos", "Terminal", "Vista previa"]);
    expect(changes).toHaveAttribute("aria-selected", "true");
    expect(changes.querySelector(".tabbar__dot")).toBeNull();
    expect(screen.getByText("Contenido de la revisión")).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Archivos" }));
    expect(await screen.findByRole("tree", { name: "Archivos" })).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Qué copia se muestra" })).toHaveTextContent(/Tarea: Arregla el cálculo/);
    await user.click(screen.getByRole("tab", { name: "Vista previa" }));
    expect(screen.getByText("Vista previa del proyecto")).toBeInTheDocument();
  });

  it("names the changes tab without a count when there is nothing to review", async () => {
    await setup(null);
    expect(screen.getAllByRole("tab").map((tab) => tab.textContent)).toEqual(["Cambios", "Archivos", "Terminal", "Vista previa"]);
  });

  it("opens the files on the folder when there is no task, with the tab's shortcut in its tooltip", async () => {
    await setup(null);
    const tab = screen.getByRole("tab", { name: "Archivos" });
    expect(tab).toHaveAttribute("aria-selected", "true");
    expect(tab).toHaveAttribute("title", "Archivos (Ctrl+Shift+E)");
    expect(await screen.findByText("Tu carpeta")).toBeInTheDocument();
  });

  it("opens a file asked through the navigator in the viewer, and a diff in the review", async () => {
    const onRequestOpen = vi.fn();
    await setup("s5", { onRequestOpen });
    act(() => navigator?.open({ copy: { kind: "task", task: "s5" }, path: "facturas/impuestos.py", line: 16 }));
    expect(await screen.findByRole("textbox", { name: "Contenido de impuestos.py" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Archivos" })).toHaveAttribute("aria-selected", "true");
    expect(onRequestOpen).toHaveBeenCalled();

    act(() => navigator?.open({ copy: { kind: "task", task: "s5" }, path: "facturas/totales.py", view: "diff" }));
    await waitFor(() => expect(screen.getByRole("tab", { name: /^Cambios/ })).toHaveAttribute("aria-selected", "true"));
  });

  it("compares a folder file with HEAD when its diff is asked (there is no review there)", async () => {
    await setup("s5");
    act(() => navigator?.open({ copy: { kind: "folder", project: PROJECT }, path: "README.md", view: "diff" }));
    expect(await screen.findByText(/^Cambios desde HEAD hasta la versión actual$/)).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Qué copia se muestra" })).toHaveTextContent("Tu carpeta");
  });

  it("searches inside Archivos, behind a switch between the tree and the search", async () => {
    const { user } = await setup(null);
    expect(screen.queryByRole("tab", { name: "Buscar" })).not.toBeInTheDocument();
    const mode = await screen.findByRole("radiogroup", { name: "Archivos" });
    const tree = within(mode).getByRole("radio", { name: "Árbol" });
    const search = within(mode).getByRole("radio", { name: "Buscar · Ctrl+Shift+F" });
    expect(tree).toHaveAttribute("aria-checked", "true");
    expect(await screen.findByRole("tree", { name: "Archivos" })).toBeVisible();

    await user.click(search);
    expect(search).toHaveAttribute("aria-checked", "true");
    await waitFor(() => expect(screen.getByRole("searchbox", { name: "Buscar en los archivos" })).toHaveFocus());
    // Still the Archivos tab, now showing the search.
    expect(screen.getByRole("tab", { name: "Archivos" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tree", { name: "Archivos", hidden: true })).not.toBeVisible();

    await user.click(tree);
    expect(tree).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("tree", { name: "Archivos" })).toBeVisible();
  });

  it("focuses the files, the search and the review with Ctrl+Shift+E / F / G", async () => {
    const onRequestOpen = vi.fn();
    await setup("s5", { onRequestOpen });
    fireEvent.keyDown(document.body, { key: "E", code: "KeyE", ctrlKey: true, shiftKey: true });
    await waitFor(() => expect(screen.getByRole("tree", { name: "Archivos" })).toHaveFocus());
    expect(screen.getByRole("radio", { name: "Árbol" })).toHaveAttribute("aria-checked", "true");
    fireEvent.keyDown(document.body, { key: "F", code: "KeyF", ctrlKey: true, shiftKey: true });
    await waitFor(() => expect(screen.getByRole("searchbox", { name: "Buscar en los archivos" })).toHaveFocus());
    expect(screen.getByRole("tab", { name: "Archivos" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("radio", { name: "Buscar · Ctrl+Shift+F" })).toHaveAttribute("aria-checked", "true");
    fireEvent.keyDown(document.body, { key: "G", code: "KeyG", ctrlKey: true, shiftKey: true });
    await waitFor(() => expect(screen.getByRole("tab", { name: /^Cambios/ })).toHaveAttribute("aria-selected", "true"));
    expect(onRequestOpen).toHaveBeenCalledTimes(3);
  });

  it("keeps the search query when switching tabs", async () => {
    const { user } = await setup("s5");
    await user.click(screen.getByRole("tab", { name: "Archivos" }));
    await user.click(await screen.findByRole("radio", { name: "Buscar · Ctrl+Shift+F" }));
    await user.type(screen.getByRole("searchbox", { name: "Buscar en los archivos" }), "IVA");
    // Clicking Archivos again while searching stays on the search.
    await user.click(screen.getByRole("tab", { name: "Archivos" }));
    expect(screen.getByRole("radio", { name: "Buscar · Ctrl+Shift+F" })).toHaveAttribute("aria-checked", "true");
    await user.click(screen.getByRole("tab", { name: /^Cambios/ }));
    await user.click(screen.getByRole("tab", { name: "Archivos" }));
    await user.click(screen.getByRole("radio", { name: "Buscar · Ctrl+Shift+F" }));
    expect(screen.getByRole("searchbox", { name: "Buscar en los archivos" })).toHaveValue("IVA");
  });

  it("resizes from the keyboard, remembers the width and maximizes", async () => {
    // A 1200 px workspace (jsdom has no layout).
    vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockImplementation(function (this: HTMLElement) {
      return this.classList.contains("workspace") ? 1200 : 0;
    });
    const { unmount } = await setup("s5");
    const edge = screen.getByRole("separator", { name: "Ancho del panel lateral" });
    const panel = screen.getByRole("complementary");
    const start = Number(edge.getAttribute("aria-valuenow"));
    edge.focus();
    fireEvent.keyDown(edge, { key: "ArrowLeft" });
    expect(Number(edge.getAttribute("aria-valuenow"))).toBe(start + 16);
    expect(panel.style.width).toBe(`${start + 16}px`);
    fireEvent.keyDown(edge, { key: "ArrowRight", shiftKey: true });
    expect(Number(edge.getAttribute("aria-valuenow"))).toBe(Math.max(MIN_PANEL, start + 16 - 64));
    const remembered = edge.getAttribute("aria-valuenow");
    unmount();
    await setup("s5");
    expect(screen.getByRole("separator", { name: "Ancho del panel lateral" })).toHaveAttribute("aria-valuenow", remembered);
    const again = screen.getByRole("separator", { name: "Ancho del panel lateral" });
    fireEvent.doubleClick(again);
    expect(again).toHaveAttribute("aria-valuenow", again.getAttribute("aria-valuemax"));
    expect(again).toHaveAttribute("aria-valuemax", String(1200 - MIN_CHAT));
    fireEvent.keyDown(again, { key: "Enter" });
    expect(again).toHaveAttribute("aria-valuenow", remembered);
    vi.restoreAllMocks();
  });
});

describe("panel helpers", () => {
  it("sizes the panel within the workspace and reads the shortcuts", () => {
    expect(defaultPanelWidth(1200)).toBe(480);
    expect(defaultPanelWidth(700)).toBe(MIN_PANEL);
    expect(defaultPanelWidth(3000)).toBe(720);
    expect(defaultPanelWidth(1000)).toBeLessThanOrEqual(1000 - MIN_CHAT);
    const key = (init: KeyboardEventInit) => new KeyboardEvent("keydown", init);
    expect(panelShortcut(key({ key: "E", code: "KeyE", ctrlKey: true, shiftKey: true }))).toBe("explorer");
    expect(panelShortcut(key({ key: "f", metaKey: true, shiftKey: true }))).toBe("search");
    expect(panelShortcut(key({ key: "g", code: "KeyG", ctrlKey: true }))).toBeNull();
  });

  it("turns a path an agent reported into one of the copy", () => {
    expect(relativeToCopy("./a/b.py", [])).toBe("a/b.py");
    expect(relativeToCopy("/wt/s5/a/b.py", ["/wt/s5", "/code/hedy"])).toBe("a/b.py");
    expect(relativeToCopy("/elsewhere/b.py", ["/wt/s5"])).toBeNull();
  });
});

