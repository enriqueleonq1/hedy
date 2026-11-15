import { useQuery, type QueryKey } from "@tanstack/react-query";

import { useApi } from "../../../api/context";
import type { ShipFailure } from "../../../api/types";

/** The folder's git status: refreshed on focus and after every git action. */
export const folderKey = (project: string): QueryKey => ["folderStatus", project];
export const shipKey = (taskId: string): QueryKey => ["shipState", taskId];

export function useFolderStatus(project: string | null) {
  const api = useApi();
  return useQuery({
    queryKey: folderKey(project ?? ""),
    queryFn: () => api.folderStatus(project ?? ""),
    enabled: !!project,
    refetchOnWindowFocus: true,
    staleTime: 5_000,
    retry: false,
  });
}

/** What a push or pull in `project` changes. */
export const gitKeys = (project: string): QueryKey[] => [
  folderKey(project),
  ["commits", project],
  ["status", project],
];

/** A push or pull failure the user can act on right away. */
export function nextStep(failure: ShipFailure): "pull" | "editor" | null {
  if (failure.kind === "rejected") return "pull";
  if (failure.kind === "diverged" || failure.kind === "localChanges") return "editor";
  return null;
}

// Accepting closes the review sheet (the task is no longer open), so the
// publish step that follows opens from the accepted view: the sheet leaves
// the task's id here for it.
const justAccepted = new Set<string>();

export function markJustAccepted(taskId: string) {
  justAccepted.add(taskId);
}

/** Whether `taskId` was accepted a moment ago (only answers `true` once). */
export function takeJustAccepted(taskId: string): boolean {
  return justAccepted.delete(taskId);
}
