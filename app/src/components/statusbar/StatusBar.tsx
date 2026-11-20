// The status bar: a slim, quiet line at the bottom of the window, as in VS
// Code. Each item says one thing about the screen and is a small button that
// does the obvious next step (switch copy, pull, review, open the status, go
// to the composer, copy the path, list the shortcuts).
// Items show only where they apply; low-priority ones go first when narrow.

import { useQuery, useQueryClient } from "@tanstack/react-query";

import type { CodeCopy, CodeTree } from "../../api/codeTypes";
import { useApi } from "../../api/context";
import type { Task } from "../../api/types";
import { codeKeys, useFileNavigator } from "../../files/navigator";
import { timeAgo } from "../../format";
import { useT } from "../../i18n";
import { FolderStatus } from "../../screens/workspace/ship/FolderStatus";
import { useFolderStatus } from "../../screens/workspace/ship/useShip";
import { isOpen, modelLabel } from "../../tasks/timeline";
import { needsYou, useActivity, useTaskDiff, useTasks } from "../../tasks/hooks";
import { viewProject, type View } from "../Layout";
import { pressPanelShortcut } from "../palette/codeCommands";
import { resolveCopy } from "../palette/copy";
import { modKey, shiftKey } from "../palette/keys";
import { useShortcuts } from "../shortcuts";
import { MenuButton, type MenuItem } from "../ui/Menu";
import { useToast } from "../ui/Toast";

interface Props {
  view: View;
  onNavigate: (view: View) => void;
}

/** The task the workspace shows, as it picks it: the one asked for, else the most recent open one. */
export function shownTask(view: View, tasks: Task[] | undefined): Task | null {
  if (view.screen !== "workspace" || view.task === null) return null;
  if (view.task === undefined) return tasks?.find(isOpen) ?? null;
  return tasks?.find((t) => t.id === view.task) ?? null;
}

export function StatusBar({ view, onNavigate }: Props) {
  const t = useT();
  const project = viewProject(view);
  const workspace = view.screen === "workspace" ? view : null;
  const tasks = useTasks(project ?? "", !!workspace);
  const task = shownTask(view, tasks.data);
  const active = workspace ? resolveCopy(view, tasks.data) : null;

  return (
    <footer className="statusbar" aria-label={t("statusbar.label")}>
      <div className="statusbar__group">
        {workspace && active && (
          <CopyItem
            project={workspace.project}
            copy={active.copy}
            tasks={tasks.data ?? []}
            onSelect={(next) => onNavigate({ screen: "workspace", project: workspace.project, task: next })}
          />
        )}
        {project && <FolderStatus project={project} placement="bar" />}
        {workspace && active && <ChangesItem project={workspace.project} copy={active.copy} task={task} />}
        {project && <TestsItem project={project} onOpen={() => onNavigate({ screen: "status", project })} />}
        {task && isOpen(task) && <AgentItem task={task} />}
      </div>
      <div className="statusbar__group statusbar__group--end">
        {workspace && <FileItem />}
        <HelpItem />
      </div>
    </footer>
  );
}

