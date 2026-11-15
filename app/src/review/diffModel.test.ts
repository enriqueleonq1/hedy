import type { DiffLine } from "../api/types";
import {
  defaultVisible,
  hunkSegments,
  pairChanges,
  revealGap,
  segments,
  selectionTarget,
  splitRows,
  wordDiff,
} from "./diffModel";

/** A 60-line file where line 30 changed: `ctx` lines, then one removal and one addition. */
function sample(): DiffLine[] {
  const lines: DiffLine[] = [];
  for (let n = 1; n <= 29; n++) lines.push({ kind: "context", oldLine: n, newLine: n, text: `line ${n}` });
  lines.push({ kind: "removed", oldLine: 30, newLine: null, text: "total = a + b" });
  lines.push({ kind: "added", oldLine: null, newLine: 30, text: "total = round(a + b)" });
  for (let n = 31; n <= 60; n++) lines.push({ kind: "context", oldLine: n, newLine: n, text: `line ${n}` });
  return lines;
}

describe("diff model", () => {
  it("shows three lines of context around a change and gaps around them", () => {
    const lines = sample();
    const visible = defaultVisible(lines);
    expect([...visible].sort((a, b) => a - b)).toEqual([26, 27, 28, 29, 30, 31, 32, 33]);
    const segs = segments(lines, visible);
    expect(segs).toEqual([
      { kind: "gap", from: 0, to: 25, position: "top", header: "@@ -27,7 +27,7 @@" },
      { kind: "lines", from: 26, to: 33 },
      { kind: "gap", from: 34, to: 60, position: "bottom", header: null },
    ]);
  });

  it("reveals 20 lines next to a change, or a whole gap", () => {
    const lines = sample();
    const visible = defaultVisible(lines);
    const up = revealGap(visible, { from: 0, to: 25 }, "up");
    expect(up.has(6)).toBe(true);
    expect(up.has(5)).toBe(false);
    const down = revealGap(visible, { from: 34, to: 60 }, "down");
    expect(down.has(53)).toBe(true);
    expect(down.has(54)).toBe(false);
    const all = revealGap(visible, { from: 34, to: 60 }, "all");
    expect(segments(lines, all).at(-1)).toEqual({ kind: "lines", from: 26, to: 60 });
  });

  it("keeps the hunks as they came when the gaps are unknown", () => {
    const { lines, segments: segs } = hunkSegments([{ header: "@@ -1,1 +1,1 @@", lines: sample().slice(29, 31) }]);
    expect(lines).toHaveLength(2);
    expect(segs).toEqual([
      { kind: "gap", from: 0, to: -1, position: "top", header: "@@ -1,1 +1,1 @@" },
      { kind: "lines", from: 0, to: 1 },
    ]);
  });

  it("pairs a removal with its replacement for the side-by-side view", () => {
    const lines = sample();
    expect(pairChanges(lines).get(29)).toBe(30);
    expect(splitRows(lines, 28, 31)).toEqual([
      { left: 28, right: 28 },
      { left: 29, right: 30 },
      { left: 31, right: 31 },
    ]);
  });

  it("emphasizes only the words that changed", () => {
    const diff = wordDiff("total = a + b", "total = round(a + b)");
    expect(diff?.old).toEqual([]);
    expect(diff?.new.map((r) => "total = round(a + b)".slice(r.start, r.end))).toEqual(["round(", ")"]);
    // Nothing in common: no emphasis (the whole line is the change).
    expect(wordDiff("import os", "def main():")).toBeNull();
  });

  it("turns a selection into the lines a comment is about", () => {
    const lines = sample();
    expect(selectionTarget(lines, "new", 31, 28)).toEqual({
      line: 29,
      endLine: 31,
      code: "line 29\ntotal = round(a + b)\nline 31",
    });
    expect(selectionTarget(lines, "old", 29, 29)).toEqual({ line: 30, endLine: null, code: "total = a + b" });
  });
});
