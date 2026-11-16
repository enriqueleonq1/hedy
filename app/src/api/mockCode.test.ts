import { describe, expect, it } from "vitest";

import { MockApi } from "./MockApi";
import { isProtectedPath, normalizeCodePath, textDiff } from "./mockCode";
import type { CodeCopy } from "./codeTypes";

const HEDY: CodeCopy = { kind: "folder", project: "/code/hedy" };
const S5: CodeCopy = { kind: "task", task: "s5" };

function api(): MockApi {
  return MockApi.withSampleData().withSampleActivity();
}

describe("MockApi code navigation", () => {
  it("lists the folder with its own changes and protected files locked", async () => {
    const tree = await api().codeTree(HEDY);
    expect(tree.copy).toMatchObject({ kind: "folder", root: "/code/hedy", live: true, base: "a3f9c1e", branch: "main" });
    expect(tree.entries.length).toBeGreaterThanOrEqual(20);
    const byPath = new Map(tree.entries.map((e) => [e.path, e]));
    expect(byPath.get("README.md")?.status).toBe("modified");
    expect(byPath.get("docs/borrador.md")?.status).toBe("untracked");
    expect(byPath.get(".env.example")?.locked).toBe(true);
    expect(byPath.has(".env")).toBe(false);
    expect([...byPath.keys()]).toEqual([...byPath.keys()].sort());

    const all = await api().codeTree(HEDY, { ignored: true });
    expect(all.entries.find((e) => e.path === ".env")).toMatchObject({ ignored: true, locked: true });
  });

  it("reads a task's worktree against its base, consistent with its review diff", async () => {
    const mock = api();
    const tree = await mock.codeTree(S5);
    expect(tree.copy).toMatchObject({ kind: "task", task: "s5", root: "/code/hedy/.hedy/worktrees/s5", base: "a3f9c1e" });
    const changed = tree.entries.filter((e) => e.status).map((e) => [e.path, e.status]);
    expect(changed).toEqual([
      ["facturas/impuestos.py", "modified"],
      ["facturas/totales.py", "modified"],
      ["tests/test_facturas.py", "modified"],
    ]);
    const review = await mock.taskDiff("s5");
    expect(review.map((d) => [d.path, d.additions, d.deletions])).toEqual([
      ["facturas/impuestos.py", 3, 1],
      ["facturas/totales.py", 3, 1],
      ["tests/test_facturas.py", 3, 1],
    ]);

    const diff = await mock.fileDiff(S5, "facturas/impuestos.py");
    expect(diff.status).toBe("modified");
    expect(diff.against).toBe("a3f9c1e");
    if (diff.body.kind !== "text") throw new Error("expected a text diff");
    expect(diff.body.hunks).toEqual(review[0]?.hunks);
    const removed = diff.body.hunks[0]?.lines.find((l) => l.kind === "removed");
    expect(removed).toMatchObject({ oldLine: 16, text: "    return precio * cantidad * IVA" });

    const full = await mock.fileDiff(S5, "facturas/impuestos.py", { full: true });
    if (full.body.kind !== "text") throw new Error("expected a text diff");
    expect(full.body.hunks).toHaveLength(1);
    expect(full.body.hunks[0]?.lines[0]).toMatchObject({ kind: "context", oldLine: 1, newLine: 1 });

    const before = await mock.readFile(S5, "facturas/impuestos.py", "base");
    const after = await mock.readFile(S5, "facturas/impuestos.py");
    expect(before.kind === "text" && before.text).toContain("return precio * cantidad * IVA");
    expect(after.kind === "text" && after.text).toContain("return redondear(neto * IVA)");
    const same = await mock.fileDiff(S5, "citas/modelos.py");
    expect(same).toMatchObject({ status: null, body: { kind: "text", hunks: [] } });
  });

  it("says why a file cannot be shown", async () => {
    const mock = api();
    expect(await mock.readFile(HEDY, "static/logo.png")).toMatchObject({ kind: "image", mime: "image/png" });
    expect(await mock.readFile(HEDY, "static/fuentes/Inter.woff2")).toEqual({ kind: "binary", size: 48_212 });
    expect(await mock.readFile(HEDY, "data/clientes_demo.csv")).toEqual({ kind: "tooLarge", size: 3_412_880 });
    expect(await mock.readFile(HEDY, ".env.example")).toEqual({ kind: "locked" });
    expect(await mock.readFile(HEDY, "no/existe.py")).toEqual({ kind: "missing" });
    expect(await mock.readFile(HEDY, "docs/borrador.md", "base")).toEqual({ kind: "missing" });
    expect(await mock.fileDiff(HEDY, ".env.example")).toMatchObject({ body: { kind: "locked" } });
    await expect(mock.readFile(HEDY, "../tienda/README.md")).rejects.toMatch(/no es un archivo/);
    await expect(mock.readFile(HEDY, "README.md", "--output=x")).rejects.toMatch(/no es un commit/);
    await mock.discardTask("s5");
    await expect(mock.codeTree(S5)).rejects.toMatch(/ya no están disponibles/);
    await expect(mock.codeTree({ kind: "folder", project: "/code/otro" })).rejects.toMatch(/no existe/);
  });

  it("searches text files with options, never protected ones", async () => {
    const mock = api();
    const results = await mock.searchText(HEDY, "iva", {});
    expect(results.truncated).toBe(false);
    expect(results.files.map((f) => f.path)).toContain("facturas/impuestos.py");
    const line = results.files.find((f) => f.path === "facturas/impuestos.py")?.matches[0];
    expect(line?.text.slice(line.ranges[0]?.start, line.ranges[0]?.end).toLowerCase()).toBe("iva");

    const exact = await mock.searchText(HEDY, "IVA", { caseSensitive: true, wholeWord: true, glob: "*.py" });
    expect(exact.files.every((f) => f.path.endsWith(".py"))).toBe(true);
    const inTask = await mock.searchText(S5, "redondear\\(neto", { regex: true });
    expect(inTask.total).toBe(1);
    expect((await mock.searchText(HEDY, "   ")).total).toBe(0);
    await expect(mock.searchText(HEDY, "(", { regex: true })).rejects.toMatch(/no válida/);
  });

  it("gives history with tasks, blame with uncommitted lines and symbols", async () => {
    const mock = api();
    const history = await mock.fileHistory(S5, "facturas/impuestos.py");
    expect(history.commits.map((c) => c.subject)).toEqual(["Facturas con IVA"]);
    expect(history.tasks).toEqual([expect.objectContaining({ task: "s5", open: true, commit: null })]);

    const blame = await mock.blame(S5, "facturas/impuestos.py");
    const lines = blame.ranges.reduce((n, r) => n + r.count, 0);
    expect(lines).toBe(23);
    const uncommitted = blame.commits.find((c) => c.uncommitted);
    expect(blame.ranges.filter((r) => r.commit === uncommitted?.hash).map((r) => [r.start, r.count])).toEqual([[16, 3]]);
    const range = await mock.blame(HEDY, "citas/servicio.py", { lines: { start: 2, end: 3 } });
    expect(range.ranges.reduce((n, r) => n + r.count, 0)).toBe(2);
    await expect(mock.blame(HEDY, ".env.example")).rejects.toMatch(/protegido/);

    expect((await mock.fileSymbols(HEDY, "facturas/impuestos.py")).map((s) => s.name)).toEqual([
      "redondear",
      "iva_de_linea",
      "iva_total",
    ]);
    const ts = await mock.fileSymbols({ kind: "folder", project: "/code/tienda" }, "src/carrito/carrito.ts");
    expect(ts.map((s) => [s.name, s.kind])).toEqual([
      ["Carrito", "class"],
      ["agregar", "method"],
      ["quitar", "method"],
      ["subtotal", "method"],
      ["total", "method"],
    ]);
    expect((await mock.fileSymbols(HEDY, "README.md")).map((s) => s.name)).toEqual([
      "Hedy Citas",
      "Instalar",
      "Variables",
      "Estructura",
    ]);
  });

  it("opens the editor and the file manager inside the copy", async () => {
    const mock = api();
    await mock.openFileInEditor(S5, "facturas/impuestos.py", 16);
    await mock.revealInFileManager(HEDY, "facturas/impuestos.py");
    expect(mock.code.opened).toEqual([{ path: "/code/hedy/.hedy/worktrees/s5/facturas/impuestos.py", line: 16 }]);
    expect(mock.code.revealed).toEqual(["/code/hedy/facturas"]);
    await expect(mock.openFileInEditor(HEDY, "/etc/passwd")).rejects.toMatch(/no es un archivo/);
  });
});

