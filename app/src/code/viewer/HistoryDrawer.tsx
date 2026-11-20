// A file's history: its commits mixed with the Hedy tasks that changed it,
// newest first. Each commit can be viewed (the file as it was) or compared with
// the current file; an open task's change lives in its own copy.

import { useEffect, useRef } from "react";
import { useQuery } from "@tanstack/react-query";

import { errorMessage, useApi } from "../../api/context";
import type { CodeCopy, FileHistory, HistoryCommit, TaskTouch } from "../../api/codeTypes";
import { Button } from "../../components/ui/Button";
import { codeKeys } from "../../files/navigator";
import { timeAgo } from "../../format";
import { useT } from "../../i18n";
import { STATUS_LABEL } from "../../tasks/timeline";
import { CloseIcon, DiffIcon, PreviewIcon } from "../icons";
import { CodeState } from "./states";

export type HistoryEntry =
  | { kind: "commit"; at: number; commit: HistoryCommit; task: TaskTouch | null }
  | { kind: "task"; at: number; task: TaskTouch };

/** Commits and tasks in one list, newest first; an accepted task shows on its commit. */
export function mergeHistory(history: FileHistory): HistoryEntry[] {
  const byCommit = new Map<string, TaskTouch>();
  for (const task of history.tasks) if (task.commit) byCommit.set(task.commit, task);
  const tasksById = new Map(history.tasks.map((task) => [task.task, task]));
  const entries: HistoryEntry[] = history.commits.map((commit) => ({
    kind: "commit",
    at: commit.date,
    commit,
    task: byCommit.get(commit.hash) ?? (commit.task ? (tasksById.get(commit.task) ?? null) : null),
  }));
  const shown = new Set(entries.flatMap((e) => (e.kind === "commit" && e.task ? [e.task.task] : [])));
  for (const task of history.tasks) {
    if (!shown.has(task.task) && !(task.commit && history.commits.some((c) => c.hash === task.commit))) {
      entries.push({ kind: "task", at: task.at, task });
    }
  }
  return entries.sort((a, b) => b.at - a.at);
}

export function HistoryDrawer({
  copy,
  path,
  onClose,
  onView,
  onCompare,
  onOpenTask,
}: {
  copy: CodeCopy;
  path: string;
  onClose: () => void;
  /** Show the file at a commit. */
  onView: (commit: HistoryCommit) => void;
  /** Compare a commit with the current file. */
  onCompare: (commit: HistoryCommit) => void;
  /** Open the file in an open task's copy. */
  onOpenTask: (task: TaskTouch) => void;
}) {
  const api = useApi();
  const t = useT();
  const drawer = useRef<HTMLElement>(null);
  const history = useQuery({ queryKey: codeKeys.history(copy, path), queryFn: () => api.fileHistory(copy, path) });

  useEffect(() => {
    drawer.current?.focus();
  }, []);

  const entries = history.data ? mergeHistory(history.data) : [];

  return (
    <aside
      ref={drawer}
      className="history-drawer"
      aria-label={t("code.history.title")}
      tabIndex={-1}
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.stopPropagation();
          onClose();
        }
      }}
    >
      <header className="history-drawer__head">
        <h3>{t("code.history.title")}</h3>
        <button type="button" className="icon-btn" aria-label={t("common.close")} title={t("common.close")} onClick={onClose}>
          <CloseIcon />
        </button>
      </header>
      {history.isPending ? (
        <div className="history-drawer__skeleton" role="status" aria-label={t("code.history.loading")}>
          {[0, 1, 2, 3].map((i) => (
            <span key={i} />
          ))}
        </div>
      ) : history.isError ? (
        <CodeState
          tone="error"
          role="alert"
          title={t("code.history.error")}
          text={errorMessage(history.error, t)}
          actions={
            <Button small onClick={() => void history.refetch()}>
              {t("common.retry")}
            </Button>
          }
        />
      ) : entries.length === 0 ? (
        <CodeState title={t("code.history.empty")} text={t("code.history.emptyText")} />
      ) : (
        <ol className="history-list">
          {entries.map((entry) =>
            entry.kind === "commit" ? (
              <li key={entry.commit.hash} className="history-item">
                <button type="button" className="history-item__main" onClick={() => onView(entry.commit)}>
                  <span className="history-item__subject">{entry.commit.subject}</span>
                  <span className="history-item__meta">
                    {entry.commit.author} · {timeAgo(entry.commit.date, t)} · <span className="mono">{entry.commit.short}</span>
                  </span>
                  {entry.task && <span className="history-item__task">{t("code.history.task", { title: entry.task.title })}</span>}
                </button>
                <span className="history-item__actions">
                  <button
                    type="button"
                    className="icon-btn"
                    aria-label={t("code.history.view", { rev: entry.commit.short })}
                    title={t("code.history.viewShort")}
                    onClick={() => onView(entry.commit)}
                  >
                    <PreviewIcon />
                  </button>
                  <button
                    type="button"
                    className="icon-btn"
                    aria-label={t("code.history.compare", { rev: entry.commit.short })}
                    title={t("code.history.compareShort")}
                    onClick={() => onCompare(entry.commit)}
                  >
                    <DiffIcon />
                  </button>
                </span>
              </li>
            ) : (
              <li key={`task-${entry.task.task}`} className="history-item history-item--task">
                <button type="button" className="history-item__main" onClick={() => onOpenTask(entry.task)}>
                  <span className="history-item__subject">{t("code.history.task", { title: entry.task.title })}</span>
                  <span className="history-item__meta">
                    {t(STATUS_LABEL[entry.task.status])} · {timeAgo(entry.task.at, t)}
                    {entry.task.open && <> · {t("code.history.onlyInTask")}</>}
                  </span>
                </button>
              </li>
            ),
          )}
        </ol>
      )}
    </aside>
  );
}
