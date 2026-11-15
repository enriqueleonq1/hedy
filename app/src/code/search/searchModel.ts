// Search results as a two-level tree (file → matching lines) for the virtual
// tree, and how a long matching line is cut so its first match stays in sight.

import type { FileMatches, LineMatch, MatchRange } from "../../api/codeTypes";
import type { TreeFolder, TreeLeaf } from "../../components/ui/treeModel";
import { baseName } from "../copies";

export interface MatchItem {
  file: string;
  match: LineMatch;
}

/** A leaf's path: the file, a NUL (never in a path) and the match's place. */
export function matchKey(file: string, line: number, index: number): string {
  return `${file}\u0000${line}:${index}`;
}

export function resultsTree(files: FileMatches[]): TreeFolder<MatchItem> {
  let size = 0;
  const children: TreeFolder<MatchItem>[] = files.map((file) => {
    size += file.matches.length;
    const leaves: TreeLeaf<MatchItem>[] = file.matches.map((match, i) => ({
      kind: "file",
      path: matchKey(file.path, match.line, i),
      name: match.text.trim(),
      item: { file: file.path, match },
    }));
    return { kind: "folder", path: file.path, name: baseName(file.path), children: leaves, size: leaves.length };
  });
  return { kind: "folder", path: "", name: "", children, size };
}

/** Characters kept before the first match when a line is cut. */
const LEAD = 24;

/**
 * The part of a line to show: leading spaces dropped, and a long prefix cut to
 * an ellipsis so the first match is near the start. Ranges move with it.
 */
export function previewLine(text: string, ranges: MatchRange[]): { text: string; ranges: MatchRange[]; cut: boolean } {
  const indent = text.length - text.trimStart().length;
  const first = ranges[0]?.start ?? indent;
  const from = first - indent > LEAD ? first - LEAD : indent;
  const cut = from > indent;
  return {
    text: text.slice(from),
    ranges: ranges.map((r) => ({ start: Math.max(0, r.start - from), end: Math.max(0, r.end - from) })),
    cut,
  };
}
