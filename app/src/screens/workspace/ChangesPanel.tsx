import { useState } from "react";
import { useQuery } from "@tanstack/react-query";

import { errorMessage, useAction, useApi } from "../../api/context";
import type { Task } from "../../api/types";
import { useProjectActions } from "../../components/projects";
import { TerminalView } from "../../components/Terminal/TerminalView";
import { Button, ConfirmButton } from "../../components/ui/Button";
import { useToast } from "../../components/ui/Toast";
import { useT, type MessageKey } from "../../i18n";
import { useTaskDiff, useTaskEvents } from "../../tasks/hooks";
import { isOpen, pendingCard } from "../../tasks/timeline";
import { DiffView } from "./DiffView";

type Tab = "changes" | "terminal" | "preview";

const TABS: { id: Tab; label: MessageKey }[] = [
  { id: "changes", label: "workspace.tab.changes" },
  { id: "terminal", label: "common.terminal" },
  { id: "preview", label: "workspace.tab.preview" },
];

export function ChangesPanel({ project, task }: { project: string; task: Task | null }) {
  const t = useT();
  const [tab, setTab] = useState<Tab>("changes");
  // Once opened, the terminal stays alive behind the other tabs: unmounting it
  // would kill what runs there (like the dev server of the preview).
  const [terminalOpened, setTerminalOpened] = useState(false);
  const dir = task?.workspace?.worktree ?? project;

  return (
    <aside className="side-panel" aria-label={t("workspace.panel")}>
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
  const toast = useToast();
  const t = useT();
  const actions = useProjectActions();
  const editable = !!task?.workspace && isOpen(task);
  const diff = useTaskDiff(task?.id ?? null, editable);
  const [selected, setSelected] = useState<string | null>(null);
  const commits = useQuery({ queryKey: ["commits", project], queryFn: () => api.recentCommits(project) });

  // Both change the project's branch: its commits and uncommitted files.
  const changed = [
    ["commits", project],
    ["status", project],
  ];
  const accept = useAction((a, id: string) => a.acceptTask(id), {
    invalidate: changed,
    onSuccess: () =>
      toast.info(t("workspace.changes.merged", { branch: task?.workspace?.baseBranch ?? t("workspace.changes.baseBranch") })),
  });
  const discard = useAction((a, id: string) => a.discardTask(id), { invalidate: changed });

  if (!task) return <p className="empty">{t("workspace.changes.noTask")}</p>;
  if (!task.workspace) return <NoWorktree task={task} />;
  if (!isOpen(task)) {
    return (
      <p className="empty">
        {task.status === "accepted" ? t("workspace.changes.accepted") : t("workspace.changes.discarded")}
      </p>
    );
  }

  const files = diff.data ?? [];
  const current = files.find((f) => f.path === selected) ?? files[0];

  return (
    <div className="changes">
      {diff.isError && <p className="error-text">{errorMessage(diff.error, t)}</p>}
      {files.length === 0 ? (
        <p className="empty">{diff.isPending ? t("workspace.changes.loading") : t("workspace.changes.none")}</p>
      ) : (
        <>
          <ul className="file-list" aria-label={t("workspace.changes.files")}>
            {files.map((f) => (
              <li key={f.path}>
                <button
                  className="file-item"
                  aria-current={f === current ? "true" : undefined}
                  onClick={() => setSelected(f.path)}
                >
                  <span className="file-item__path">{f.path}</span>
                  <span className="file-item__stats">
                    <span className="plus">+{f.additions}</span> <span className="minus">-{f.deletions}</span>
                  </span>
                </button>
              </li>
            ))}
          </ul>
          {current && <DiffView file={current} />}
        </>
      )}

      <div className="actions">
        <Button
          variant="primary"
          disabled={files.length === 0 || accept.isPending || task.status === "running"}
          title={task.status === "running" ? t("workspace.changes.waitForAgent") : undefined}
          onClick={() => accept.mutate(task.id)}
        >
          {t("workspace.changes.accept")}
        </Button>
        <Button onClick={() => actions.openEditor({ path: task.workspace?.worktree ?? project })}>{t("common.openInVsCode")}</Button>
        <ConfirmButton confirmLabel={t("workspace.changes.discardConfirm")} onConfirm={() => discard.mutate(task.id)}>
          {t("workspace.changes.discard")}
        </ConfirmButton>
      </div>
      <p className="muted small">
        {t("workspace.changes.branch")} <span className="mono">{task.workspace.branch}</span> {t("workspace.changes.from")}{" "}
        <span className="mono">{task.workspace.baseBranch}</span>
      </p>
      {commits.data && commits.data.length > 0 && (
        <p className="muted small">
          {t("common.recentCommits")} · {commits.data.map((c) => `${c.hash} ${c.subject}`).join(" · ")}
        </p>
      )}
    </div>
  );
}
