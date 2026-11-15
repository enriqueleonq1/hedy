// The explorer: a copy's files as a virtualized tree with git status, filters
// (only changed files, ignored files), hover actions, a context menu and the
// file operations of an editable copy (new, rename, duplicate, delete).

import { useEffect, useMemo, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { errorMessage, useApi } from "../../api/context";
import type { CodeCopy, CopyInfo, TreeEntry } from "../../api/codeTypes";
import { useContextMenu, type ContextMenuItem } from "../../components/ui/ContextMenu";
import { allFolders, ancestorsOf, buildTree, flattenTree, parentOf, type TreeRow } from "../../components/ui/treeModel";
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
  DuplicateIcon,
  IgnoredIcon,
  MoreIcon,
  NewFileIcon,
  NewFolderIcon,
  RefreshIcon,
  RenameIcon,
  SparkIcon,
  TargetIcon,
  TrashIcon,
} from "../icons";
import { changedEntries, countChanged, rollUpStatus, STATUS_LETTER } from "./explorerModel";
import { DRAFT_PATH, draftProblem, joinPath, NameInput, nameProblem, useFileOps, withDraft } from "./fileOps";

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
  /** The open file has unsaved changes (a dot on its row). */
  dirtyPath?: string | null;
  /** A file was created or duplicated (open it). */
  onCreated?: (path: string) => void;
  /** A file or folder was renamed (`to`) or deleted (`null`). */
  onMoved?: (from: string, to: string | null) => void;
}

