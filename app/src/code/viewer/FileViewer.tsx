// The file viewer: one file of a copy on CodeMirror (read-only for now), with
// breadcrumbs, change markers against the base, find in file, go to line,
// history, blame, "antes de la IA" and an inline comparison with any commit.

import { useCallback, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { errorMessage, useApi } from "../../api/context";
import type { Blame, CodeCopy, CopyInfo, HistoryCommit, MatchRange, Rev, TaskTouch } from "../../api/codeTypes";
import { Button } from "../../components/ui/Button";
import { useContextMenu, type ContextMenuItem } from "../../components/ui/ContextMenu";
import { MenuButton } from "../../components/ui/Menu";
import { codeKeys } from "../../files/navigator";
import { timeAgo } from "../../format";
import { useT, type Translate } from "../../i18n";
import { baseName, lineReference } from "../copies";
import { CodeEditor, type CodeEditorHandle, type EditorPoint } from "../editor/CodeEditor";
import type { BlameMark } from "../editor/extensions";
import { lineMarksFromHunks } from "../editor/lineMarks";
import { useFileActions } from "../fileActions";
import {
  BackIcon,
  BlameIcon,
  CloseIcon,
  CommentIcon,
  CopyIcon,
  DiffIcon,
  ExternalIcon,
  HistoryIcon,
  LineIcon,
  LockIcon,
  MoreIcon,
  RewindIcon,
  SparkIcon,
} from "../icons";
import { modLabel } from "../keys";
import { useReviewBridge } from "../reviewBridge";
import { Floating } from "./Floating";
import { HistoryDrawer } from "./HistoryDrawer";
import { InlineDiff } from "./InlineDiff";
import { CodeSkeleton, CodeState, formatBytes, ImagePreview } from "./states";

/** What the viewer shows: a file, maybe at a line (with search hits) or at a revision. */
export interface OpenFile {
  path: string;
  line?: number;
  ranges?: MatchRange[];
  /** A commit, or `"base"` (before the AI); the copy's current file when absent. */
  rev?: Rev;
  /** How to name `rev` in the banner (a commit's subject). */
  revLabel?: string;
  /** Opens with the history drawer shown. */
  history?: boolean;
  /** Opens comparing the current file with this revision (an inline diff). */
  compare?: Rev;
  /** The search result it was opened from (its row stays selected). */
  select?: string;
  /** Changes on every request, so asking again for the same line reveals it again. */
  seq: number;
}

export interface FileViewerProps {
  copy: CodeCopy;
  info: CopyInfo | undefined;
  file: OpenFile;
  /** Show another revision of the same file (or the current one). */
  onChange: (file: OpenFile) => void;
  /** A breadcrumb: reveal that folder in the explorer. */
  onRevealFolder: (folder: string) => void;
  /** "Ver cambios": the review of this file (or an inline diff when there is no review). */
  onShowDiff: (path: string) => void;
  /** Open the file in an open task's copy (from the history). */
  onOpenTask: (task: TaskTouch, path: string) => void;
  /** Back to the list (narrow panels). */
  onBack: () => void;
  onClose: () => void;
}

type Popover =
  | { kind: "blame"; commit: string; at: { x: number; y: number }; pinned: boolean }
  | { kind: "lineBlame"; start: number; end: number; at: { x: number; y: number } }
  | { kind: "comment"; start: number; end: number; at: { x: number; y: number } };

/** CodeMirror's own texts (find panel, go to line, folding) in the user's language. */
function editorPhrases(t: Translate): Record<string, string> {
  return {
    Find: t("code.find.find"),
    next: t("code.find.next"),
    previous: t("code.find.previous"),
    all: t("code.find.all"),
    "match case": t("code.find.matchCase"),
    regexp: t("code.find.regexp"),
    "by word": t("code.find.byWord"),
    close: t("common.close"),
    "current match": t("code.find.current"),
    "on line": t("code.find.onLine"),
    "Go to line": t("code.find.gotoLine"),
    go: t("code.find.go"),
    "Fold line": t("code.find.fold"),
    "Unfold line": t("code.find.unfold"),
    "folded code": t("code.find.folded"),
    unfold: t("code.find.unfold"),
  };
}

export function blameMarks(blame: Blame, t: Translate): BlameMark[] {
  const commits = new Map(blame.commits.map((c) => [c.hash, c]));
  return blame.ranges.map((range) => {
    const commit = commits.get(range.commit);
    return {
      start: range.start,
      count: range.count,
      commit: range.commit,
      author: !commit ? "" : commit.uncommitted ? t("code.blame.uncommitted") : commit.author,
      date: commit && !commit.uncommitted ? timeAgo(commit.date, t) : "",
    };
  });
}

export function FileViewer({ copy, info, file, onChange, onRevealFolder, onShowDiff, onOpenTask, onBack, onClose }: FileViewerProps) {
  const api = useApi();
  const t = useT();
  const client = useQueryClient();
  const actions = useFileActions(copy, info);
  const review = useReviewBridge();
  const menu = useContextMenu();
  const editor = useRef<CodeEditorHandle>(null);
  const { path, rev } = file;
  const [blameOn, setBlameOn] = useState(false);
  const [historyOpen, setHistoryOpen] = useState(!!file.history);
  const compareOf = (r: Rev | undefined) => (r ? { rev: r, label: r === "base" ? (copy.kind === "task" ? t("code.viewer.beforeAi") : "HEAD") : r.slice(0, 7) } : null);
  const [compare, setCompare] = useState<{ rev: string; label: string } | null>(() => compareOf(file.compare));
  // A new request for the same file can ask for the history or a comparison again.
  const [seenSeq, setSeenSeq] = useState(file.seq);
  if (file.seq !== seenSeq) {
    setSeenSeq(file.seq);
    if (file.history) setHistoryOpen(true);
    if (file.compare) setCompare(compareOf(file.compare));
  }
  const [popover, setPopover] = useState<Popover | null>(null);
  const hoverTimer = useRef<number | undefined>(undefined);
  const name = baseName(path);
  const isTask = copy.kind === "task";

  const content = useQuery({ queryKey: codeKeys.file(copy, path, rev), queryFn: () => api.readFile(copy, path, rev) });
  const isText = content.data?.kind === "text";
  const diff = useQuery({
    queryKey: codeKeys.diff(copy, path),
    queryFn: () => api.fileDiff(copy, path),
    enabled: !rev && isText,
  });
  const blame = useQuery({
    queryKey: codeKeys.blame(copy, path, rev),
    queryFn: () => api.blame(copy, path, rev ? { rev } : undefined),
    enabled: blameOn && isText,
  });
  const compared = useQuery({
    queryKey: codeKeys.diff(copy, path, compare?.rev ?? "base"),
    queryFn: () => api.fileDiff(copy, path, { against: compare?.rev ?? "base" }),
    enabled: !!compare,
  });

  const lineMarks = useMemo(() => {
    const body = diff.data?.body;
    return !rev && body?.kind === "text" ? lineMarksFromHunks(body.hunks) : undefined;
  }, [diff.data, rev]);
  const marks = useMemo(() => (blameOn && blame.data ? blameMarks(blame.data, t) : null), [blameOn, blame.data, t]);
  const target = useMemo(() => (file.line ? { line: file.line, ranges: file.ranges } : null), [file.line, file.ranges]);
  const phrases = useMemo(() => editorPhrases(t), [t]);
  const closePopover = useCallback(() => setPopover(null), []);

  const beforeLabel = isTask ? t("code.viewer.beforeAi") : t("code.viewer.atHead");
  const atCurrent = () => onChange({ path, seq: file.seq + 1 });
  const viewCommit = (commit: { hash: string; short: string; subject?: string; summary?: string }) => {
    setCompare(null);
    onChange({ path, rev: commit.hash, revLabel: `${commit.short} · ${commit.subject ?? commit.summary ?? ""}`, seq: file.seq + 1 });
  };
  const compareWith = (commit: { hash: string; short: string }) => {
    if (rev) onChange({ path, seq: file.seq + 1 });
    setCompare({ rev: commit.hash, label: commit.short });
  };

  const menuPoint = useRef({ x: 0, y: 0 });
  const cursorLine = () => (isText ? (editor.current?.cursorLine() ?? 1) : undefined);

  const lineItems = (point: EditorPoint): ContextMenuItem[] => {
    const many = point.end > point.start;
    return [
      ...(point.selection
        ? [{ label: t("code.action.copy"), icon: <CopyIcon />, shortcut: modLabel("C"), onSelect: () => actions.copySelection(point.selection) }]
        : []),
      {
        label: many ? t("code.action.copyLines") : t("code.action.copyLine"),
        icon: <LineIcon />,
        onSelect: () => actions.copyLineReference(path, point.start, point.end),
      },
      {
        label: t("code.action.askAboutThis"),
        icon: <SparkIcon />,
        separatorBefore: true,
        onSelect: () => actions.askAi(path, { start: point.start, end: point.end }),
      },
      {
        label: t("code.action.comment"),
        icon: <CommentIcon />,
        disabled: !isTask || !!rev,
        onSelect: () => setPopover({ kind: "comment", start: point.start, end: point.end, at: menuPoint.current }),
      },
      {
        label: many ? t("code.action.blameLines") : t("code.action.blameLine"),
        icon: <BlameIcon />,
        separatorBefore: true,
        onSelect: () => setPopover({ kind: "lineBlame", start: point.start, end: point.end, at: menuPoint.current }),
      },
      { label: t("code.action.openHere"), icon: <ExternalIcon />, onSelect: () => actions.openInEditor(path, point.line) },
    ];
  };
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const mod = e.ctrlKey || e.metaKey;
    if (mod && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "f" && isText) {
      e.preventDefault();
      editor.current?.openSearch();
    } else if (mod && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "g" && isText) {
      e.preventDefault();
      editor.current?.gotoLine();
    }
  };

  const moreItems = [
    { label: t("code.viewer.find"), shortcut: modLabel("F"), disabled: !isText, onSelect: () => editor.current?.openSearch() },
    { label: t("code.viewer.gotoLine"), shortcut: modLabel("G"), disabled: !isText, onSelect: () => editor.current?.gotoLine() },
    { label: t("code.action.copyRelativePath"), separatorBefore: true, onSelect: () => actions.copyRelativePath(path) },
    { label: t("code.action.reveal"), onSelect: () => actions.reveal(path) },
    { label: t("code.action.askAi"), onSelect: () => actions.askAi(path) },
    {
      label: t("code.viewer.reload"),
      separatorBefore: true,
      onSelect: () => void client.invalidateQueries({ queryKey: codeKeys.copy(copy) }),
    },
    { label: t("code.viewer.close"), onSelect: onClose },
  ];

  const segments = path.split("/");
  const locked = content.data?.kind === "locked";

  let body;
  if (compare) {
    body = compared.isPending ? (
      <CodeSkeleton />
    ) : compared.isError ? (
      <CodeState tone="error" role="alert" title={t("code.viewer.error")} text={errorMessage(compared.error, t)} />
    ) : compared.data ? (
      <InlineDiff
        diff={compared.data}
        onOpenLine={(line) => {
          setCompare(null);
          onChange({ path, line, seq: file.seq + 1 });
        }}
      />
    ) : null;
  } else if (content.isPending) {
    body = <CodeSkeleton />;
  } else if (content.isError) {
    body = (
      <CodeState
        tone="error"
        role="alert"
        title={t("code.viewer.error")}
        text={errorMessage(content.error, t)}
        actions={
          <Button small onClick={() => void content.refetch()}>
            {t("common.retry")}
          </Button>
        }
      />
    );
  } else {
    const data = content.data;
    const openButton = (
      <Button small onClick={() => actions.openInEditor(path)}>
        <ExternalIcon />
        {t("common.openInVsCode")}
      </Button>
    );
    switch (data.kind) {
      case "text":
        body = (
          <CodeEditor
            ref={editor}
            doc={data.text}
            path={path}
            label={t("code.viewer.editorLabel", { name })}
            lineMarks={lineMarks}
            blame={marks}
            target={target}
            seq={file.seq}
            phrases={phrases}
            onContextMenu={(point, e) => {
              menuPoint.current = { x: e.clientX, y: e.clientY };
              menu.open(e, lineItems(point), t("code.viewer.lineMenu", { line: point.line }));
            }}
            onBlameClick={(commit, _line, anchor) => {
              const box = anchor.getBoundingClientRect();
              setPopover({ kind: "blame", commit, at: { x: box.left, y: box.bottom + 4 }, pinned: true });
            }}
            onBlameHover={(commit, anchor) => {
              window.clearTimeout(hoverTimer.current);
              if (!commit || !anchor) {
                hoverTimer.current = window.setTimeout(
                  () => setPopover((p) => (p?.kind === "blame" && !p.pinned ? null : p)),
                  250,
                );
                return;
              }
              const box = anchor.getBoundingClientRect();
              hoverTimer.current = window.setTimeout(
                () =>
                  setPopover((p) =>
                    p && p.kind !== "blame" ? p : { kind: "blame", commit, at: { x: box.left, y: box.bottom + 4 }, pinned: false },
                  ),
                350,
              );
            }}
          />
        );
        break;
      case "image":
        body = <ImagePreview src={data.dataUrl} size={data.size} name={name} />;
        break;
      case "binary":
        body = (
          <CodeState
            icon={<DiffIcon />}
            title={t("code.viewer.binary")}
            text={t("code.viewer.binaryText", { size: formatBytes(data.size) })}
            actions={openButton}
          />
        );
        break;
      case "tooLarge":
        body = (
          <CodeState
            title={t("code.viewer.tooLarge")}
            text={t("code.viewer.tooLargeText", { size: formatBytes(data.size) })}
            actions={openButton}
          />
        );
        break;
      case "locked":
        body = (
          <CodeState
            tone="locked"
            icon={<LockIcon size={22} />}
            title={t("code.locked.title")}
            text={t("code.locked.text")}
            actions={openButton}
          />
        );
        break;
      case "missing":
        body =
          rev === "base" ? (
            <CodeState
              title={t("code.viewer.newFile")}
              text={isTask ? t("code.viewer.newFileText") : t("code.viewer.newFileHeadText")}
              actions={<Button small onClick={atCurrent}>{t("code.viewer.backToCurrent")}</Button>}
            />
          ) : (
            <CodeState
              title={t("code.viewer.missing")}
              text={rev ? t("code.viewer.missingAtRev") : t("code.viewer.missingText")}
              actions={
                rev ? (
                  <Button small onClick={atCurrent}>{t("code.viewer.backToCurrent")}</Button>
                ) : info?.base ? (
                  <Button small onClick={() => onChange({ path, rev: "base", seq: file.seq + 1 })}>{beforeLabel}</Button>
                ) : undefined
              }
            />
          );
        break;
    }
  }

  return (
    <div className="viewer" onKeyDown={onKeyDown}>
      <header className="viewer__head">
        <button type="button" className="icon-btn viewer__back" aria-label={t("code.viewer.back")} title={t("code.viewer.back")} onClick={onBack}>
          <BackIcon />
        </button>
        <nav className="breadcrumbs" aria-label={t("code.viewer.breadcrumbs")}>
          <ol>
            {segments.map((segment, i) => {
              const last = i === segments.length - 1;
              const folder = segments.slice(0, i + 1).join("/");
              return (
                <li key={folder}>
                  {last ? (
                    <span className="breadcrumbs__file" aria-current="page" title={path}>
                      {segment}
                    </span>
                  ) : (
                    <button type="button" className="breadcrumbs__part" title={t("code.viewer.revealFolder", { folder })} onClick={() => onRevealFolder(folder)}>
                      {segment}
                    </button>
                  )}
                </li>
              );
            })}
          </ol>
        </nav>
        <div className="viewer__tools" role="toolbar" aria-label={t("code.viewer.tools")}>
          <ToolButton label={t("code.action.showDiff")} disabled={locked} onClick={() => onShowDiff(path)}>
            <DiffIcon />
          </ToolButton>
          <ToolButton label={t("code.action.history")} expanded={historyOpen} disabled={locked} onClick={() => setHistoryOpen((o) => !o)}>
            <HistoryIcon />
          </ToolButton>
          <ToolButton label={t("code.viewer.blame")} pressed={blameOn} disabled={!isText || !!compare} onClick={() => setBlameOn((o) => !o)}>
            <BlameIcon />
          </ToolButton>
          {info?.base && (
            <ToolButton
              label={beforeLabel}
              pressed={rev === "base"}
              disabled={locked}
              onClick={() => (rev === "base" ? atCurrent() : (setCompare(null), onChange({ path, rev: "base", seq: file.seq + 1 })))}
            >
              <RewindIcon />
            </ToolButton>
          )}
          <ToolButton label={t("code.viewer.openAtLine")} onClick={() => actions.openInEditor(path, cursorLine())}>
            <ExternalIcon />
          </ToolButton>
          <ToolButton label={t("code.action.copyPath")} onClick={() => actions.copyPath(path)}>
            <CopyIcon />
          </ToolButton>
          <MenuButton label={t("code.action.more")} className="icon-btn" items={moreItems}>
            <MoreIcon />
          </MenuButton>
          <ToolButton label={t("code.viewer.close")} onClick={onClose}>
            <CloseIcon />
          </ToolButton>
        </div>
      </header>

      {rev && !compare && (
        <div className="viewer__banner" role="status">
          <span className="viewer__banner-text">
            <RewindIcon />
            {rev === "base"
              ? isTask
                ? t("code.viewer.bannerBefore", { rev: info?.base?.slice(0, 7) ?? "" })
                : t("code.viewer.bannerHead", { rev: info?.base?.slice(0, 7) ?? "" })
              : t("code.viewer.bannerCommit", { rev: file.revLabel ?? rev.slice(0, 7) })}
            <span className="viewer__banner-tag">{t("code.viewer.readOnly")}</span>
          </span>
          {rev !== "base" && (
            <button type="button" className="link-btn" onClick={() => compareWith({ hash: rev, short: rev.slice(0, 7) })}>
              {t("code.viewer.compareCurrent")}
            </button>
          )}
          <button type="button" className="link-btn" onClick={atCurrent}>
            {t("code.viewer.backToCurrent")}
          </button>
        </div>
      )}
      {compare && (
        <div className="viewer__banner" role="status">
          <span className="viewer__banner-text">
            <DiffIcon />
            {t("code.viewer.comparing", { rev: compare.label })}
          </span>
          <button type="button" className="link-btn" onClick={() => setCompare(null)}>
            {t("code.viewer.closeCompare")}
          </button>
        </div>
      )}

      <div className="viewer__body">
        {body}
        {historyOpen && (
          <HistoryDrawer
            copy={copy}
            path={path}
            onClose={() => setHistoryOpen(false)}
            onView={(commit: HistoryCommit) => viewCommit(commit)}
            onCompare={(commit) => compareWith(commit)}
            onOpenTask={(task) => onOpenTask(task, path)}
          />
        )}
      </div>

      {popover?.kind === "blame" && blame.data && (
        <BlameCard
          blame={blame.data}
          commits={[popover.commit]}
          at={popover.at}
          onClose={closePopover}
          onHover={(inside) => {
            window.clearTimeout(hoverTimer.current);
            if (!inside) hoverTimer.current = window.setTimeout(() => setPopover((p) => (p?.kind === "blame" && !p.pinned ? null : p)), 250);
          }}
          onView={(c) => {
            closePopover();
            viewCommit(c);
          }}
          onCompare={(c) => {
            closePopover();
            compareWith(c);
          }}
        />
      )}
      {popover?.kind === "lineBlame" && (
        <LineBlame
          copy={copy}
          path={path}
          rev={rev}
          start={popover.start}
          end={popover.end}
          at={popover.at}
          onClose={closePopover}
          onView={(c) => {
            closePopover();
            viewCommit(c);
          }}
          onCompare={(c) => {
            closePopover();
            compareWith(c);
          }}
        />
      )}
      {popover?.kind === "comment" && (
        <CommentBox
          reference={lineReference(path, popover.start, popover.end)}
          at={popover.at}
          onClose={closePopover}
          onSubmit={(text) => {
            review.add({ copy, path, line: popover.start, endLine: popover.end > popover.start ? popover.end : undefined, side: "new", text });
            closePopover();
          }}
        />
      )}
      {menu.element}
    </div>
  );
}

