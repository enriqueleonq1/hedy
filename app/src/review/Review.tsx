import { useEffect, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent, type RefObject } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { errorMessage, useAction, useApi } from "../api/context";
import type { CodeCopy } from "../api/codeTypes";
import type { FileDiff, Task } from "../api/types";
import { ContextMenu, type MenuPoint } from "../components/ui/ContextMenu";
import { Button, ConfirmButton } from "../components/ui/Button";
import { MenuButton, type MenuItem } from "../components/ui/Menu";
import { useProjectActions } from "../components/projects";
import { useToast } from "../components/ui/Toast";
import { codeKeys, useFileNavigator } from "../files/navigator";
import { useT, type MessageKey } from "../i18n";
import { useTaskDiff, useTaskEvents } from "../tasks/hooks";
import { isOpen, pendingCard } from "../tasks/timeline";
import { AcceptSheet, useAcceptedToast } from "../screens/workspace/AcceptSheet";
import { DiffView, type DiffLayout } from "../screens/workspace/DiffView";
import { useReviewed } from "../screens/workspace/reviewed";
import { useReviewComments } from "./comments";
import { HistoryDialog } from "./HistoryDialog";
import { CheckIcon, CommentIcon, DisclosureIcon, ExternalIcon, FileIcon, SplitIcon, UnifiedIcon } from "./icons";
import "../styles/reviewer.css";

/** Narrower than this, the side-by-side view would squeeze the code: unified only. */
const SPLIT_MIN_WIDTH = 720;
const LAYOUT_KEY = "hedy.review.layout";

const STATUS: Record<FileDiff["status"], { letter: string; label: MessageKey }> = {
  added: { letter: "A", label: "reviewer.status.added" },
  modified: { letter: "M", label: "reviewer.status.modified" },
  deleted: { letter: "D", label: "reviewer.status.deleted" },
  renamed: { letter: "R", label: "reviewer.status.renamed" },
};

/** The width of an element as it resizes; `null` where it cannot be measured (tests). */
function useWidth(ref: RefObject<HTMLElement | null>): number | null {
  const [width, setWidth] = useState<number | null>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(([entry]) => entry && setWidth(entry.contentRect.width));
    observer.observe(el);
    return () => observer.disconnect();
  }, [ref]);
  return width;
}