export function Explorer({
  copy,
  info,
  openPath,
  onOpen,
  onShowDiff,
  onHistory,
  revealRequest,
  focusSeq = 0,
  dirtyPath = null,
  onCreated,
  onMoved,
}: ExplorerProps) {
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
  const key = copyKey(copy);
  // Folders created here: git lists no empty folder, so they show until a file lands in them.
  const [created, setCreated] = useState<{ copy: string; folders: string[] }>({ copy: "", folders: [] });
  const entries = useMemo(() => {
    const listed = query.data?.entries ?? [];
    const extra = (created.copy === key ? created.folders : []).filter(
      (folder) => !listed.some((e) => e.path === `${folder}/` || e.path.startsWith(`${folder}/`)),
    );
    if (!extra.length) return listed;
    const folders = extra.map((folder) => ({ path: `${folder}/`, status: null, oldPath: null, locked: false, ignored: false }));
    return [...listed, ...folders].sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));
  }, [query.data, created, key]);
  const changed = useMemo(() => countChanged(entries), [entries]);
  const changedOnly = changedChoice?.copy === key ? changedChoice.on : copy.kind === "task" && changed > 0;

  const shown = useMemo(() => (changedOnly ? changedEntries(entries) : entries), [entries, changedOnly]);
  const root = useMemo(() => buildTree<TreeEntry>(shown, (e) => e.path), [shown]);
  const rollUp = useMemo(() => rollUpStatus(entries), [entries]);
  const open = useMemo(() => {
    if (!changedOnly) return expanded.copy === key ? expanded.open : new Set<string>();
    // Only changes: every folder open (they are few), unless closed by hand.
    return new Set(allFolders(root).filter((f) => !collapsedChanged.has(f)));
  }, [changedOnly, expanded, key, root, collapsedChanged]);
  const treeRows = useMemo(() => flattenTree(root, open), [root, open]);

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

  // Where a new file goes: the folder last clicked, or the folder of the file last opened.
  const [here, setHere] = useState("");
  const exists = (path: string) => entries.some((e) => e.path === path || e.path === `${path}/` || e.path.startsWith(`${path}/`));
  const ops = useFileOps({
    copy,
    info,
    exists,
    dirtyPath,
    onCreated: (path, kind) => {
      setHere(kind === "folder" ? path : parentOf(path));
      if (kind === "file") onCreated?.(path);
      reveal(path, kind === "folder", kind === "folder");
    },
    onMoved: (from, to) => {
      setCreated((old) => ({
        copy: key,
        folders: (old.copy === key ? old.folders : []).flatMap((f) =>
          f === from || f.startsWith(`${from}/`) ? (to === null ? [] : [`${to}${f.slice(from.length)}`]) : [f],
        ),
      }));
      onMoved?.(from, to);
      if (to) reveal(to, true);
      else tree.current?.focus();
    },
    onFolderCreated: (folder) =>
      setCreated((old) => ({ copy: key, folders: [...(old.copy === key ? old.folders : []), folder] })),
  });
  const rows = useMemo(() => withDraft(treeRows, ops.draft), [treeRows, ops.draft]);
  const create = (kind: "file" | "folder", folder: string) => {
    // Only changes shown: a new file would not be listed.
    if (changedOnly) setChangedChoice({ copy: key, on: false });
    if (folder) {
      setExpanded((old) => {
        const next = new Set(old.copy === key ? old.open : []);
        for (const f of [...ancestorsOf(folder), folder]) next.add(f);
        return { copy: key, open: next };
      });
    }
    ops.startCreate(kind, folder);
  };
  const endInline = () => {
    ops.cancel();
    tree.current?.focus();
  };

  const lockHint = ops.lockReason ?? undefined;
  const editItems = (path: string, folder: boolean, locked: boolean): ContextMenuItem[] => [
    ...(folder
      ? [
          { label: t("edit.newFile"), icon: <NewFileIcon />, disabled: !!lockHint, hint: lockHint, onSelect: () => create("file", path) },
          { label: t("edit.newFolder"), icon: <NewFolderIcon />, disabled: !!lockHint, hint: lockHint, onSelect: () => create("folder", path) },
        ]
      : []),
    {
      label: t("edit.rename"),
      icon: <RenameIcon />,
      shortcut: "F2",
      disabled: !!lockHint || locked,
      hint: lockHint,
      separatorBefore: !folder,
      onSelect: () => ops.startRename(path),
    },
    ...(folder
      ? []
      : [{ label: t("edit.duplicate"), icon: <DuplicateIcon />, disabled: !!lockHint || locked, hint: lockHint, onSelect: () => ops.duplicate(path) }]),
    {
      label: t("edit.delete"),
      icon: <TrashIcon />,
      shortcut: t("edit.deleteKey"),
      danger: true,
      disabled: !!lockHint || locked,
      hint: lockHint,
      onSelect: () => ops.askDelete(path, folder),
    },
  ];

  const itemsFor = (row: TreeRow<TreeEntry>): ContextMenuItem[] => {
    const path = row.node.path;
    if (row.node.kind === "folder") {
      return [
        ...editItems(path, true, false),
        { label: t("code.action.copyPath"), icon: <CopyIcon />, separatorBefore: true, onSelect: () => actions.copyPath(path) },
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
      ...editItems(path, false, entry.locked || entry.status === "deleted"),
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
        className={`icon-btn${lockHint ? " icon-btn--soft-disabled" : ""}`}
        aria-label={t("edit.newFile")}
        aria-disabled={!!lockHint || undefined}
        title={lockHint ?? t("edit.newFile")}
        onClick={() => create("file", here)}
      >
        <NewFileIcon />
      </button>
      <button
        type="button"
        className={`icon-btn${lockHint ? " icon-btn--soft-disabled" : ""}`}
        aria-label={t("edit.newFolder")}
        aria-disabled={!!lockHint || undefined}
        title={lockHint ?? t("edit.newFolder")}
        onClick={() => create("folder", here)}
      >
        <NewFolderIcon />
      </button>
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
        onContextMenu={(row, at) => row.node.path !== DRAFT_PATH && openMenu(row, at)}
        onRowFocus={(row) => setHere(row.node.kind === "folder" ? row.node.path : parentOf(row.node.path))}
        onRowKey={(row, key) => {
          if (row.node.path === DRAFT_PATH) return false;
          const locked = row.node.kind === "file" && (row.node.item.locked || row.node.item.status === "deleted");
          if (key === "F2" && !locked) {
            ops.startRename(row.node.path);
            return true;
          }
          if (key === "Delete" && !locked) {
            ops.askDelete(row.node.path, row.node.kind === "folder");
            return true;
          }
          return false;
        }}
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
          const draft = ops.draft;
          if (node.path === DRAFT_PATH && draft) {
            return (
              <>
                {draft.kind === "folder" ? <FolderIcon open={false} /> : <FileIcon path={draft.name || "x"} locked={false} />}
                <NameInput
                  value={draft.name}
                  label={draft.kind === "folder" ? t("edit.name.folder") : t("edit.name.file")}
                  problem={draftProblem(draft.name, draft.folder, exists)}
                  onChange={ops.setDraftName}
                  onCommit={() => void ops.commitDraft()}
                  onCancel={endInline}
                />
              </>
            );
          }
          const renaming = ops.renaming?.path === node.path ? ops.renaming : null;
          const renameInput = renaming && (
            <NameInput
              value={renaming.name}
              label={t("edit.name.rename", { name: node.name })}
              problem={nameProblem(
                renaming.name.trim(),
                (n) => n.toLowerCase() !== node.name.toLowerCase() && exists(joinPath(parentOf(node.path), n)),
              )}
              onChange={ops.setRenameName}
              onCommit={() => void ops.commitRename()}
              onCancel={endInline}
            />
          );
          if (node.kind === "folder") {
            const status = rollUp.get(node.path);
            return (
              <>
                <FolderIcon open={open.has(node.path)} />
                {renameInput || <span className="vtree__name">{node.name}</span>}
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
                hidden={!!renameInput}
              >
                {node.name}
              </span>
              {renameInput}
              {dirtyPath === node.path && (
                <span className="dirty-dot" title={t("edit.dirty")}>
                  <span className="sr-only">, {t("edit.dirty")}</span>
                </span>
              )}
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
      {ops.dialog}
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