describe("code helpers", () => {
  it("keeps paths inside the copy and knows protected names", () => {
    expect(normalizeCodePath("./src//app.py")).toBe("src/app.py");
    for (const bad of ["", "/etc/passwd", "../x", "a/../../b", ".git/config", "C:/x", "~/.ssh"]) {
      expect(normalizeCodePath(bad)).toBeNull();
    }
    expect(isProtectedPath("config/.env.production")).toBe(true);
    expect(isProtectedPath("deploy/server.pem")).toBe(true);
    expect(isProtectedPath("src/env.ts")).toBe(false);
  });

  it("diffs texts into numbered hunks", () => {
    const diff = textDiff("a.txt", "1\n2\n3\n4\n5\n6\n7\n8\n9\n", "1\n2\n3\n4\nCINCO\n6\n7\n8\n9\n", 1);
    expect(diff?.hunks).toEqual([
      {
        header: "@@ -4,3 +4,3 @@",
        lines: [
          { kind: "context", oldLine: 4, newLine: 4, text: "4" },
          { kind: "removed", oldLine: 5, newLine: null, text: "5" },
          { kind: "added", oldLine: null, newLine: 5, text: "CINCO" },
          { kind: "context", oldLine: 6, newLine: 6, text: "6" },
        ],
      },
    ]);
    expect(textDiff("n.txt", null, "a\n")?.status).toBe("added");
    expect(textDiff("n.txt", "a\n", "a\n")).toBeNull();
  });
});
