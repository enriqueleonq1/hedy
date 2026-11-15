import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { CodeCopy } from "../../api/codeTypes";
import { useComposerInsert } from "../../screens/workspace/composerBridge";
import { renderCode } from "../testing";
import { Explorer } from "./Explorer";
import { rollUpStatus } from "./explorerModel";

const S5: CodeCopy = { kind: "task", task: "s5" };
const HEDY: CodeCopy = { kind: "folder", project: "/code/hedy" };

function Composer({ onInsert }: { onInsert: (text: string) => void }) {
  useComposerInsert(onInsert);
  return null;
}

function setup(copy: CodeCopy, props: Partial<Parameters<typeof Explorer>[0]> = {}) {
  const onOpen = vi.fn();
  const onShowDiff = vi.fn();
  const onHistory = vi.fn();
  const inserted = vi.fn();
  const view = renderCode(
    <>
      <Composer onInsert={inserted} />
      <Explorer copy={copy} info={undefined} openPath={null} onOpen={onOpen} onShowDiff={onShowDiff} onHistory={onHistory} {...props} />
    </>,
  );
  return { ...view, onOpen, onShowDiff, onHistory, inserted, user: userEvent.setup() };
}

describe("Explorer", () => {
  it("shows only a task's changes by default, with their status, and all files on demand", async () => {
    const { user } = setup(S5);
    const tree = await screen.findByRole("tree", { name: "Archivos" });
    expect(within(tree).getByRole("treeitem", { name: /impuestos\.py/ })).toHaveTextContent("M");
    expect(within(tree).queryByRole("treeitem", { name: /README/ })).toBeNull();
    const filter = screen.getByRole("button", { name: "Solo cambiados" });
    expect(filter).toHaveAttribute("aria-pressed", "true");
    await user.click(filter);
    expect(await screen.findByRole("treeitem", { name: /README\.md/ })).toBeInTheDocument();
    expect(filter).toHaveAttribute("aria-pressed", "false");
  });

  it("shows the whole folder by default, ignored files on demand, and locks protected files", async () => {
    const { user } = setup(HEDY);
    expect(await screen.findByRole("treeitem", { name: /README\.md/ })).toBeInTheDocument();
    expect(screen.getByRole("treeitem", { name: /\.env\.example.*protegido/ })).toBeInTheDocument();
    expect(screen.queryByRole("treeitem", { name: /^\.venv/ })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Mostrar ignorados" }));
    expect(await screen.findByRole("treeitem", { name: /^\.venv/ })).toBeInTheDocument();
  });

  it("opens files and runs context menu actions through the API and the composer", async () => {
    const { api, onOpen, onShowDiff, onHistory, inserted, user } = setup(S5);
    const row = await screen.findByRole("treeitem", { name: /impuestos\.py/ });
    await user.click(row);
    expect(onOpen).toHaveBeenCalledWith("facturas/impuestos.py");

    const menuFor = async () => {
      fireEvent.contextMenu(screen.getByRole("treeitem", { name: /impuestos\.py/ }));
      return screen.findByRole("menu", { name: "Acciones de impuestos.py" });
    };
    await user.click(within(await menuFor()).getByRole("menuitem", { name: "Abrir en VS Code" }));
    await waitFor(() => expect(api.code.opened.at(-1)?.path).toMatch(/facturas\/impuestos\.py$/));
    await user.click(within(await menuFor()).getByRole("menuitem", { name: "Mostrar en la carpeta" }));
    await waitFor(() => expect(api.code.revealed.at(-1)).toMatch(/facturas$/));
    await user.click(within(await menuFor()).getByRole("menuitem", { name: "Ver cambios" }));
    expect(onShowDiff).toHaveBeenCalledWith("facturas/impuestos.py");
    await user.click(within(await menuFor()).getByRole("menuitem", { name: "Historial" }));
    expect(onHistory).toHaveBeenCalledWith("facturas/impuestos.py");
    await user.click(within(await menuFor()).getByRole("menuitem", { name: "Preguntar a la IA sobre este archivo" }));
    expect(inserted).toHaveBeenCalledWith("@facturas/impuestos.py ");
  });

  it("opens the context menu from the keyboard and closes it with Escape", async () => {
    const { user } = setup(S5);
    const tree = await screen.findByRole("tree", { name: "Archivos" });
    tree.focus();
    fireEvent.keyDown(tree, { key: "ArrowDown" });
    fireEvent.keyDown(tree, { key: "ContextMenu" });
    const menu = await screen.findByRole("menu");
    expect(within(menu).getAllByRole("menuitem")[0]).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).toBeNull();
    expect(tree).toHaveFocus();
  });

  it("rolls a folder's status up from its files", () => {
    const entry = (path: string, status: "added" | "modified" | "deleted" | null) => ({
      path,
      status,
      oldPath: null,
      locked: false,
      ignored: false,
    });
    const rolled = rollUpStatus([entry("a/b/x.ts", "deleted"), entry("a/y.ts", "modified"), entry("c/z.ts", null)]);
    expect(rolled.get("a")).toBe("modified");
    expect(rolled.get("a/b")).toBe("deleted");
    expect(rolled.has("c")).toBe(false);
  });
});