function ToolButton({
  label,
  onClick,
  pressed,
  expanded,
  disabled,
  children,
}: {
  label: string;
  onClick: () => void;
  pressed?: boolean;
  expanded?: boolean;
  disabled?: boolean;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      className="icon-btn"
      aria-label={label}
      title={label}
      aria-pressed={pressed}
      aria-expanded={expanded}
      disabled={disabled}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

type CommitLike = { hash: string; short: string; summary: string };

/** A blame entry's commit: who, when, what; view the file then or compare with now. */
function BlameCard({
  blame,
  commits,
  at,
  onClose,
  onHover,
  onView,
  onCompare,
  label,
}: {
  blame: Blame;
  commits: string[];
  at: { x: number; y: number };
  onClose: () => void;
  onHover?: (inside: boolean) => void;
  onView: (commit: CommitLike) => void;
  onCompare: (commit: CommitLike) => void;
  label?: string;
}) {
  const t = useT();
  const byHash = new Map(blame.commits.map((c) => [c.hash, c]));
  const shown = commits.map((hash) => byHash.get(hash)).filter((c) => !!c);
  return (
    <Floating
      at={at}
      label={label ?? t("code.blame.card")}
      onClose={onClose}
      className="blame-card"
      onPointerEnter={() => onHover?.(true)}
      onPointerLeave={() => onHover?.(false)}
    >
      {shown.length === 0 && <p className="muted small">{t("code.blame.none")}</p>}
      {shown.map((commit) => (
        <div key={commit.hash} className="blame-card__commit">
          <p className="blame-card__summary">{commit.uncommitted ? t("code.blame.uncommittedText") : commit.summary}</p>
          <p className="blame-card__meta">
            {commit.uncommitted ? t("code.blame.uncommitted") : commit.author}
            {!commit.uncommitted && (
              <>
                {" · "}
                {timeAgo(commit.date, t)} · <span className="mono">{commit.short}</span>
              </>
            )}
          </p>
          {!commit.uncommitted && (
            <div className="blame-card__actions">
              <button type="button" className="link-btn" onClick={() => onView(commit)}>
                {t("code.blame.viewThen")}
              </button>
              <button type="button" className="link-btn" onClick={() => onCompare(commit)}>
                {t("code.viewer.compareCurrent")}
              </button>
            </div>
          )}
        </div>
      ))}
    </Floating>
  );
}

/** Blame for a range of lines, asked from the context menu. */
function LineBlame({
  copy,
  path,
  rev,
  start,
  end,
  at,
  onClose,
  onView,
  onCompare,
}: {
  copy: CodeCopy;
  path: string;
  rev: Rev | undefined;
  start: number;
  end: number;
  at: { x: number; y: number };
  onClose: () => void;
  onView: (commit: CommitLike) => void;
  onCompare: (commit: CommitLike) => void;
}) {
  const api = useApi();
  const t = useT();
  const query = useQuery({
    queryKey: [...codeKeys.blame(copy, path, rev), start, end],
    queryFn: () => api.blame(copy, path, { ...(rev ? { rev } : {}), lines: { start, end } }),
  });
  const label = end > start ? t("code.blame.linesTitle", { start, end }) : t("code.blame.lineTitle", { line: start });
  if (!query.data) {
    return (
      <Floating at={at} label={label} onClose={onClose} className="blame-card">
        <p className="blame-card__title">{label}</p>
        <p className="muted small" role={query.isError ? "alert" : "status"}>
          {query.isError ? errorMessage(query.error, t) : t("code.blame.loading")}
        </p>
      </Floating>
    );
  }
  const order = [...new Set(query.data.ranges.map((r) => r.commit))];
  return (
    <BlameCard blame={query.data} commits={order} at={at} onClose={onClose} onView={onView} onCompare={onCompare} label={label} />
  );
}

/** "Comentar en la revisión": a short note on the lines, for the review. */
function CommentBox({
  reference,
  at,
  onClose,
  onSubmit,
}: {
  reference: string;
  at: { x: number; y: number };
  onClose: () => void;
  onSubmit: (text: string) => void;
}) {
  const t = useT();
  const [text, setText] = useState("");
  const submit = () => {
    if (text.trim()) onSubmit(text.trim());
  };
  return (
    <Floating at={at} label={t("code.comment.title")} onClose={onClose} className="comment-box" autoFocus>
      <p className="comment-box__ref mono">{reference}</p>
      <textarea
        rows={3}
        value={text}
        aria-label={t("code.comment.label")}
        placeholder={t("code.comment.placeholder")}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
            e.preventDefault();
            submit();
          }
        }}
      />
      <div className="comment-box__actions">
        <span className="muted small">{t("code.comment.hint", { keys: modLabel("Enter") })}</span>
        <Button small variant="ghost" onClick={onClose}>
          {t("common.cancel")}
        </Button>
        <Button small variant="primary" disabled={!text.trim()} onClick={submit}>
          {t("code.comment.submit")}
        </Button>
      </div>
    </Floating>
  );
}
