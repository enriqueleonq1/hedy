// A line diff of two texts (longest common subsequence), for the mock's
// diffs and the editor's "comparar" in a save conflict. Common lines at both
// ends are skipped first, so a small edit in a big file stays cheap; a middle
// too big to compare is shown as replaced.

import type { DiffHunk } from "./codeTypes";
import type { DiffLine, FileDiff } from "./types";

/** Lines compared by LCS at most (their product), beyond which the middle is replaced. */
const MAX_LCS_CELLS = 4_000_000;

/** Splits text into lines like git (a final newline does not start a line). */
export function lines(text: string): string[] {
  if (text === "") return [];
  const all = text.split("\n");
  if (all[all.length - 1] === "") all.pop();
  return all;
}

export type Op = { kind: DiffLine["kind"]; oldLine: number | null; newLine: number | null; text: string };

/** Line diff by longest common subsequence (the sample files are small). */
function lcsOps(a: string[], b: string[]): Op[] {
  const n = a.length;
  const m = b.length;
  // lcs[i * (m + 1) + j]: common lines of a[i..] and b[j..].
  const lcs = new Uint32Array((n + 1) * (m + 1));
  const at = (i: number, j: number) => lcs[i * (m + 1) + j] ?? 0;
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      lcs[i * (m + 1) + j] = a[i] === b[j] ? at(i + 1, j + 1) + 1 : Math.max(at(i + 1, j), at(i, j + 1));
    }
  }
  const ops: Op[] = [];
  let i = 0;
  let j = 0;
  while (i < n || j < m) {
    if (i < n && j < m && a[i] === b[j]) {
      ops.push({ kind: "context", oldLine: i + 1, newLine: j + 1, text: a[i] ?? "" });
      i++;
      j++;
    } else if (i < n && (j === m || at(i + 1, j) >= at(i, j + 1))) {
      // Removed lines come first within a change, like git.
      ops.push({ kind: "removed", oldLine: i + 1, newLine: null, text: a[i] ?? "" });
      i++;
    } else {
      ops.push({ kind: "added", oldLine: null, newLine: j + 1, text: b[j] ?? "" });
      j++;
    }
  }
  return ops;
}

function hunks(ops: Op[], context: number): DiffHunk[] {
  const changed = ops.map((op, i) => (op.kind === "context" ? -1 : i)).filter((i) => i >= 0);
  const out: DiffHunk[] = [];
  let k = 0;
  while (k < changed.length) {
    let end = changed[k] ?? 0;
    const start = Math.max(0, end - context);
    while (k + 1 < changed.length && (changed[k + 1] ?? 0) - end <= 2 * context + 1) end = changed[++k] ?? end;
    end = Math.min(ops.length - 1, end + context);
    k++;
    const slice = ops.slice(start, end + 1);
    const oldLines = slice.filter((o) => o.kind !== "added");
    const newLines = slice.filter((o) => o.kind !== "removed");
    const firstOld = oldLines[0]?.oldLine ?? (ops.slice(0, start).filter((o) => o.kind !== "added").length);
    const firstNew = newLines[0]?.newLine ?? (ops.slice(0, start).filter((o) => o.kind !== "removed").length);
    out.push({
      header: `@@ -${firstOld},${oldLines.length} +${firstNew},${newLines.length} @@`,
      lines: slice.map(({ kind, oldLine, newLine, text }) => ({ kind, oldLine, newLine, text })),
    });
  }
  return out;
}

/** A unified diff of two texts (`null` = the file does not exist on that side). */
export function textDiff(path: string, before: string | null, after: string | null, context = 3): FileDiff | null {
  if (before === after) return null;
  const ops = diffOps(lines(before ?? ""), lines(after ?? ""));
  return {
    path,
    oldPath: null,
    status: before === null ? "added" : after === null ? "deleted" : "modified",
    additions: ops.filter((o) => o.kind === "added").length,
    deletions: ops.filter((o) => o.kind === "removed").length,
    binary: false,
    hunks: hunks(ops, context),
  };
}

/** Line operations from `a` to `b`, trimming the common start and end first. */
export function diffOps(a: string[], b: string[]): Op[] {
  let head = 0;
  while (head < a.length && head < b.length && a[head] === b[head]) head++;
  let tail = 0;
  while (tail < a.length - head && tail < b.length - head && a[a.length - 1 - tail] === b[b.length - 1 - tail]) tail++;
  const midA = a.slice(head, a.length - tail);
  const midB = b.slice(head, b.length - tail);
  const ops: Op[] = [];
  for (let i = 0; i < head; i++) ops.push({ kind: "context", oldLine: i + 1, newLine: i + 1, text: a[i] ?? "" });
  const middle =
    (midA.length + 1) * (midB.length + 1) <= MAX_LCS_CELLS
      ? lcsOps(midA, midB)
      : [
          ...midA.map((text, i): Op => ({ kind: "removed", oldLine: i + 1, newLine: null, text })),
          ...midB.map((text, j): Op => ({ kind: "added", oldLine: null, newLine: j + 1, text })),
        ];
  for (const op of middle) {
    ops.push({
      ...op,
      oldLine: op.oldLine === null ? null : op.oldLine + head,
      newLine: op.newLine === null ? null : op.newLine + head,
    });
  }
  for (let k = tail; k > 0; k--) {
    const i = a.length - k;
    const j = b.length - k;
    ops.push({ kind: "context", oldLine: i + 1, newLine: j + 1, text: a[i] ?? "" });
  }
  return ops;
}
