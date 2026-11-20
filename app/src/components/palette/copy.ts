// Which copy of the code the palette searches: the open task's worktree if a
// task is open, else the open project's folder. Never guessed across projects:
// outside a project (home, accounts, a quick question) there is no copy.

import type { CodeCopy } from "../../api/codeTypes";
import type { Task } from "../../api/types";
import { useTasks } from "../../tasks/hooks";
import type { View } from "../Layout";

export interface ActiveCopy {
  copy: CodeCopy;
  project: string;
  /** The code is on screen there (the workspace); elsewhere it has to be opened first. */
  onScreen: boolean;
}

/** A task with files of its own to read: not a question, not discarded, not accepted without a commit. */
function readable(task: Task): boolean {
  if (task.conversation || task.status === "discarded") return false;
  return !(task.status === "accepted" && task.workspace && !task.merge);
}

export function resolveCopy(view: View, tasks: Task[] | undefined): ActiveCopy | null {
  if (view.screen === "memory" || view.screen === "status") {
    return { copy: { kind: "folder", project: view.project }, project: view.project, onScreen: false };
  }
  if (view.screen !== "workspace") return null;
  const { project } = view;
  const folder: ActiveCopy = { copy: { kind: "folder", project }, project, onScreen: true };
  // As the workspace does: a project opens on its home, the folder's copy.
  if (view.task === null || view.task === undefined) return folder;
  const task = tasks?.find((t) => t.id === view.task);
  if (task) return readable(task) ? { copy: { kind: "task", task: task.id }, project, onScreen: true } : folder;
  // Not loaded yet: a task asked for by id is still that task's copy.
  if (typeof view.task === "string") return { copy: { kind: "task", task: view.task }, project, onScreen: true };
  return folder;
}

export function useActiveCopy(view: View): ActiveCopy | null {
  const project = view.screen === "workspace" ? view.project : null;
  const tasks = useTasks(project ?? "", project !== null);
  return resolveCopy(view, tasks.data);
}
