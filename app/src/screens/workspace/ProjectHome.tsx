// A project's own home: where a click on the project lands (never an old
// task). Its sections as tabs, and its tasks by what they need.

import { useState, type ReactNode } from "react";

import { useApi } from "../../api/context";
import type { BoardState, Task } from "../../api/types";
import type { View } from "../../components/Layout";
import { StatusPill } from "../../components/ui/StatusPill";
import { useT, type MessageKey } from "../../i18n";
import { useActivity } from "../../tasks/hooks";
import { taskState } from "../../tasks/state";

export type ProjectTab = "tasks" | "flows" | "memory" | "summary" | "settings";

const TABS: { id: ProjectTab; label: MessageKey }[] = [
  { id: "tasks", label: "project.tab.tasks" },
  { id: "flows", label: "project.tab.flows" },
  { id: "memory", label: "project.tab.memory" },
  { id: "summary", label: "project.tab.summary" },
  { id: "settings", label: "project.tab.settings" },
];

/** Where a project's tab leads: Tareas is its home, the others their own screens. */
export function projectTabView(tab: ProjectTab, project: string): View {
  switch (tab) {
    case "tasks":
      return { screen: "workspace", project };
    case "flows":
      return { screen: "flows", project };
    case "memory":
      return { screen: "memory", project };
    case "summary":
      return { screen: "status", project };
    case "settings":
      return { screen: "projectSettings", project };
  }
}

/** The project's name and its sections; the other tabs are their own screens. */
export function ProjectHead({
  name,
  title,
  path,
  current,
  tools,
  onTab,
}: {
  name: string;
  /** The heading when it is not just the name ("Ajustes de hedy"). */
  title?: string;
  path: string;
  current: ProjectTab;
  /** More buttons at the end of the head (the side panel's toggle). */
  tools?: ReactNode;
  onTab: (tab: ProjectTab) => void;
}) {
  const t = useT();
  const api = useApi();
  return (
    <header className="project-head">
      <div className="project-head__top">
        <h1 title={path}>{title ?? name}</h1>
        <div className="chat__tools">
          <button type="button" className="tool-btn" onClick={() => void api.openInEditor(path)}>
            <span className="tool-btn__label">{t("project.openEditor")}</span>
          </button>
          {tools}
        </div>
      </div>
      <nav className="project-tabs" aria-label={t("project.tabs")}>
        {TABS.map((tab) => (
          <button
            key={tab.id}
            type="button"
            className="project-tabs__tab"
            aria-current={tab.id === current ? "page" : undefined}
            onClick={() => tab.id !== current && onTab(tab.id)}
          >
            {t(tab.label)}
          </button>
        ))}
      </nav>
    </header>
  );
}

type Group = "waiting" | "working" | "ready" | "finished";

const GROUP: Record<BoardState, Group> = {
  plan: "waiting",
  permission: "waiting",
  limited: "waiting",
  failed: "waiting",
  working: "working",
  review: "ready",
  answered: "ready",
  accepted: "finished",
  cancelled: "finished",
};

const GROUP_LABEL: Record<Exclude<Group, "finished">, MessageKey> = {
  waiting: "project.waiting",
  working: "project.working",
  ready: "project.ready",
};

/** Under the new task's composer: the project's tasks by what they need, finished ones folded. */
export function ProjectTasks({ tasks, onOpen }: { tasks: Task[]; onOpen: (task: string) => void }) {
  const t = useT();
  const board = useActivity();
  const [history, setHistory] = useState(false);
  const byId = new Map((board.data ?? []).map((item) => [item.task.id, item]));
  const rows = tasks.map((task) => ({ task, state: taskState(task, byId.get(task.id)) }));
  const of = (group: Group) => rows.filter((r) => GROUP[r.state] === group);
  const finished = of("finished");

  const list = (items: typeof rows) => (
    <ul className="project-tasks__list">
      {items.map(({ task, state }) => (
        <li key={task.id}>
          <button type="button" className="project-task" onClick={() => onOpen(task.id)}>
            <span className="project-task__title">{task.title}</span>
            <StatusPill state={state} task={task} />
          </button>
        </li>
      ))}
    </ul>
  );

  if (rows.length === 0) return null;
  return (
    <div className="project-tasks">
      {(["waiting", "working", "ready"] as const).map((group) => {
        const items = of(group);
        if (items.length === 0) return null;
        return (
          <section key={group} className="project-tasks__group" aria-label={t(GROUP_LABEL[group])}>
            <h2 className="project-tasks__heading">{t(GROUP_LABEL[group])}</h2>
            {list(items)}
          </section>
        );
      })}
      {finished.length > 0 && (
        <>
          <button type="button" className="project-tasks__more" aria-expanded={history} onClick={() => setHistory((h) => !h)}>
            {history
              ? t("project.hideFinished")
              : finished.length === 1
                ? t("project.finishedOne")
                : t("project.finished", { n: finished.length })}
          </button>
          {history && list(finished)}
        </>
      )}
    </div>
  );
}
