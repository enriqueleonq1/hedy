// Which copy of the code a view shows: the user's folder or a task's worktree.
// Never implied: every code view carries one (see docs/decisions.md §25).

import type { CodeCopy } from "../api/codeTypes";
import type { Task } from "../api/types";

/**
 * Whether a task has files of its own to read: a project task with a worktree
 * (or, once accepted, its last commit). A discarded task has none left, and a
 * question reads the folder.
 */
export function taskHasCopy(task: Task | null): task is Task {
  return !!task && task.mode === "project" && !!task.workspace && task.status !== "discarded";
}

/** The copy a view opens on: the open task's own, else the folder. */
export function defaultCopy(project: string, task: Task | null): CodeCopy {
  return taskHasCopy(task) ? { kind: "task", task: task.id } : { kind: "folder", project };
}

/** `a/b/c.py` → `c.py`. */
export function baseName(path: string): string {
  const clean = path.replace(/\/$/, "");
  return clean.slice(clean.lastIndexOf("/") + 1);
}

/** `a/b/c.py` → `a/b` (`""` at the top). */
export function dirName(path: string): string {
  const cut = path.replace(/\/$/, "").lastIndexOf("/");
  return cut < 0 ? "" : path.slice(0, cut);
}

/** The absolute path of a file of a copy, for "Copiar ruta". */
export function absolutePath(root: string, path: string): string {
  return `${root.replace(/\/+$/, "")}/${path}`;
}

/**
 * A path an agent reported (relative, or absolute inside the task's worktree or
 * the project) as a path of the copy, or `null` when it lies elsewhere.
 */
export function relativeToCopy(path: string, roots: (string | null | undefined)[]): string | null {
  if (!path.startsWith("/") && !/^[A-Za-z]:[\\/]/.test(path)) return path.replace(/^\.\//, "");
  for (const root of roots) {
    if (!root) continue;
    const prefix = `${root.replace(/\/+$/, "")}/`;
    if (path.startsWith(prefix)) return path.slice(prefix.length);
  }
  return null;
}

/** `path:10` or `path:10-24`: how a line reference is written for the AI. */
export function lineReference(path: string, start: number, end = start): string {
  return end > start ? `${path}:${start}-${end}` : `${path}:${start}`;
}

/** Copies text to the clipboard; resolves to whether it worked. */
export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}
