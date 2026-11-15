// Whether a copy's files (and one file) can be edited now, and why not: the
// same rules the backend enforces (docs/decisions.md §27), so the UI explains
// a lock before the user tries.

import { useCallback } from "react";
import { useQueryClient } from "@tanstack/react-query";

import type { CodeCopy, CopyInfo, FileContent, Rev } from "../../api/codeTypes";
import { codeKeys } from "../../files/navigator";
import type { MessageKey } from "../../i18n";
import { folderKey } from "../../screens/workspace/ship/useShip";
import { diffKey } from "../../tasks/hooks";

/** Why a copy cannot be written: an accepted task's snapshot, or its agent works on it. */
export type CopyLock = "loading" | "snapshot" | "working";

/** Why one file cannot be edited (besides its copy's lock). */
export type FileLock = CopyLock | "revision" | "compare" | "protected" | "notText" | "missing";

export function copyLock(info: CopyInfo | undefined): CopyLock | null {
  if (!info) return "loading";
  if (!info.live) return "snapshot";
  if (info.agentWorking) return "working";
  return null;
}

/** Why the viewer cannot edit this file now (`null`: it can). */
export function fileLock(
  info: CopyInfo | undefined,
  content: FileContent | undefined,
  options: { rev?: Rev; comparing?: boolean },
): FileLock | null {
  const copy = copyLock(info);
  if (copy === "snapshot") return copy;
  if (options.rev) return "revision";
  if (content?.kind === "locked") return "protected";
  if (content?.kind === "missing") return "missing";
  if (content && content.kind !== "text") return "notText";
  if (options.comparing) return "compare";
  if (copy) return copy;
  return content ? null : "loading";
}

export const LOCK_MESSAGE: Record<FileLock, MessageKey> = {
  loading: "edit.lock.loading",
  snapshot: "edit.lock.snapshot",
  working: "edit.lock.working",
  revision: "edit.lock.revision",
  compare: "edit.lock.compare",
  protected: "edit.lock.protected",
  notText: "edit.lock.notText",
  missing: "edit.lock.missing",
};

/**
 * After a file of a copy changed (saved, created, renamed, deleted): what the
 * code views cached for it, the task's diff (its review reads the worktree) or
 * the folder's git status.
 */
export function useAfterEdit() {
  const client = useQueryClient();
  return useCallback(
    (copy: CodeCopy) => {
      void client.invalidateQueries({ queryKey: codeKeys.copy(copy) });
      if (copy.kind === "task") {
        void client.invalidateQueries({ queryKey: diffKey(copy.task) });
      } else {
        void client.invalidateQueries({ queryKey: folderKey(copy.project) });
        void client.invalidateQueries({ queryKey: ["status", copy.project] });
      }
    },
    [client],
  );
}
