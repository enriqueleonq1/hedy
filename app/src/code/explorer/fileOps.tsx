// The explorer's file operations: new file / folder (an inline name in the
// tree), rename (inline, F2), duplicate and delete (to the system's trash, after
// a confirmation naming it). Only on editable copies; the backend checks again.

import { useRef, useState, type KeyboardEvent } from "react";

import { errorMessage, useApi } from "../../api/context";
import type { CodeCopy, CopyInfo, TreeEntry } from "../../api/codeTypes";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { useToast } from "../../components/ui/Toast";
import type { TreeRow } from "../../components/ui/treeModel";
import { useT, type MessageKey } from "../../i18n";
import { baseName, dirName } from "../copies";
import { copyLock, LOCK_MESSAGE, useAfterEdit } from "../editor/editability";
import { useUnsavedChanges } from "../editor/unsaved";

/** The draft row's path: never a real one (paths have no NUL). */
export const DRAFT_PATH = "\u0000draft";

/**
 * Why a name cannot be used (`null`: it can), like the backend's checks. A
 * file the project keeps from the AI is refused by the backend, which knows
 * the project's list.
 */
export function nameProblem(name: string, taken: (name: string) => boolean): MessageKey | null {
  if (!name.trim()) return "edit.name.empty";
  // eslint-disable-next-line no-control-regex
  if (name === "." || name === ".." || /[. ]$/.test(name) || name.startsWith(" ") || /[\u0000-\u001f/\\:*?"<>|]/.test(name)) {
    return "edit.name.invalid";
  }
  if (taken(name)) return "edit.name.exists";
  return null;
}

/**
 * Why a new file's name cannot be used. It may hold folders (`api/v2/x.py`,
 * created along the way), each part checked like a name.
 */
export function draftProblem(name: string, folder: string, exists: (path: string) => boolean): MessageKey | null {
  const clean = name.trim().replace(/^\/+|\/+$/g, "");
  if (!clean) return "edit.name.empty";
  const parts = clean.split("/");
  for (const part of parts) {
    const problem = nameProblem(part, () => false);
    if (problem) return problem;
  }
  return exists(joinPath(folder, parts.join("/"))) ? "edit.name.exists" : null;
}

/** `folder` + `name` as a copy path. */
export function joinPath(folder: string, name: string): string {
  return folder ? `${folder}/${name}` : name;
}

export type Draft = { kind: "file" | "folder"; folder: string; name: string };

export interface FileOps {
  /** Why nothing can be written in this copy (`null`: it can). */
  lockReason: string | null;
  draft: Draft | null;
  renaming: { path: string; name: string } | null;
  startCreate(kind: "file" | "folder", folder: string): void;
  startRename(path: string): void;
  askDelete(path: string, folder: boolean): void;
  duplicate(path: string): void;
  setDraftName(name: string): void;
  setRenameName(name: string): void;
  commitDraft(): Promise<void>;
  commitRename(): Promise<void>;
  cancel(): void;
  /** The confirmation dialog, when one is open. */
  dialog: React.ReactNode;
  busy: boolean;
}

/**
 * State and actions for the explorer's file operations. `exists` answers
 * whether a path is already in the tree; `onDone` is told what changed (a new
 * path to reveal, or a move/delete so the viewer can follow).
 */
export function useFileOps({
  copy,
  info,
  exists,
  dirtyPath,
  onCreated,
  onMoved,
  onFolderCreated,
}: {
  copy: CodeCopy;
  info: CopyInfo | undefined;
  exists: (path: string) => boolean;
  dirtyPath: string | null;
  onCreated: (path: string, kind: "file" | "folder") => void;
  /** `to: null`: deleted. */
  onMoved: (from: string, to: string | null) => void;
  onFolderCreated: (path: string) => void;
}): FileOps {
  const api = useApi();
  const t = useT();
  const toast = useToast();
  const afterEdit = useAfterEdit();
  const unsaved = useUnsavedChanges();
  const lock = copyLock(info);
  const lockReason = lock ? t("edit.explorer.locked", { reason: t(LOCK_MESSAGE[lock]) }) : null;
  const [draft, setDraft] = useState<Draft | null>(null);
  const [renaming, setRenaming] = useState<{ path: string; name: string } | null>(null);
  const [deleting, setDeleting] = useState<{ path: string; folder: boolean } | null>(null);
  const [busy, setBusy] = useState(false);

  const fail = (error: unknown) => toast.error(errorMessage(error, t));
  /** Unsaved changes in (or under) `path` are saved or dropped first. */
  const touchesDirty = (path: string) => !!dirtyPath && (dirtyPath === path || dirtyPath.startsWith(`${path}/`));
  const leaving = async (path: string) => !touchesDirty(path) || (await unsaved.confirmLeave());

  // A ref too: Enter and the blur that follows must not create twice.
  const working = useRef(false);
  const run = async (work: () => Promise<void>) => {
    if (working.current) return;
    working.current = true;
    setBusy(true);
    try {
      await work();
    } catch (error) {
      fail(error);
    } finally {
      working.current = false;
      setBusy(false);
    }
  };

  const commitDraft = async () => {
    if (!draft || working.current) return;
    const path = joinPath(draft.folder, draft.name.trim().replace(/^\/+|\/+$/g, ""));
    if (draftProblem(draft.name, draft.folder, exists)) return;
    await run(async () => {
      if (draft.kind === "file") await api.createFile(copy, path);
      else await api.createFolder(copy, path);
      setDraft(null);
      afterEdit(copy);
      if (draft.kind === "folder") onFolderCreated(path);
      onCreated(path, draft.kind);
    });
  };

  const commitRename = async () => {
    if (!renaming || working.current) return;
    const { path, name } = renaming;
    const clean = name.trim();
    if (clean === baseName(path)) {
      setRenaming(null);
      return;
    }
    const to = joinPath(dirName(path), clean);
    const sameName = (n: string) => n.toLowerCase() === baseName(path).toLowerCase();
    if (nameProblem(clean, (n) => !sameName(n) && exists(joinPath(dirName(path), n)))) return;
    if (!(await leaving(path))) return;
    await run(async () => {
      await api.renamePath(copy, path, to);
      setRenaming(null);
      afterEdit(copy);
      onMoved(path, to);
    });
  };

  const confirmDelete = async () => {
    if (!deleting) return;
    const { path } = deleting;
    if (!(await leaving(path))) return;
    await run(async () => {
      await api.deletePath(copy, path);
      setDeleting(null);
      afterEdit(copy);
      onMoved(path, null);
      toast.info(t("edit.deleted", { name: baseName(path) }));
    });
  };

  const dialog = deleting ? (
    <Dialog
      title={t("edit.delete.title", { name: baseName(deleting.path) })}
      onClose={() => setDeleting(null)}
      footer={
        <>
          <Button variant="ghost" onClick={() => setDeleting(null)}>
            {t("common.cancel")}
          </Button>
          <Button variant="danger" disabled={busy} onClick={() => void confirmDelete()}>
            {t("edit.delete.confirm")}
          </Button>
        </>
      }
    >
      <p className="mono small">{deleting.path}</p>
      <p className="muted">{deleting.folder ? t("edit.delete.folderText") : t("edit.delete.text")}</p>
    </Dialog>
  ) : null;

  return {
    lockReason,
    draft,
    renaming,
    busy,
    dialog,
    startCreate: (kind, folder) => {
      if (lockReason) return toast.info(lockReason);
      setRenaming(null);
      setDraft({ kind, folder, name: "" });
    },
    startRename: (path) => {
      if (lockReason) return toast.info(lockReason);
      setDraft(null);
      setRenaming({ path, name: baseName(path) });
    },
    askDelete: (path, folder) => {
      if (lockReason) return toast.info(lockReason);
      setDeleting({ path, folder });
    },
    duplicate: (path) => {
      if (lockReason) return void toast.info(lockReason);
      void run(async () => {
        const created = await api.duplicateFile(copy, path);
        afterEdit(copy);
        onCreated(created, "file");
        toast.info(t("edit.duplicated", { name: baseName(created) }), { durationMs: 2500 });
      });
    },
    setDraftName: (name) => setDraft((d) => (d ? { ...d, name } : d)),
    setRenameName: (name) => setRenaming((r) => (r ? { ...r, name } : r)),
    commitDraft,
    commitRename,
    cancel: () => {
      setDraft(null);
      setRenaming(null);
    },
  };
}

/** The draft row, inserted at the top of its folder's children. */
export function withDraft(rows: TreeRow<TreeEntry>[], draft: Draft | null): TreeRow<TreeEntry>[] {
  if (!draft) return rows;
  const at = draft.folder ? rows.findIndex((r) => r.node.path === draft.folder) : -1;
  const depth = at >= 0 ? (rows[at]?.depth ?? 0) + 1 : 0;
  const row: TreeRow<TreeEntry> = {
    node: {
      kind: "file",
      path: DRAFT_PATH,
      name: "",
      item: { path: DRAFT_PATH, status: null, oldPath: null, locked: false, ignored: false },
    },
    depth,
    parent: draft.folder,
  };
  const out = [...rows];
  out.splice(at + 1, 0, row);
  return out;
}

/** An inline name in the tree: Enter confirms, Esc cancels, the problem shows under it. */
export function NameInput({
  value,
  label,
  problem,
  onChange,
  onCommit,
  onCancel,
}: {
  value: string;
  label: string;
  problem: MessageKey | null;
  onChange: (value: string) => void;
  onCommit: () => void;
  onCancel: () => void;
}) {
  const t = useT();
  const shown = problem && value !== "" ? problem : null;
  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    e.stopPropagation();
    if (e.key === "Enter") {
      e.preventDefault();
      onCommit();
    } else if (e.key === "Escape") {
      e.preventDefault();
      onCancel();
    }
  };
  return (
    <span className="name-input" onMouseDown={(e) => e.stopPropagation()} onClick={(e) => e.stopPropagation()}>
      <input
        // A new name is typed right away.
        autoFocus
        className={`name-input__field${shown ? " name-input__field--invalid" : ""}`}
        value={value}
        aria-label={label}
        aria-invalid={!!shown}
        title={t("edit.name.hint")}
        spellCheck={false}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={onKeyDown}
        onFocus={(e) => {
          // Renaming selects the name without its extension, like file managers.
          const dot = e.target.value.lastIndexOf(".");
          e.target.setSelectionRange(0, dot > 0 ? dot : e.target.value.length);
        }}
        onBlur={() => (problem ? onCancel() : onCommit())}
      />
      {shown && (
        <span className="name-input__problem" role="alert">
          {t(shown, { name: value.trim() })}
        </span>
      )}
    </span>
  );
}
