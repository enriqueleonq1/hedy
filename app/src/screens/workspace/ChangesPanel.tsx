import { useRef, useState, type KeyboardEvent } from "react";
import { useQuery } from "@tanstack/react-query";

import { errorMessage, useAction, useApi } from "../../api/context";
import type { Task } from "../../api/types";
import { useProjectActions } from "../../components/projects";
import { TerminalView } from "../../components/Terminal/TerminalView";
import { Button, ConfirmButton } from "../../components/ui/Button";
import { useT, type MessageKey } from "../../i18n";
import { useTaskDiff, useTaskEvents } from "../../tasks/hooks";
import { isOpen, pendingCard } from "../../tasks/timeline";
import { AcceptSheet, useAcceptedToast } from "./AcceptSheet";
import { DiffView } from "./DiffView";
import { useReviewed } from "./reviewed";

type Tab = "changes" | "terminal" | "preview";

const TABS: { id: Tab; label: MessageKey }[] = [
  { id: "changes", label: "workspace.tab.changes" },
  { id: "terminal", label: "common.terminal" },
  { id: "preview", label: "workspace.tab.preview" },
];

export function ChangesPanel({ project, task, hidden = false }: { project: string; task: Task | null; hidden?: boolean }) {
  const t = useT();
  const [tab, setTab] = useState<Tab>("changes");
  // Once opened, the terminal stays alive behind the other tabs: unmounting it
  // would kill what runs there (like the dev server of the preview).
  const [terminalOpened, setTerminalOpened] = useState(false);
  const dir = task?.workspace?.worktree ?? project;

  return (
    <aside className="side-panel" aria-label={t("workspace.panel")} hidden={hidden}>
      <div className="side-panel__tabs" role="tablist">
        {TABS.map((item) => (
          <button
            key={item.id}
            role="tab"
            aria-selected={tab === item.id}
            className="side-tab"
            onClick={() => {
              setTab(item.id);
              if (item.id === "terminal") setTerminalOpened(true);
            }}
          >
            {t(item.label)}
          </button>
        ))}
      </div>
      <div className="side-panel__body">
        {tab === "changes" && <Changes project={project} task={task} />}
        {terminalOpened && (
          // One terminal per folder; it opens in the task's worktree.
          <div className="side-terminal" hidden={tab !== "terminal"}>
            <TerminalView key={dir} request={{ kind: "shell", path: dir }} active={tab === "terminal"} onExit={() => {}} />
          </div>
        )}
        {tab === "preview" && <Preview project={project} />}
      </div>
    </aside>
  );
}

const LOCAL_URL = /^https?:\/\/(localhost|127\.0\.0\.1)(:\d+)?(\/.*)?$/;

/** The project's dev server inside Hedy. Local addresses only. */
function Preview({ project }: { project: string }) {
  const t = useT();
  const storageKey = `hedy.preview.${project}`;
  const [url, setUrl] = useState(() => {
    try {
      return localStorage.getItem(storageKey) ?? "http://localhost:3000";
    } catch {
      return "http://localhost:3000";
    }
  });
  const [shown, setShown] = useState<string | null>(null);
  const [reloads, setReloads] = useState(0);
  const valid = LOCAL_URL.test(url.trim());

  return (
    <div className="preview">
      <form
        className="row"
        onSubmit={(e) => {
          e.preventDefault();
          if (!valid) return;
          setShown(url.trim());
          setReloads((n) => n + 1);
          try {
            localStorage.setItem(storageKey, url.trim());
          } catch {
            // Remembering the address is only a convenience.
          }
        }}
      >
        <input aria-label={t("workspace.preview.address")} className="mono preview__url" value={url} onChange={(e) => setUrl(e.target.value)} />
        <Button type="submit" variant="primary" small disabled={!valid}>
          {shown ? t("workspace.preview.reload") : t("workspace.preview.open")}
        </Button>
      </form>
      {!valid && <p className="error-text">{t("workspace.preview.localOnly")}</p>}
      {shown ? (
        <iframe
          key={reloads}
          className="preview__frame"
          src={shown}
          title={t("workspace.tab.preview")}
          sandbox="allow-scripts allow-same-origin allow-forms allow-popups"
        />
      ) : (
        <p className="empty">{t("workspace.preview.empty")}</p>
      )}
    </div>
  );
}

/** No worktree: a question or research never gets one; a project task gets it once the agent starts. */
function NoWorktree({ task }: { task: Task }) {
  const t = useT();
  const events = useTaskEvents(task.mode === "project" ? task.id : null);
  if (task.mode !== "project") return <p className="empty">{t("workspace.changes.noWorkspace")}</p>;
  if (!isOpen(task) || task.status === "review" || task.status === "failed") {
    return <p className="empty">{t("workspace.changes.noneMade")}</p>;
  }
  const planWaits = pendingCard(events.data ?? []) === "plan";
  return <p className="empty">{planWaits ? t("workspace.changes.afterPlan") : t("workspace.changes.notYet")}</p>;
}

