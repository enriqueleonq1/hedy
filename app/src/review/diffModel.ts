// The shape of a diff as the review shows it: which lines are visible around
// the changes, the gaps between them (expandable like on GitHub), rows for the
// side-by-side view and word-level emphasis inside a changed line. Pure: no
// React, no I/O, so it is tested on its own.

import type { DiffLine } from "../api/types";

/** Lines of context shown around a change before anything is expanded. */
export const CONTEXT = 3;
/** Lines one "Mostrar 20 líneas" reveals. */
export const EXPAND_STEP = 20;

export type Side = "old" | "new";

/**
 * What the diff shows, in order: runs of visible lines (`from..=to`, indexes in
 * the line list) and the gaps between them. A gap hides `from..=to`; one with
 * `to < from` hides nothing (only a hunk header when the gaps are unknown).
 */
export type Segment =
  | { kind: "lines"; from: number; to: number }
  | { kind: "gap"; from: number; to: number; position: "top" | "middle" | "bottom"; header: string | null };

export function isChange(line: DiffLine): boolean {
  return line.kind !== "context";
}

/** Lines within `context` of a change: what a diff shows before any expansion. */
export function defaultVisible(lines: DiffLine[], context = CONTEXT): Set<number> {
  const visible = new Set<number>();
  lines.forEach((line, i) => {
    if (!isChange(line)) return;
    for (let k = Math.max(0, i - context); k <= Math.min(lines.length - 1, i + context); k++) visible.add(k);
  });
  return visible;
}

/** `@@ -a,b +c,d @@` for lines `from..=to`, numbered like git. */
export function hunkHeader(lines: DiffLine[], from: number, to: number): string {
  const slice = lines.slice(from, to + 1);
  const old = slice.filter((l) => l.kind !== "added");
  const now = slice.filter((l) => l.kind !== "removed");
  const before = lines.slice(0, from);
  const firstOld = old[0]?.oldLine ?? before.filter((l) => l.kind !== "added").length;
  const firstNew = now[0]?.newLine ?? before.filter((l) => l.kind !== "removed").length;
  return `@@ -${firstOld},${old.length} +${firstNew},${now.length} @@`;
}

/** Splits the line list into visible runs and the gaps around them. */
export function segments(lines: DiffLine[], visible: Set<number>): Segment[] {
  const out: Segment[] = [];
  let i = 0;
  while (i < lines.length) {
    const shown = visible.has(i);
    let j = i;
    while (j + 1 < lines.length && visible.has(j + 1) === shown) j++;
    if (shown) {
      out.push({ kind: "lines", from: i, to: j });
    } else {
      const position = i === 0 ? "top" : j === lines.length - 1 ? "bottom" : "middle";
      // A gap at the end of the file hides lines after the last change: no hunk follows.
      out.push({ kind: "gap", from: i, to: j, position, header: null });
    }
    i = j + 1;
  }
  // Each gap names the run that follows it, like git's hunk headers.
  out.forEach((segment, k) => {
    const next = out[k + 1];
    if (segment.kind === "gap" && next?.kind === "lines") segment.header = hunkHeader(lines, next.from, next.to);
  });
  return out;
}

/** The hunks as they came (gaps unknown): their lines, all visible, with header bars between them. */
export function hunkSegments(hunks: { header: string; lines: DiffLine[] }[]): { lines: DiffLine[]; segments: Segment[] } {
  const lines: DiffLine[] = [];
  const out: Segment[] = [];
  hunks.forEach((hunk, k) => {
    const from = lines.length;
    lines.push(...hunk.lines);
    out.push({ kind: "gap", from, to: from - 1, position: k === 0 ? "top" : "middle", header: hunk.header });
    if (hunk.lines.length > 0) out.push({ kind: "lines", from, to: lines.length - 1 });
  });
  return { lines, segments: out };
}

export function gapSize(gap: Extract<Segment, { kind: "gap" }>): number {
  return Math.max(0, gap.to - gap.from + 1);
}

/**
 * Reveals part of a gap: `up` the lines just above the next change (the gap's
 * end), `down` the lines just below the previous one (its start), `all` of it.
 */
export function revealGap(
  visible: Set<number>,
  gap: { from: number; to: number },
  mode: "up" | "down" | "all",
  step = EXPAND_STEP,
): Set<number> {
  const next = new Set(visible);
  const from = mode === "up" ? Math.max(gap.from, gap.to - step + 1) : gap.from;
  const to = mode === "down" ? Math.min(gap.to, gap.from + step - 1) : gap.to;
  for (let i = from; i <= to; i++) next.add(i);
  return next;
}

/**
 * Pairs each removed line with the added line that replaced it (the i-th of a
 * block of removals with the i-th of the additions after it), both ways.
 */
export function pairChanges(lines: DiffLine[]): Map<number, number> {
  const pairs = new Map<number, number>();
  let i = 0;
  while (i < lines.length) {
    if (lines[i]?.kind !== "removed") {
      i++;
      continue;
    }
    const removed: number[] = [];
    while (lines[i]?.kind === "removed") removed.push(i++);
    const added: number[] = [];
    while (lines[i]?.kind === "added") added.push(i++);
    removed.forEach((r, k) => {
      const a = added[k];
      if (a === undefined) return;
      pairs.set(r, a);
      pairs.set(a, r);
    });
  }
  return pairs;
}

