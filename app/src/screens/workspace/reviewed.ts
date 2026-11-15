import { useCallback, useState } from "react";

import type { FileDiff } from "../../api/types";

/**
 * A file's version as reviewed: when the agent changes it again, the mark
 * goes away, since what was reviewed is no longer what is there.
 */
export function fileVersion(file: FileDiff): string {
  const lines = file.hunks.reduce((n, h) => n + h.lines.length, 0);
  return `${file.path}\u0000${file.additions}:${file.deletions}:${lines}`;
}

const key = (taskId: string) => `hedy.reviewed.${taskId}`;

function load(taskId: string): Set<string> {
  try {
    const raw = localStorage.getItem(key(taskId));
    const list: unknown = raw ? JSON.parse(raw) : [];
    return new Set(Array.isArray(list) ? list.filter((v): v is string => typeof v === "string") : []);
  } catch {
    return new Set();
  }
}

/**
 * Files marked "Revisado" in a task's diff. Local to this window (a
 * convenience): kept per task in the browser storage when it is available.
 */
export function useReviewed(taskId: string) {
  const [state, setState] = useState(() => ({ taskId, marks: load(taskId) }));
  const marks = state.taskId === taskId ? state.marks : load(taskId);

  const toggle = useCallback(
    (file: FileDiff) => {
      const next = new Set(state.taskId === taskId ? state.marks : load(taskId));
      const version = fileVersion(file);
      if (next.has(version)) next.delete(version);
      else next.add(version);
      setState({ taskId, marks: next });
      try {
        localStorage.setItem(key(taskId), JSON.stringify([...next]));
      } catch {
        // Only a convenience: the marks still work for this session.
      }
    },
    [state, taskId],
  );

  const isReviewed = useCallback((file: FileDiff) => marks.has(fileVersion(file)), [marks]);
  return { isReviewed, toggle };
}
