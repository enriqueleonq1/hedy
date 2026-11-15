// What the explorer shows of a copy's tree: filters (only changed files) and
// the status each folder rolls up from the files below it.

import type { ChangeKind, TreeEntry } from "../../api/codeTypes";
import { parentOf } from "../../components/ui/treeModel";

/** One letter per status, as git and VS Code show them. */
export const STATUS_LETTER: Record<ChangeKind, string> = {
  added: "A",
  modified: "M",
  deleted: "D",
  renamed: "R",
  untracked: "U",
};

/** Which status a folder shows when its files differ: the most telling one wins. */
const WEIGHT: Record<ChangeKind, number> = { deleted: 1, renamed: 2, untracked: 3, added: 4, modified: 5 };

export function rollUpStatus(entries: TreeEntry[]): Map<string, ChangeKind> {
  const folders = new Map<string, ChangeKind>();
  for (const entry of entries) {
    const status = entry.status;
    if (!status) continue;
    for (let folder = parentOf(entry.path); folder !== ""; folder = parentOf(folder)) {
      const current = folders.get(folder);
      if (current && WEIGHT[current] >= WEIGHT[status]) break;
      folders.set(folder, status);
    }
  }
  return folders;
}

export function changedEntries(entries: TreeEntry[]): TreeEntry[] {
  return entries.filter((entry) => entry.status !== null);
}

/** Paths of the files a copy changed, in tree order. */
export function countChanged(entries: TreeEntry[]): number {
  let n = 0;
  for (const entry of entries) if (entry.status) n++;
  return n;
}
