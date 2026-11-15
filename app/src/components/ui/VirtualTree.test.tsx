import { fireEvent, render, screen } from "@testing-library/react";
import { useMemo, useState } from "react";
import { describe, expect, it, vi } from "vitest";

import { ancestorsOf, allFolders, buildTree, flattenTree } from "./treeModel";
import { scrollFor, visibleRange } from "./VirtualList";
import { typeaheadMatch, VirtualTree } from "./VirtualTree";

const PATHS = ["src/app.ts", "src/api/client.ts", "src/api/types.ts", "README.md", "docs/guide.md", "node_modules/"];

describe("tree model", () => {
  it("builds folders first in natural order, with sizes, and flattens only open folders", () => {
    const root = buildTree(["b/file10.ts", "b/file2.ts", "a.ts", ...PATHS], (p) => p);
    expect(root.children.map((n) => n.name)).toEqual(["b", "docs", "node_modules", "src", "a.ts", "README.md"]);
    const b = root.children[0];
    expect(b?.kind === "folder" && b.children.map((n) => n.name)).toEqual(["file2.ts", "file10.ts"]);
    expect(root.size).toBe(8);
    expect(flattenTree(root, new Set()).map((r) => r.node.path)).toEqual(["b", "docs", "node_modules", "src", "a.ts", "README.md"]);
    const open = flattenTree(root, new Set(["src", "src/api"]));
    expect(open.map((r) => `${r.depth}:${r.node.name}`)).toContain("2:client.ts");
    expect(allFolders(root)).toEqual(["b", "docs", "node_modules", "src", "src/api"]);
    expect(ancestorsOf("a/b/c.py")).toEqual(["a", "a/b"]);
  });

  it("windows rows and scrolls just enough", () => {
    expect(visibleRange(100_000, 26, 26 * 5000, 520)).toEqual({ start: 4992, end: 5028 });
    expect(scrollFor(10, 20, 0, 100)).toBe(120);
    expect(scrollFor(2, 20, 0, 100)).toBeNull();
    expect(scrollFor(1, 20, 100, 100)).toBe(20);
  });
});

function Harness({ paths, onActivate, onMenu }: { paths: string[]; onActivate: (p: string) => void; onMenu?: (p: string) => void }) {
  const root = useMemo(() => buildTree(paths, (p) => p), [paths]);
  const [open, setOpen] = useState<Set<string>>(new Set());
  const rows = flattenTree(root, open);
  return (
    <VirtualTree<string>
      label="Archivos"
      rows={rows}
      expanded={open}
      onToggle={(path, on) =>
        setOpen((old) => {
          const next = new Set(old);
          if (on) next.add(path);
          else next.delete(path);
          return next;
        })
      }
      onActivate={(row) => onActivate(row.node.path)}
      onContextMenu={onMenu ? (row) => onMenu(row.node.path) : undefined}
      renderRow={(row) => <span>{row.node.name}</span>}
    />
  );
}

const active = () => {
  const tree = screen.getByRole("tree");
  const id = tree.getAttribute("aria-activedescendant");
  return id ? document.getElementById(id)?.textContent : null;
};

describe("VirtualTree", () => {
  it("moves, opens, closes and activates with the keyboard", () => {
    const onActivate = vi.fn();
    const onMenu = vi.fn();
    render(<Harness paths={PATHS} onActivate={onActivate} onMenu={onMenu} />);
    const tree = screen.getByRole("tree", { name: "Archivos" });
    tree.focus();
    expect(active()).toBe("docs");
    fireEvent.keyDown(tree, { key: "End" });
    expect(active()).toBe("README.md");
    fireEvent.keyDown(tree, { key: "Home" });
    fireEvent.keyDown(tree, { key: "ArrowDown" });
    fireEvent.keyDown(tree, { key: "ArrowDown" });
    expect(active()).toBe("src");
    fireEvent.keyDown(tree, { key: "ArrowRight" });
    expect(screen.getByRole("treeitem", { name: "src" })).toHaveAttribute("aria-expanded", "true");
    fireEvent.keyDown(tree, { key: "ArrowRight" });
    expect(active()).toBe("api");
    fireEvent.keyDown(tree, { key: "ArrowRight" });
    fireEvent.keyDown(tree, { key: "ArrowRight" });
    expect(active()).toBe("client.ts");
    expect(screen.getByRole("treeitem", { name: "client.ts" })).toHaveAttribute("aria-level", "3");
    fireEvent.keyDown(tree, { key: "Enter" });
    expect(onActivate).toHaveBeenCalledWith("src/api/client.ts");
    fireEvent.keyDown(tree, { key: "ArrowLeft" });
    expect(active()).toBe("api");
    fireEvent.keyDown(tree, { key: "ArrowLeft" });
    expect(screen.getByRole("treeitem", { name: "api" })).toHaveAttribute("aria-expanded", "false");
    fireEvent.keyDown(tree, { key: "F10", shiftKey: true });
    expect(onMenu).toHaveBeenCalledWith("src/api");
  });

  it("finds a row by typing its name", () => {
    render(<Harness paths={PATHS} onActivate={() => {}} />);
    const tree = screen.getByRole("tree");
    tree.focus();
    const now = vi.spyOn(Date, "now").mockReturnValue(1_000);
    fireEvent.keyDown(tree, { key: "r" });
    fireEvent.keyDown(tree, { key: "e" });
    expect(active()).toBe("README.md");
    // A pause starts a new search.
    now.mockReturnValue(5_000);
    fireEvent.keyDown(tree, { key: "s" });
    expect(active()).toBe("src");
    now.mockRestore();
    expect(typeaheadMatch(flattenTree(buildTree(PATHS, (p) => p), new Set()), 0, "zz")).toBe(-1);
  });

  it("renders only the rows in sight of a huge folder", () => {
    const many = Array.from({ length: 20_000 }, (_, i) => `big/file${i}.ts`);
    render(<Harness paths={many} onActivate={() => {}} />);
    const tree = screen.getByRole("tree");
    tree.focus();
    fireEvent.keyDown(tree, { key: "ArrowRight" });
    const rendered = screen.getAllByRole("treeitem").length;
    expect(rendered).toBeGreaterThan(10);
    expect(rendered).toBeLessThan(100);
    fireEvent.keyDown(tree, { key: "End" });
    expect(active()).toBe("file19999.ts");
  });
});
