// Which lines of the current file changed against the base, from a diff's hunks
// (pure: no editor here). A run of added lines is "added", or "modified" when it
// replaced removed ones; lines removed with nothing in their place mark the line
// that now follows them.

import type { DiffHunk } from "../../api/codeTypes";

export type LineMark = "added" | "modified" | "deleted";

export function lineMarksFromHunks(hunks: DiffHunk[]): Map<number, LineMark> {
  const marks = new Map<number, LineMark>();
  for (const hunk of hunks) {
    let removed = 0;
    let added: number[] = [];
    let lastNew = 0;
    const flush = (nextNew: number | null) => {
      if (added.length > 0) {
        for (const line of added) marks.set(line, removed > 0 ? "modified" : "added");
      } else if (removed > 0) {
        const at = nextNew ?? Math.max(1, lastNew);
        if (!marks.has(at)) marks.set(at, "deleted");
      }
      removed = 0;
      added = [];
    };
    for (const line of hunk.lines) {
      if (line.kind === "removed") removed++;
      else if (line.kind === "added" && line.newLine !== null) {
        added.push(line.newLine);
        lastNew = line.newLine;
      } else if (line.kind === "context") {
        flush(line.newLine);
        if (line.newLine !== null) lastNew = line.newLine;
      }
    }
    flush(null);
  }
  return marks;
}
