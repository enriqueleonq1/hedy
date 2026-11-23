// A project's latest tasks under its row in the sidebar, folded by default
// except for the project on screen: one click opens any of them.

import { useState } from "react";

import type { Task } from "../api/types";
import { useLanguage, useT } from "../i18n";
import { useActivity, useTasks } from "../tasks/hooks";
import { taskState } from "../tasks/state";
import { PILL_TONE, pillLabel } from "./ui/StatusPill";

/** How many tasks a project shows in the sidebar; the rest are on its home. */
export const SIDEBAR_TASKS = 10;
const STORAGE_KEY = "hedy.sidebar.tasks";

function readOpen(): Record<string, boolean> {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}");
    return value && typeof value === "object" ? (value as Record<string, boolean>) : {};
  } catch {
    return {};
  }
}

function writeOpen(value: Record<string, boolean>) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(value));
  } catch {
    // Storage blocked: the choice lasts until the window closes.
  }
}

/**
 * Which projects show their tasks: what the user chose on this computer,
 * else only the project on screen.
 */
export function useOpenProjects(active: string | null) {
  const [chosen, setChosen] = useState<Record<string, boolean>>(readOpen);
  return {
    isOpen: (path: string) => chosen[path] ?? path === active,
    toggle: (path: string) =>
      setChosen((old) => {
        const next = { ...old, [path]: !(old[path] ?? path === active) };
        writeOpen(next);
        return next;
      }),
  };
}

/** The newest tasks of `project`, each opening its task. */
export function SidebarTasks({
  id,
  project,
  current,
  onOpen,
  onAll,
}: {
  id: string;
  project: string;
  /** The task on screen, marked as the current one. */
  current: string | null;
  onOpen: (task: string) => void;
  /** The project's home, with every task. */
  onAll: () => void;
}) {
  const t = useT();
  const language = useLanguage();
  const tasks = useTasks(project);
  const board = useActivity();
  const byId = new Map((board.data ?? []).map((item) => [item.task.id, item]));
  const all: Task[] = [...(tasks.data ?? [])].sort((a, b) => b.createdAt - a.createdAt);
  const shown = all.slice(0, SIDEBAR_TASKS);

  return (
    <ul id={id} className="sidebar__tasks" aria-label={t("shell.tasksOf")}>
      {tasks.isSuccess && all.length === 0 && <li className="sidebar__tasks-empty">{t("shell.noTasks")}</li>}
      {shown.map((task) => {
        const state = taskState(task, byId.get(task.id));
        const label = pillLabel(state, task, t, language);
        return (
          <li key={task.id}>
            <button
              type="button"
              className="sidebar__task"
              aria-current={current === task.id ? "page" : undefined}
              title={`${task.title} · ${label}`}
              onClick={() => onOpen(task.id)}
            >
              <span className={`sidebar__task-dot status-pill--${PILL_TONE[state]}`} aria-hidden />
              <span className="sidebar__task-title">{task.title}</span>
              <span className="sr-only">{label}</span>
            </button>
          </li>
        );
      })}
      {all.length > SIDEBAR_TASKS && (
        <li>
          <button type="button" className="sidebar__task sidebar__task--all" onClick={onAll}>
            {t("shell.allTasks", { n: all.length })}
          </button>
        </li>
      )}
    </ul>
  );
}