function Changes({ project, task }: { project: string; task: Task | null }) {
  const api = useApi();
  const t = useT();
  const actions = useProjectActions();
  const editable = !!task?.workspace && isOpen(task);
  const diff = useTaskDiff(task?.id ?? null, editable);
  const [selected, setSelected] = useState<string | null>(null);
  const [reviewing, setReviewing] = useState(false);
  const commits = useQuery({ queryKey: ["commits", project], queryFn: () => api.recentCommits(project) });
  const accepted = useAcceptedToast(task);
  const reviewed = useReviewed(task?.id ?? "");
  const fileButtons = useRef(new Map<string, HTMLButtonElement>());

  // Both change the project's branch: its commits and uncommitted files.
  const changed = [
    ["commits", project],
    ["status", project],
  ];
  const discard = useAction((a, id: string) => a.discardTask(id), { invalidate: changed });

  if (!task) return <p className="empty">{t("workspace.changes.noTask")}</p>;
  if (!task.workspace) return <NoWorktree task={task} />;
  if (!isOpen(task)) {
    const branch = task.workspace.baseBranch;
    return (
      <div className="changes">
        <p className="empty">
          {task.status === "accepted" ? t("workspace.changes.accepted") : t("workspace.changes.discarded")}
        </p>
        {task.status === "accepted" && task.merge && (
          <div className="undo-merge">
            <Button disabled={accepted.undo.isPending} onClick={() => accepted.undo.mutate(task.id)}>
              {t("review.undoMerge")}
            </Button>
            <p className="muted small">{t("review.undoHint", { branch })}</p>
          </div>
        )}
      </div>
    );
  }

  const files = diff.data ?? [];
  const current = files.find((f) => f.path === selected) ?? files[0];
  const reviewedCount = files.filter((f) => reviewed.isReviewed(f)).length;

  // j / k move through the files, like in a code review.
  function onKeyDown(e: KeyboardEvent<HTMLDivElement>) {
    if (e.key !== "j" && e.key !== "k") return;
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    const target = e.target as HTMLElement;
    if (target.closest("input, textarea, select, [contenteditable='true']")) return;
    if (files.length === 0) return;
    e.preventDefault();
    const index = current ? files.indexOf(current) : -1;
    const next = files[Math.min(files.length - 1, Math.max(0, index + (e.key === "j" ? 1 : -1)))];
    if (!next) return;
    setSelected(next.path);
    fileButtons.current.get(next.path)?.focus();
  }

  return (
    <div className="changes" onKeyDown={onKeyDown}>
      {diff.isError && <p className="error-text">{errorMessage(diff.error, t)}</p>}
      {files.length === 0 ? (
        <p className="empty">{diff.isPending ? t("workspace.changes.loading") : t("workspace.changes.none")}</p>
      ) : (
        <>
          <p className="review-progress">
            <span>{t("review.progress", { n: reviewedCount, total: files.length })}</span>
            <span className="muted small">{t("review.keys")}</span>
          </p>
          <ul className="file-list" aria-label={t("workspace.changes.files")}>
            {files.map((f) => {
              const done = reviewed.isReviewed(f);
              return (
                <li key={f.path} className="file-row">
                  <input
                    type="checkbox"
                    className="file-row__check"
                    checked={done}
                    aria-label={t("review.markNamed", { path: f.path })}
                    onChange={() => reviewed.toggle(f)}
                  />
                  <button
                    ref={(el) => {
                      if (el) fileButtons.current.set(f.path, el);
                      else fileButtons.current.delete(f.path);
                    }}
                    className={`file-item${done ? " file-item--reviewed" : ""}`}
                    aria-current={f === current ? "true" : undefined}
                    onClick={() => setSelected(f.path)}
                  >
                    <span className="file-item__path">{f.path}</span>
                    {done && <span className="sr-only">, {t("review.reviewed")}</span>}
                    <span className="file-item__stats">
                      <span className="plus">+{f.additions}</span> <span className="minus">-{f.deletions}</span>
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>
          {current && (
            <>
              <label className="check review-mark">
                <input type="checkbox" checked={reviewed.isReviewed(current)} onChange={() => reviewed.toggle(current)} />
                {t("review.mark")} · <span className="mono">{current.path}</span>
              </label>
              <DiffView file={current} />
            </>
          )}
        </>
      )}

      <p className="muted small">
        {t("workspace.changes.branch")} <span className="mono">{task.workspace.branch}</span> {t("workspace.changes.from")}{" "}
        <span className="mono">{task.workspace.baseBranch}</span>
      </p>
      {commits.data && commits.data.length > 0 && (
        <p className="muted small">
          {t("common.recentCommits")} · {commits.data.map((c) => `${c.hash} ${c.subject}`).join(" · ")}
        </p>
      )}
      {/* Always in reach, however long the diff: the decision on these changes. */}
      <div className="changes__footer">
        <Button
          small
          variant="primary"
          disabled={files.length === 0 || task.status === "running"}
          title={task.status === "running" ? t("workspace.changes.waitForAgent") : undefined}
          onClick={() => setReviewing(true)}
        >
          {t("workspace.changes.accept")}
        </Button>
        <Button small onClick={() => actions.openEditor({ path: task.workspace?.worktree ?? project })}>{t("common.openInVsCode")}</Button>
        <ConfirmButton
          small
          className="changes__discard"
          confirmLabel={t("workspace.changes.discardConfirm")}
          onConfirm={() => discard.mutate(task.id)}
        >
          {t("workspace.changes.discard")}
        </ConfirmButton>
      </div>
      {reviewing && (
        <AcceptSheet
          task={task}
          onClose={() => setReviewing(false)}
          onAccepted={(branch) => accepted.show(task.id, branch)}
        />
      )}
    </div>
  );
}