/** Which copy the code views show; its menu switches to the folder or another open task. */
function CopyItem({
  project,
  copy,
  tasks,
  onSelect,
}: {
  project: string;
  copy: CodeCopy;
  tasks: Task[];
  /** `null`: the folder (the workspace without a task). */
  onSelect: (task: string | null) => void;
}) {
  const t = useT();
  const mod = modKey();
  const current = copy.kind === "task" ? tasks.find((x) => x.id === copy.task) : undefined;
  const title = copy.kind === "task" ? (current?.title ?? "…") : null;
  const name = title === null ? t("statusbar.copy.folder") : t("statusbar.copy.taskItem", { title });
  const items: MenuItem[] = [
    { label: t("statusbar.copy.folder"), disabled: copy.kind === "folder", onSelect: () => onSelect(null) },
    ...tasks
      .map((task, index) => ({ task, index }))
      .filter(({ task }) => isOpen(task) && !task.conversation && task.project === project)
      .map(({ task, index }, i) => ({
        label: t("statusbar.copy.taskItem", { title: task.title }),
        disabled: copy.kind === "task" && copy.task === task.id,
        // Ctrl/⌘ 1…9 opens the project's Nth task, from anywhere in the workspace.
        shortcut: index < 9 ? `${mod} ${index + 1}` : undefined,
        separatorBefore: i === 0,
        onSelect: () => onSelect(task.id),
      })),
  ];

  return (
    <MenuButton
      label={t("statusbar.copy.menu", { copy: name })}
      title={t("statusbar.copy.title")}
      items={items}
      align="start"
      className="statusbar__item statusbar__copy"
    >
      <CopyIcon kind={copy.kind} />
      {title === null ? (
        <span>{t("statusbar.copy.folder")}</span>
      ) : (
        <>
          <span className="statusbar__muted">{t("statusbar.copy.task")}</span>
          <span className="statusbar__text">{title}</span>
        </>
      )}
    </MenuButton>
  );
}

/** Opens the review, as Ctrl/⌘ ⇧ G does; without a panel to take it, the first changed file's diff. */
function useOpenReview(copy: CodeCopy, firstChanged: () => string | undefined) {
  const nav = useFileNavigator();
  return () => {
    if (pressPanelShortcut("G")) return;
    const path = firstChanged();
    if (path) nav.open({ copy, path, view: "diff" });
  };
}

/** The changes of the copy on screen: the task's files, or the folder's uncommitted ones. */
function ChangesItem({ project, copy, task }: { project: string; copy: CodeCopy; task: Task | null }) {
  const t = useT();
  const client = useQueryClient();
  const onTask = copy.kind === "task" && !!task && task.id === copy.task;
  const editable = onTask && !!task.workspace && isOpen(task);
  const diff = useTaskDiff(onTask ? task.id : null, editable);
  const folder = useFolderStatus(copy.kind === "folder" ? project : null);
  const open = useOpenReview(copy, () =>
    copy.kind === "task"
      ? diff.data?.[0]?.path
      : client.getQueryData<CodeTree>(codeKeys.tree(copy))?.entries.find((e) => e.status !== null && !e.locked)?.path,
  );

  const n = copy.kind === "task" ? (editable ? (diff.data?.length ?? 0) : 0) : (folder.data?.changes ?? 0);
  if (n === 0) return null;
  const keys = [modKey(), shiftKey(), "G"].join(" ");
  const label =
    copy.kind === "task"
      ? t(n === 1 ? "statusbar.changes.taskOne" : "statusbar.changes.task", { n, keys })
      : t(n === 1 ? "statusbar.changes.folderOne" : "statusbar.changes.folder", { n, keys });

  return (
    <button
      type="button"
      className="statusbar__item statusbar__changes"
      title={label}
      aria-label={label}
      aria-keyshortcuts={modKey() === "⌘" ? "Meta+Shift+G" : "Control+Shift+G"}
      onClick={open}
    >
      <span className="statusbar__dot statusbar__dot--changes" aria-hidden />
      <span aria-hidden>{n}</span>
    </button>
  );
}

/** The project's last test run, if Hedy saw one (the status screen's figure). */
function TestsItem({ project, onOpen }: { project: string; onOpen: () => void }) {
  const t = useT();
  const api = useApi();
  const status = useQuery({ queryKey: ["status", project], queryFn: () => api.projectStatus(project), staleTime: 60_000 });
  if (!status.data) return null;
  const last = status.data.lastTests;
  const label = last
    ? t(last.passed ? "statusbar.tests.passed" : "statusbar.tests.failed", {
        command: last.command,
        when: timeAgo(last.at, t),
      })
    : t("statusbar.tests.none");
  const tone = last ? (last.passed ? "ok" : "failed") : "none";

  return (
    <button
      type="button"
      className={`statusbar__item statusbar__item--low statusbar__tests statusbar__tests--${tone}`}
      title={label}
      aria-label={label}
      onClick={onOpen}
    >
      <span className="statusbar__glyph" aria-hidden>
        {last ? (last.passed ? "✓" : "✗") : "—"}
      </span>
      <span aria-hidden>{t("statusbar.tests")}</span>
    </button>
  );
}