/** One row of the side-by-side view: the old line on the left, the new on the right (indexes). */
export interface SplitRow {
  left: number | null;
  right: number | null;
}

/** Rows for lines `from..=to`: context on both sides, a removal next to the addition that replaced it. */
export function splitRows(lines: DiffLine[], from: number, to: number): SplitRow[] {
  const rows: SplitRow[] = [];
  let i = from;
  while (i <= to) {
    const line = lines[i];
    if (!line) break;
    if (line.kind === "context") {
      rows.push({ left: i, right: i });
      i++;
      continue;
    }
    const removed: number[] = [];
    while (i <= to && lines[i]?.kind === "removed") removed.push(i++);
    const added: number[] = [];
    while (i <= to && lines[i]?.kind === "added") added.push(i++);
    for (let k = 0; k < Math.max(removed.length, added.length); k++) {
      rows.push({ left: removed[k] ?? null, right: added[k] ?? null });
    }
  }
  return rows;
}

/** A line's number on a side, or null when it does not exist there. */
export function lineNumber(line: DiffLine, side: Side): number | null {
  return side === "old" ? (line.kind === "added" ? null : line.oldLine) : line.kind === "removed" ? null : line.newLine;
}

/** The side a line of the unified view belongs to: removals are old, the rest new. */
export function sideOf(line: DiffLine): Side {
  return line.kind === "removed" ? "old" : "new";
}

/** Where line `n` of a side is in the list (`-1`: not there). */
export function indexOfLine(lines: DiffLine[], side: Side, n: number): number {
  return lines.findIndex((l) => lineNumber(l, side) === n);
}

/**
 * What a selection of lines `a..=b` (indexes, either order) comments on: the
 * numbers on `side` (lines without one there are skipped) and their text.
 */
export function selectionTarget(
  lines: DiffLine[],
  side: Side,
  a: number,
  b: number,
): { line: number; endLine: number | null; code: string } | null {
  const picked = lines.slice(Math.min(a, b), Math.max(a, b) + 1).filter((l) => lineNumber(l, side) !== null);
  const numbers = picked.map((l) => lineNumber(l, side) ?? 0);
  if (numbers.length === 0) return null;
  const line = Math.min(...numbers);
  const end = Math.max(...numbers);
  return { line, endLine: end === line ? null : end, code: picked.map((l) => l.text).join("\n") };
}

/** A stretch of a line, in UTF-16 units, end exclusive. */
export interface Range {
  start: number;
  end: number;
}

const TOKEN = /\w+|\s+|[^\w\s]/gu;
/** Past this many tokens a line is not compared word by word (cost and noise). */
const MAX_TOKENS = 400;

/**
 * The words that differ between a removed line and the line that replaced it,
 * for a stronger tint inside the changed lines. `null` when the lines have too
 * little in common for the emphasis to help (it would cover almost everything).
 */
export function wordDiff(a: string, b: string): { old: Range[]; new: Range[] } | null {
  if (a === b) return null;
  const ta = a.match(TOKEN) ?? [];
  const tb = b.match(TOKEN) ?? [];
  if (ta.length > MAX_TOKENS || tb.length > MAX_TOKENS) return null;
  const n = ta.length;
  const m = tb.length;
  const lcs = new Uint16Array((n + 1) * (m + 1));
  const at = (i: number, j: number) => lcs[i * (m + 1) + j] ?? 0;
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      lcs[i * (m + 1) + j] = ta[i] === tb[j] ? at(i + 1, j + 1) + 1 : Math.max(at(i + 1, j), at(i, j + 1));
    }
  }
  const keepA = new Array<boolean>(n).fill(false);
  const keepB = new Array<boolean>(m).fill(false);
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (ta[i] === tb[j]) {
      keepA[i++] = true;
      keepB[j++] = true;
    } else if (at(i + 1, j) >= at(i, j + 1)) i++;
    else j++;
  }
  const common = ta.reduce((sum, token, k) => sum + (keepA[k] && token.trim() ? token.length : 0), 0);
  const meaningful = Math.max(a.trim().length, b.trim().length, 1);
  if (common / meaningful < 0.4) return null;
  return { old: changedRanges(ta, keepA), new: changedRanges(tb, keepB) };
}

/** Merges runs of changed tokens (whitespace between two changes joins them). */
function changedRanges(tokens: string[], keep: boolean[]): Range[] {
  const ranges: Range[] = [];
  let offset = 0;
  tokens.forEach((token, k) => {
    const start = offset;
    offset += token.length;
    if (keep[k]) {
      // Whitespace kept between two changes still reads as one change.
      const last = ranges.at(-1);
      const nextChanged = keep[k + 1] === false;
      if (last && last.end === start && !token.trim() && nextChanged) last.end = offset;
      return;
    }
    const last = ranges.at(-1);
    if (last && last.end === start) last.end = offset;
    else ranges.push({ start, end: offset });
  });
  return ranges.filter((r) => tokens.join("").slice(r.start, r.end).trim() !== "");
}
