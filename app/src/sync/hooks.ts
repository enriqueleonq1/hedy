// Personal sync in the UI (docs/decisions.md §34): its status, running it
// now, and refreshing what a sync may have brought in.

import { useEffect } from "react";
import { useMutation, useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";

import { errorMessage, OVERVIEW_KEY, useApi } from "../api/context";
import type { SyncDevice, SyncStatus } from "../api/types";
import { useToast } from "../components/ui/Toast";
import { useT, type Translate } from "../i18n";
import { TEMPLATES_KEY } from "../tasks/hooks";
import { THEMES_KEY } from "../theme/ThemeProvider";
import { syncErrorTitle } from "../screens/sync/SyncError";

export const SYNC_KEY = ["sync"] as const;
export const SYNC_STATUS_KEY = [...SYNC_KEY, "status"] as const;
export const SYNC_CONFLICTS_KEY = [...SYNC_KEY, "conflicts"] as const;
export const MEMORY_HOMES_KEY = [...SYNC_KEY, "homes"] as const;
export const memoryHomeKey = (project: string) => [...SYNC_KEY, "home", project] as const;

/** What a sync may change: its own state, the teams, memory, skills, templates, themes. */
export function refreshAfterSync(client: QueryClient): Promise<unknown> {
  return Promise.all(
    [SYNC_KEY, ["teams"], ["memory"], ["skills"], ["status"], TEMPLATES_KEY, THEMES_KEY, OVERVIEW_KEY].map((queryKey) =>
      client.invalidateQueries({ queryKey }),
    ),
  );
}

/** Once, at the root: the sync that runs on its own finished, so what it may have changed is read again. */
export function useSyncEvents() {
  const api = useApi();
  const client = useQueryClient();
  useEffect(() => api.onSyncUpdated(() => void refreshAfterSync(client)), [api, client]);
}

/** `null` while it is not set up. */
export function useSyncStatus() {
  const api = useApi();
  return useQuery({ queryKey: SYNC_STATUS_KEY, queryFn: () => api.syncStatus() });
}

/** "Sincronizar ahora": says what it sent and received, or why it failed. */
export function useSyncNow() {
  const api = useApi();
  const client = useQueryClient();
  const toast = useToast();
  const t = useT();
  return useMutation({
    mutationFn: () => api.syncNow(),
    onSuccess: (report) => toast.info(t("sync.done", { sent: report.sent, received: report.received })),
    // Plain words ("Sin conexión"…); git's own message is in the section's "Detalles".
    onError: (error) => toast.error(t(syncErrorTitle(errorMessage(error, t)))),
    onSettled: () => refreshAfterSync(client),
  });
}

export function deviceName(device: SyncDevice | undefined, t: Translate): string {
  return device?.name.trim() || t("sync.unnamed");
}

/** A PC's name by its id, "esta PC" for this one. */
export function pcName(status: SyncStatus | null | undefined, id: string, t: Translate): string {
  const device = status?.devices.find((d) => d.id === id);
  return device?.thisPc ? t("sync.thisPc") : deviceName(device, t);
}

/** "alex/hedy-memory" from any git address, for a short line. */
export function repoLabel(url: string): string {
  const path = url
    .trim()
    .replace(/^[a-z+]+:\/\/[^/]+\//i, "")
    .replace(/^[^/:]*:(?!\/)/, "");
  return path.replace(/\.git$/, "") || url;
}

/** The one-line state: failed, changes to review, not synced yet, or up to date. */
export function syncState(status: SyncStatus): "error" | "review" | "pending" | "ok" {
  if (status.lastError) return "error";
  if (status.conflicts > 0) return "review";
  if (status.lastSync === null) return "pending";
  return "ok";
}