/** Puts the focus on the composer; `false` when it is not on screen. */
function focusComposer(): boolean {
  const box = document.getElementById("composer-input");
  if (!(box instanceof HTMLTextAreaElement) || box.disabled) return false;
  box.focus();
  return true;
}

/** Puts the focus on the card that waits for an answer (the latest), else the composer. */
function focusCard() {
  const cards = document.querySelectorAll<HTMLElement>(".plan-card--pending, .permission-card, .limit-card");
  const card = cards[cards.length - 1];
  if (!card) {
    focusComposer();
    return;
  }
  card.scrollIntoView?.({ block: "nearest" });
  const target = card.hasAttribute("tabindex") ? card : card.querySelector<HTMLElement>("button:not(:disabled)");
  (target ?? card).focus();
}

/** The open task's agent: working (a small live dot) or waiting for the user. */
function AgentItem({ task }: { task: Task }) {
  const t = useT();
  const row = useActivity().data?.find((item) => item.task.id === task.id);
  const agent = row?.agent ?? modelLabel(task.model);
  const waiting = row ? needsYou(row.state) : task.status === "waitingApproval";
  const working = row ? row.state === "working" : task.live && task.status === "running";

  if (waiting) {
    const label = t("statusbar.agent.waitingHint", { agent });
    return (
      <button type="button" className="statusbar__item statusbar__agent statusbar__agent--waiting" title={label} aria-label={label} onClick={focusCard}>
        <span className="statusbar__dot statusbar__dot--waiting" aria-hidden />
        <span className="statusbar__text" aria-hidden>
          {t("statusbar.agent.waiting")}
        </span>
      </button>
    );
  }
  if (!working) return null;
  const label = t("statusbar.agent.workingHint", { agent });
  return (
    <button type="button" className="statusbar__item statusbar__agent" title={label} aria-label={label} onClick={focusComposer}>
      <span className="statusbar__dot statusbar__dot--working" aria-hidden />
      <span className="statusbar__text" aria-hidden>
        {t("statusbar.agent.working", { agent })}
      </span>
    </button>
  );
}

/** The file open in the viewer: its name and line; a click copies its path. */
function FileItem() {
  const t = useT();
  const toast = useToast();
  const current = useFileNavigator().current;
  if (!current) return null;
  const { path, line } = current;
  const name = path.slice(path.lastIndexOf("/") + 1);
  const label = t("statusbar.file.copy", { path });
  const copy = () =>
    void navigator.clipboard
      ?.writeText(path)
      .then(
        () => toast.info(t("statusbar.file.copied", { path })),
        () => toast.info(t("palette.copyFailed")),
      );

  return (
    <button type="button" className="statusbar__item statusbar__item--low statusbar__file" title={label} aria-label={label} onClick={copy}>
      <span className="statusbar__text">{name}</span>
      {line !== undefined && <span className="statusbar__muted">{t("statusbar.file.line", { n: line })}</span>}
    </button>
  );
}

function HelpItem() {
  const t = useT();
  const shortcuts = useShortcuts();
  return (
    <button
      type="button"
      className="statusbar__item statusbar__help"
      title={t("statusbar.help")}
      aria-label={t("statusbar.help")}
      aria-keyshortcuts="?"
      onClick={shortcuts.show}
    >
      <span aria-hidden>?</span>
    </button>
  );
}

/** Decorative: a folder, or a task's own copy (a branch off it). */
function CopyIcon({ kind }: { kind: CodeCopy["kind"] }) {
  return (
    <svg className="icon" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" focusable="false">
      {kind === "folder" ? (
        <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
      ) : (
        <>
          <rect x="4" y="4" width="11" height="11" rx="2" />
          <path d="M9 19h9a2 2 0 0 0 2-2V8" />
        </>
      )}
    </svg>
  );
}