function useLayoutChoice(): [DiffLayout, (layout: DiffLayout) => void] {
  const [layout, setLayout] = useState<DiffLayout>(() => {
    try {
      return localStorage.getItem(LAYOUT_KEY) === "split" ? "split" : "unified";
    } catch {
      return "unified";
    }
  });
  return [
    layout,
    (next) => {
      setLayout(next);
      try {
        localStorage.setItem(LAYOUT_KEY, next);
      } catch {
        // Remembering the view is only a convenience.
      }
    },
  ];
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

/** `facturas/` muted, `impuestos.py` in full: the name is what you scan for. */
function PathLabel({ path }: { path: string }) {
  const cut = path.lastIndexOf("/") + 1;
  return (
    <span className="rv-path" title={path}>
      {cut > 0 && <span className="rv-path__dir">{path.slice(0, cut)}</span>}
      <span className="rv-path__name">{path.slice(cut)}</span>
    </span>
  );
}

function StatusLetter({ status }: { status: FileDiff["status"] }) {
  const t = useT();
  const s = STATUS[status];
  return (
    <span className={`rv-status rv-status--${status}`} title={t(s.label)} aria-hidden>
      {s.letter}
    </span>
  );
}

/**
 * The Revisión tab: a task's changes reviewed like a pull request with the AI
 * on the other side. The changed files (marked reviewed one by one), one file's
 * diff with comments on its lines, and the decision: send the comments to the
 * AI, or accept (or discard) the changes.
 */
export function Changes({ project, task }: { project: string; task: Task | null }) {
  const api = useApi();
  const t = useT();
  const toast = useToast();
  const nav = useFileNavigator();
  const client = useQueryClient();
  const review = useReviewComments();
  const actions = useProjectActions();
  const editable = !!task?.workspace && isOpen(task);
  const diff = useTaskDiff(task?.id ?? null, editable);
  const [localSelected, setLocalSelected] = useState<string | null>(null);
  const [reviewing, setReviewing] = useState(false);
  const [preview, setPreview] = useState(false);
  const [history, setHistory] = useState<string | null>(null);
  const [menu, setMenu] = useState<{ at: MenuPoint; file: FileDiff } | null>(null);
  const commits = useQuery({ queryKey: ["commits", project], queryFn: () => api.recentCommits(project) });
  const accepted = useAcceptedToast(task);
  const reviewed = useReviewed(task?.id ?? "");
  const fileButtons = useRef(new Map<string, HTMLButtonElement>());
  const root = useRef<HTMLDivElement>(null);
  const fileHead = useRef<HTMLDivElement>(null);
  const width = useWidth(root);
  const [layoutChoice, setLayout] = useLayoutChoice();
  const splitFits = width === null || width >= SPLIT_MIN_WIDTH;
  const layout: DiffLayout = layoutChoice === "split" && splitFits ? "split" : "unified";
  const taskId = task?.id ?? "";
  const copy = useMemo<CodeCopy>(() => ({ kind: "task", task: taskId }), [taskId]);

  // The agent changed files: what the code views cached for this copy is stale.
  const updatedAt = diff.dataUpdatedAt;
  const seen = useRef(updatedAt);
  useEffect(() => {
    if (seen.current === updatedAt) return;
    seen.current = updatedAt;
    void client.invalidateQueries({ queryKey: codeKeys.copy(copy) });
  }, [updatedAt, client, copy]);

  // `selectFile` from elsewhere (the explorer's "Ver cambios", a diff link): show it.
  const selectSeq = review.selectSeq;
  useEffect(() => {
    if (selectSeq === 0) return;
    const path = review.selectedFile;
    if (!path) return;
    fileButtons.current.get(path)?.scrollIntoView?.({ block: "nearest" });
    if (review.selectedLine === null) fileHead.current?.scrollIntoView?.({ block: "nearest" });
    // Only a new request scrolls; the rest of the store changes often.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectSeq]);

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
  const selected = review.selectedFile ?? localSelected;
  const current = files.find((f) => f.path === selected) ?? files[0];
  const reviewedCount = files.filter((f) => reviewed.isReviewed(f)).length;
  const additions = files.reduce((n, f) => n + f.additions, 0);
  const deletions = files.reduce((n, f) => n + f.deletions, 0);
  const allReviewed = files.length > 0 && reviewedCount === files.length;
  const pending = review.pending;
  const sentCount = review.comments.length - pending.length;
  const running = task.status === "running";

  function select(path: string) {
    setLocalSelected(path);
    review.selectFile(path);
  }

  // j / k move through the files, like in a code review.
  function onKeyDown(e: KeyboardEvent<HTMLDivElement>) {
    if (e.key !== "j" && e.key !== "k") return;
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    const target = e.target as HTMLElement;
    if (target.closest("input:not([type=checkbox]), textarea, select, [contenteditable='true']")) return;
    if (files.length === 0) return;
    e.preventDefault();
    const index = current ? files.indexOf(current) : -1;
    const next = files[Math.min(files.length - 1, Math.max(0, index + (e.key === "j" ? 1 : -1)))];
    if (!next) return;
    select(next.path);
    fileButtons.current.get(next.path)?.focus();
  }

  const firstLine = (f: FileDiff) => f.hunks.flatMap((h) => h.lines).find((l) => l.kind !== "context")?.newLine ?? undefined;

  const fileMenu = (f: FileDiff): MenuItem[] => [
    {
      label: t("reviewer.file.open"),
      onSelect: () =>
        nav.open(
          f.status === "deleted"
            ? { copy, path: f.path, rev: "base", view: "file" }
            : { copy, path: f.path, line: firstLine(f), view: "file" },
        ),
    },
    {
      label: t("common.openInVsCode"),
      disabled: f.status === "deleted",
      onSelect: () => void api.openFileInEditor(copy, f.path, firstLine(f)).catch((e: unknown) => toast.error(errorMessage(e, t))),
    },
    {
      label: t("reviewer.file.copyPath"),
      onSelect: () =>
        void navigator.clipboard
          ?.writeText(f.path)
          .then(() => toast.info(t("reviewer.file.pathCopied")))
          .catch(() => {}),
    },
    {
      label: reviewed.isReviewed(f) ? t("reviewer.file.unmark") : t("reviewer.file.mark"),
      separatorBefore: true,
      onSelect: () => reviewed.toggle(f),
    },
    { label: t("reviewer.file.history"), onSelect: () => setHistory(f.path) },
  ];

  const pendingLabel = (n: number) => (n === 1 ? t("reviewer.pendingCountOne") : t("reviewer.pendingCount", { n }));
  const commentCount = (path: string) => pending.filter((c) => c.path === path).length;

  return (
    <div className="review-host" ref={root}>
      <div className="changes review" onKeyDown={onKeyDown}>
        {diff.isError && <p className="error-text">{errorMessage(diff.error, t)}</p>}
        {files.length === 0 ? (
          <p className="empty">{diff.isPending ? t("workspace.changes.loading") : t("workspace.changes.none")}</p>
        ) : (
          <>
            <header className="rv-summary">
              <p className="rv-summary__counts">
                <span>{files.length === 1 ? t("reviewer.filesOne") : t("reviewer.files", { n: files.length })}</span>
                <span className="rv-summary__lines">
                  <span className="plus">+{additions}</span> <span className="minus">−{deletions}</span>
                </span>
                <span className="rv-summary__progress">{t("review.progress", { n: reviewedCount, total: files.length })}</span>
              </p>
              <div className="rv-summary__tools" role="toolbar" aria-label={t("reviewer.tools")}>
                <div className="rv-toggle" role="group" aria-label={t("reviewer.layout")}>
                  <button
                    type="button"
                    className="rv-icon-btn"
                    aria-pressed={layout === "unified"}
                    aria-label={t("reviewer.layout.unified")}
                    title={t("reviewer.layout.unified")}
                    onClick={() => setLayout("unified")}
                  >
                    <UnifiedIcon />
                  </button>
                  <button
                    type="button"
                    className="rv-icon-btn"
                    aria-pressed={layout === "split"}
                    aria-label={t("reviewer.layout.split")}
                    title={splitFits ? t("reviewer.layout.split") : t("reviewer.layout.splitNarrow")}
                    disabled={!splitFits}
                    onClick={() => setLayout("split")}
                  >
                    <SplitIcon />
                  </button>
                </div>
                <button
                  type="button"
                  className="rv-icon-btn"
                  aria-label={t("common.openInVsCode")}
                  title={t("common.openInVsCode")}
                  onClick={() => actions.openEditor({ path: task.workspace?.worktree ?? project })}
                >
                  <ExternalIcon />
                </button>
              </div>
              <div
                className="rv-progress"
                role="progressbar"
                aria-label={t("reviewer.progressLabel")}
                aria-valuemin={0}
                aria-valuemax={files.length}
                aria-valuenow={reviewedCount}
              >
                <span className="rv-progress__fill" style={{ transform: `scaleX(${reviewedCount / files.length})` }} />
              </div>
            </header>

            <ul className="rv-files" aria-label={t("workspace.changes.files")} title={t("review.keys")}>
              {files.map((f) => {
                const done = reviewed.isReviewed(f);
                const notes = commentCount(f.path);
                return (
                  <li
                    key={f.path}
                    className={`rv-file${f === current ? " rv-file--current" : ""}${done ? " rv-file--reviewed" : ""}`}
                    onContextMenu={(e) => {
                      e.preventDefault();
                      setMenu({ at: { x: e.clientX, y: e.clientY }, file: f });
                    }}
                  >
                    <input
                      type="checkbox"
                      className="rv-file__check"
                      checked={done}
                      aria-label={t("review.markNamed", { path: f.path })}
                      onChange={() => reviewed.toggle(f)}
                    />
                    <button
                      ref={(el) => {
                        if (el) fileButtons.current.set(f.path, el);
                        else fileButtons.current.delete(f.path);
                      }}
                      className="rv-file__main"
                      aria-current={f === current ? "true" : undefined}
                      onClick={() => select(f.path)}
                    >
                      <StatusLetter status={f.status} />
                      <PathLabel path={f.path} />
                      <span className="sr-only">, {t(STATUS[f.status].label)}</span>
                      {done && <span className="sr-only">, {t("review.reviewed")}</span>}
                      {notes > 0 && (
                        <span className="rv-file__notes" title={pendingLabel(notes)}>
                          <CommentIcon />
                          {notes}
                          <span className="sr-only"> {pendingLabel(notes)}</span>
                        </span>
                      )}
                      <span className="rv-file__stats">
                        <span className="plus">+{f.additions}</span> <span className="minus">−{f.deletions}</span>
                      </span>
                    </button>
                    <MenuButton
                      label={t("reviewer.file.more", { path: f.path })}
                      className="menu-button rv-file__more"
                      items={fileMenu(f)}
                    />
                  </li>
                );
              })}
            </ul>

            {current && (
              <section className="rv-file-view" aria-label={current.path}>
                <div className="rv-filehead" ref={fileHead}>
                  <StatusLetter status={current.status} />
                  <PathLabel path={current.path} />
                  <span className="rv-filehead__stats">
                    <span className="plus">+{current.additions}</span> <span className="minus">−{current.deletions}</span>
                  </span>
                  <span className="rv-filehead__tools">
                    <button
                      type="button"
                      className="rv-icon-btn"
                      aria-label={t("reviewer.file.open")}
                      title={t("reviewer.file.open")}
                      onClick={() => fileMenu(current)[0]?.onSelect()}
                    >
                      <FileIcon />
                    </button>
                    <label className="rv-viewed">
                      <input type="checkbox" checked={reviewed.isReviewed(current)} onChange={() => reviewed.toggle(current)} />
                      {t("review.mark")}
                    </label>
                  </span>
                </div>
                <DiffView
                  key={current.path}
                  copy={copy}
                  file={current}
                  layout={layout}
                  focusLine={review.selectedFile === current.path ? review.selectedLine : null}
                  focusSeq={review.selectedFile === current.path ? review.selectSeq : 0}
                />
              </section>
            )}
          </>
        )}

        <p className="muted small rv-branch">
          {t("workspace.changes.branch")} <span className="mono">{task.workspace.branch}</span> {t("workspace.changes.from")}{" "}
          <span className="mono">{task.workspace.baseBranch}</span>
          {commits.data && commits.data.length > 0 && (
            <>
              {" · "}
              {t("common.recentCommits")} · {commits.data.map((c) => `${c.hash} ${c.subject}`).join(" · ")}
            </>
          )}
        </p>

        {/* Always in reach, however long the diff: the decision on these changes. */}
        <div className="changes__footer rv-bar">
          {pending.length > 0 && preview && (
            <pre className="rv-bar__preview" aria-label={t("reviewer.bar.previewLabel")} tabIndex={0}>
              {review.message()}
            </pre>
          )}
          {pending.length > 0 && (
            <div className="rv-bar__status">
              <button
                type="button"
                className="rv-bar__count"
                aria-expanded={preview}
                title={t("reviewer.bar.preview")}
                onClick={() => setPreview((p) => !p)}
              >
                <CommentIcon />
                {pending.length === 1 ? t("reviewer.bar.pendingOne") : t("reviewer.bar.pending", { n: pending.length })}
                <DisclosureIcon open={preview} />
              </button>
              {running && <span className="muted small">{t("reviewer.bar.queuedShort")}</span>}
            </div>
          )}
          <div className="rv-bar__row">
            {pending.length > 0 ? (
              <>
                <Button
                  small
                  variant="primary"
                  disabled={review.sending || !review.canSend}
                  title={running ? t("reviewer.bar.queued") : t("reviewer.bar.requestChanges")}
                  onClick={() => void review.sendToAi().then((ok) => ok && setPreview(false))}
                >
                  {t("reviewer.bar.send")}
                </Button>
                <Button
                  small
                  disabled={running}
                  title={running ? t("workspace.changes.waitForAgent") : undefined}
                  onClick={() => setReviewing(true)}
                >
                  {t("workspace.changes.accept")}
                </Button>
              </>
            ) : (
              <>
                {allReviewed ? (
                  <span className="rv-bar__done">
                    <CheckIcon />
                    {t("reviewer.bar.allReviewed")}
                  </span>
                ) : (
                  sentCount > 0 && <span className="muted small">{sentCount === 1 ? t("reviewer.bar.sentOne") : t("reviewer.bar.sent", { n: sentCount })}</span>
                )}
                <Button
                  small
                  variant="primary"
                  disabled={files.length === 0 || running}
                  title={running ? t("workspace.changes.waitForAgent") : undefined}
                  onClick={() => setReviewing(true)}
                >
                  {t("workspace.changes.accept")}
                </Button>
              </>
            )}
            <ConfirmButton
              small
              className="changes__discard"
              confirmLabel={t("workspace.changes.discardConfirm")}
              onConfirm={() => discard.mutate(task.id)}
            >
              {t("workspace.changes.discard")}
            </ConfirmButton>
          </div>
        </div>
        {menu && (
          <ContextMenu
            at={menu.at}
            label={t("reviewer.file.more", { path: menu.file.path })}
            items={fileMenu(menu.file)}
            onClose={() => setMenu(null)}
          />
        )}
        {history && <HistoryDialog copy={copy} path={history} onClose={() => setHistory(null)} />}
        {reviewing && (
          <AcceptSheet
            task={task}
            onClose={() => setReviewing(false)}
            onAccepted={(branch) => accepted.show(task.id, branch)}
          />
        )}
      </div>
    </div>
  );
}
