// The explorer: a copy's files as a virtualized tree with git status, filters
// (only changed files, ignored files), hover actions and a context menu.

import { useEffect, useMemo, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { errorMessage, useApi } from "../../api/context";
import type { CodeCopy, CopyInfo, TreeEntry } from "../../api/codeTypes";
import { useContextMenu, type ContextMenuItem } from "../../components/ui/ContextMenu";
import { allFolders, ancestorsOf, buildTree, flattenTree, type TreeRow } from "../../components/ui/treeModel";
import { VirtualTree, type VirtualTreeHandle } from "../../components/ui/VirtualTree";
import { codeKeys, copyKey } from "../../files/navigator";
import { useT } from "../../i18n";
import { useFileActions } from "../fileActions";
import { FileIcon, FolderIcon } from "../fileIcons";
import {
  ChangedIcon,
  CollapseAllIcon,
  CopyIcon,
  DiffIcon,
  ExternalIcon,
  FilesIcon,
  FolderOpenIcon,
  HistoryIcon,
  IgnoredIcon,
  MoreIcon,
  RefreshIcon,
  SparkIcon,
  TargetIcon,
} from "../icons";
import { changedEntries, countChanged, rollUpStatus, STATUS_LETTER } from "./explorerModel";

export interface ExplorerProps {
  copy: CodeCopy;
  info: CopyInfo | undefined;
  /** The file open in the viewer: its row is selected (and revealed on `revealSeq`). */
  openPath: string | null;
  onOpen: (path: string) => void;
  onShowDiff: (path: string) => void;
  onHistory: (path: string) => void;
  /** Reveal this file (or folder) in the tree; a new `seq` asks again. */
  revealRequest?: { path: string; folder: boolean; seq: number } | null;
  /** Changes to put the keyboard focus in the tree. */
  focusSeq?: number;
}

export function Explorer({ copy, info, openPath, onOpen, onShowDiff, onHistory, revealRequest, focusSeq = 0 }: ExplorerProps) {
  const api = useApi();
  const t = useT();
  const client = useQueryClient();
  const actions = useFileActions(copy, info);
  const menu = useContextMenu();
  const tree = useRef<VirtualTreeHandle>(null);
  const [showIgnored, setShowIgnored] = useState(false);
  // Per copy: the user's choice; by default only the changes of a task with changes.
  const [changedChoice, setChangedChoice] = useState<{ copy: string; on: boolean } | null>(null);
  const [expanded, setExpanded] = useState<{ copy: string; open: Set<string> }>({ copy: "", open: new Set() });
  const [collapsedChanged, setCollapsedChanged] = useState<Set<string>>(new Set());

  const query = useQuery({
    queryKey: codeKeys.tree(copy, showIgnored),
    queryFn: () => api.codeTree(copy, { ignored: showIgnored }),
    placeholderData: (previous) => previous,
  });
  const entries = useMemo(() => query.data?.entries ?? [], [query.data]);
  const changed = useMemo(() => countChanged(entries), [entries]);
  const key = copyKey(copy);
  const changedOnly = changedChoice?.copy === key ? changedChoice.on : copy.kind === "task" && changed > 0;

  const shown = useMemo(() => (changedOnly ? changedEntries(entries) : entries), [entries, changedOnly]);
  const root = useMemo(() => buildTree<TreeEntry>(shown, (e) => e.path), [shown]);
  const rollUp = useMemo(() => rollUpStatus(entries), [entries]);
  const open = useMemo(() => {
    if (!changedOnly) return expanded.copy === key ? expanded.open : new Set<string>();
    // Only changes: every folder open (they are few), unless closed by hand.
    return new Set(allFolders(root).filter((f) => !collapsedChanged.has(f)));
  }, [changedOnly, expanded, key, root, collapsedChanged]);
  const rows = useMemo(() => flattenTree(root, open), [root, open]);

  const toggle = (path: string, on: boolean) => {
    if (changedOnly) {
      setCollapsedChanged((old) => {
        const next = new Set(old);
        if (on) next.delete(path);
        else next.add(path);
        return next;
      });
      return;
    }
    setExpanded((old) => {
      const next = new Set(old.copy === key ? old.open : []);
      if (on) next.add(path);
      else next.delete(path);
      return { copy: key, open: next };
    });
  };

  // Opening the way to a path is state; scrolling the tree to it waits for the rows.
  const [pending, setPending] = useState<{ path: string; focus: boolean; n: number } | null>(null);
  const reveal = (path: string, focus = false, folder = false) => {
    const way = folder ? [...ancestorsOf(path), path] : ancestorsOf(path);
    if (changedOnly && !entries.some((e) => (folder ? e.path.startsWith(`${path}/`) : e.path === path) && e.status)) {
      setChangedChoice({ copy: key, on: false });
    }
    setExpanded((old) => {
      const next = new Set(old.copy === key ? old.open : []);
      for (const f of way) next.add(f);
      return { copy: key, open: next };
    });
    setCollapsedChanged((old) => {
      const next = new Set(old);
      for (const f of way) next.delete(f);
      return next;
    });
    setPending((p) => ({ path, focus, n: (p?.n ?? 0) + 1 }));
  };
  useEffect(() => {
    if (pending) tree.current?.reveal(pending.path, { focus: pending.focus });
  }, [pending]);

  // Asked from outside (a breadcrumb, a link): show that file or folder in the tree.
  const [seenReveal, setSeenReveal] = useState(revealRequest?.seq ?? 0);
  if (revealRequest && revealRequest.seq !== seenReveal) {
    setSeenReveal(revealRequest.seq);
    reveal(revealRequest.path, false, revealRequest.folder);
  }

  // Ctrl/⌘ Shift E: the tree takes the focus, once it is there (it may still be loading).
  const wantsFocus = useRef(false);
  useEffect(() => {
    wantsFocus.current = focusSeq !== 0;
  }, [focusSeq]);
  useEffect(() => {
    if (!wantsFocus.current || !tree.current) return;
    wantsFocus.current = false;
    tree.current.focus();
  });

  const refresh = () => void client.invalidateQueries({ queryKey: codeKeys.copy(copy) });

  const itemsFor = (row: TreeRow<TreeEntry>): ContextMenuItem[] => {
    const path = row.node.path;
    if (row.node.kind === "folder") {
      return [
        { label: t("code.action.copyPath"), icon: <CopyIcon />, onSelect: () => actions.copyPath(path) },
        { label: t("code.action.copyRelativePath"), onSelect: () => actions.copyRelativePath(path) },
        { label: t("code.action.reveal"), icon: <FolderOpenIcon />, onSelect: () => actions.reveal(path), separatorBefore: true },
        { label: t("code.action.askAi"), icon: <SparkIcon />, onSelect: () => actions.askAi(path), separatorBefore: true },
      ];
    }
    const entry = row.node.item;
    return [
      { label: t("code.action.open"), icon: <FilesIcon />, shortcut: "Enter", onSelect: () => onOpen(path) },
      {
        label: t("code.action.showDiff"),
        icon: <DiffIcon />,
        disabled: !entry.status || entry.locked,
        onSelect: () => onShowDiff(path),
      },
      { label: t("code.action.history"), icon: <HistoryIcon />, disabled: entry.locked, onSelect: () => onHistory(path) },
      { label: t("code.action.copyPath"), icon: <CopyIcon />, onSelect: () => actions.copyPath(path), separatorBefore: true },
      { label: t("code.action.copyRelativePath"), onSelect: () => actions.copyRelativePath(path) },
      { label: t("code.action.reveal"), icon: <FolderOpenIcon />, onSelect: () => actions.reveal(path), separatorBefore: true },
      { label: t("common.openInVsCode"), icon: <ExternalIcon />, onSelect: () => actions.openInEditor(path) },
      { label: t("code.action.askAi"), icon: <SparkIcon />, onSelect: () => actions.askAi(path), separatorBefore: true },
    ];
  };

  const openMenu = (row: TreeRow<TreeEntry>, at: { x: number; y: number }) =>
    menu.open(at, itemsFor(row), t("code.explorer.menu", { name: row.node.name }));

  const toolbar = (
    <div className="code-toolbar" role="toolbar" aria-label={t("code.explorer.tools")}>
      <span className="code-toolbar__title">
        {changedOnly ? t("code.explorer.changedCount", { n: changed }) : t("code.explorer.allFiles")}
      </span>
      <button
        type="button"
        className="icon-btn"
        aria-pressed={changedOnly}
        aria-label={t("code.explorer.changedOnly")}
        title={t("code.explorer.changedOnly")}
        onClick={() => setChangedChoice({ copy: key, on: !changedOnly })}
      >
        <ChangedIcon />
      </button>
      <button
        type="button"
        className="icon-btn"
        aria-pressed={showIgnored}
        aria-label={t("code.explorer.showIgnored")}
        title={t("code.explorer.showIgnored")}
        onClick={() => setShowIgnored((on) => !on)}
      >
        <IgnoredIcon />
      </button>
      <button
        type="button"
        className="icon-btn"
        aria-label={t("code.explorer.revealActive")}
        title={t("code.explorer.revealActive")}
        disabled={!openPath}
        onClick={() => openPath && reveal(openPath, true)}
      >
        <TargetIcon />
      </button>
      <button
        type="button"
        className="icon-btn"
        aria-label={t("code.explorer.collapseAll")}
        title={t("code.explorer.collapseAll")}
        onClick={() => {
          if (changedOnly) setCollapsedChanged(new Set(allFolders(root)));
          else setExpanded({ copy: key, open: new Set() });
        }}
      >
        <CollapseAllIcon />
      </button>
      <button
        type="button"
        className="icon-btn"
        aria-label={t("code.explorer.refresh")}
        title={t("code.explorer.refresh")}
        onClick={refresh}
      >
        <RefreshIcon />
      </button>
    </div>
  );

  let body;
  if (query.isPending) {
    body = <TreeSkeleton label={t("code.explorer.loading")} />;
  } else if (query.isError) {
    body = (
      <div className="code-state code-state--compact" role="alert">
        <p className="code-state__title">{t("code.explorer.error")}</p>
        <p className="code-state__text">{errorMessage(query.error, t)}</p>
        <button type="button" className="btn btn--secondary btn--small" onClick={() => void query.refetch()}>
          {t("common.retry")}
        </button>
      </div>
    );
  } else {
    body = (
      <VirtualTree<TreeEntry>
        ref={tree}
        label={t("code.explorer.tree")}
        rows={rows}
        expanded={open}
        onToggle={toggle}
        selectedPath={openPath}
        onActivate={(row) => onOpen(row.node.path)}
        onContextMenu={openMenu}
        empty={
          <div className="code-state code-state--compact">
            <p className="code-state__title">{changedOnly ? t("code.explorer.noChanges") : t("code.explorer.empty")}</p>
            {changedOnly && (
              <button type="button" className="btn btn--secondary btn--small" onClick={() => setChangedChoice({ copy: key, on: false })}>
                {t("code.explorer.showAll")}
              </button>
            )}
          </div>
        }
        renderRow={(row) => {
          const node = row.node;
          if (node.kind === "folder") {
            const status = rollUp.get(node.path);
            return (
              <>
                <FolderIcon open={open.has(node.path)} />
                <span className="vtree__name">{node.name}</span>
                {status && <span className={`tree-dot tree-dot--${status}`} aria-label={t(`code.status.${status}`)} />}
                <RowActions>
                  <RowAction label={t("code.action.more")} onClick={(e) => openMenu(row, pointBelow(e))}>
                    <MoreIcon />
                  </RowAction>
                </RowActions>
              </>
            );
          }
          const entry = node.item;
          return (
            <>
              <FileIcon path={node.path} locked={entry.locked} />
              <span
                className={[
                  "vtree__name",
                  entry.status && `vtree__name--${entry.status}`,
                  entry.ignored && "vtree__name--ignored",
                  entry.locked && "vtree__name--locked",
                ]
                  .filter(Boolean)
                  .join(" ")}
                title={entry.oldPath ? t("code.explorer.renamedFrom", { path: entry.oldPath }) : undefined}
              >
                {node.name}
              </span>
              {entry.locked && <span className="sr-only">, {t("code.locked.short")}</span>}
              <RowActions>
                {entry.status && !entry.locked && (
                  <RowAction label={t("code.action.showDiff")} onClick={() => onShowDiff(node.path)}>
                    <DiffIcon />
                  </RowAction>
                )}
                <RowAction label={t("code.action.askAi")} onClick={() => actions.askAi(node.path)}>
                  <SparkIcon />
                </RowAction>
                <RowAction label={t("code.action.more")} onClick={(e) => openMenu(row, pointBelow(e))}>
                  <MoreIcon />
                </RowAction>
              </RowActions>
              {entry.status && (
                <span className={`tree-status tree-status--${entry.status}`} title={t(`code.status.${entry.status}`)}>
                  {STATUS_LETTER[entry.status]}
                  <span className="sr-only">, {t(`code.status.${entry.status}`)}</span>
                </span>
              )}
            </>
          );
        }}
      />
    );
  }

  return (
    <div className="explorer">
      {toolbar}
      {query.data?.truncated && <p className="code-notice">{t("code.explorer.truncated")}</p>}
      <div className="explorer__tree">{body}</div>
      {menu.element}
    </div>
  );
}

function pointBelow(e: React.MouseEvent<HTMLElement>) {
  const box = e.currentTarget.getBoundingClientRect();
  return { x: box.left, y: box.bottom + 2 };
}

/**
 * Actions that show on a row's hover (and on the active row while the tree has
 * focus). For the mouse: the keyboard reaches the same actions in the row's
 * context menu, so they stay out of the row's accessible name.
 */
export function RowActions({ children }: { children: React.ReactNode }) {
  return (
    <span className="row-actions" aria-hidden>
      {children}
    </span>
  );
}

export function RowAction({
  label,
  onClick,
  children,
}: {
  label: string;
  onClick: (e: React.MouseEvent<HTMLButtonElement>) => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      className="row-action"
      tabIndex={-1}
      aria-label={label}
      title={label}
      onMouseDown={(e) => e.preventDefault()}
      onClick={(e) => {
        e.stopPropagation();
        onClick(e);
      }}
    >
      {children}
    </button>
  );
}

/** Rows that shimmer while the tree loads. */
export function TreeSkeleton({ label, rows = 9 }: { label: string; rows?: number }) {
  return (
    <div className="tree-skeleton" role="status" aria-label={label}>
      {Array.from({ length: rows }, (_, i) => (
        <span key={i} className="tree-skeleton__row" style={{ width: `${40 + ((i * 37) % 45)}%`, marginLeft: (i % 3) * 12 }} />
      ))}
    </div>
  );
}
