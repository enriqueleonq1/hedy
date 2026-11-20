// Team memory in the UI (docs/decisions.md §35): the teams, one team's
// state, and the actions that refresh what they change.

import { useMutation, useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";

import { errorMessage, OVERVIEW_KEY, useAction, useApi } from "../api/context";
import type { HedyApi } from "../api/HedyApi";
import type { TeamPerson, TeamStatus } from "../api/types";
import { useToast } from "../components/ui/Toast";
import { useT, type Translate } from "../i18n";
import { SYNC_KEY } from "../sync/hooks";
import { syncErrorTitle } from "../screens/sync/SyncError";

export const TEAMS_KEY = ["teams"] as const;
export const teamStatusKey = (team: string) => [...TEAMS_KEY, "status", team] as const;
export const teamNotesKey = (team: string) => [...TEAMS_KEY, "notes", team] as const;
export const teamSuggestionsKey = (team: string) => [...TEAMS_KEY, "suggestions", team] as const;

/** What a team change may touch: the teams, memory homes, memory. */
export function refreshTeams(client: QueryClient): Promise<unknown> {
  return Promise.all(
    [TEAMS_KEY, SYNC_KEY, ["memory"], OVERVIEW_KEY].map((queryKey) => client.invalidateQueries({ queryKey })),
  );
}

export function useTeams() {
  const api = useApi();
  return useQuery({ queryKey: TEAMS_KEY, queryFn: () => api.teams() });
}

export function useTeamStatus(team: string | null) {
  const api = useApi();
  return useQuery({
    queryKey: teamStatusKey(team ?? ""),
    queryFn: () => api.teamStatus(team ?? ""),
    enabled: team !== null,
  });
}

export function useTeamSuggestions(team: string | null) {
  const api = useApi();
  return useQuery({
    queryKey: teamSuggestionsKey(team ?? ""),
    queryFn: () => api.teamSuggestions(team ?? ""),
    enabled: team !== null,
  });
}

export function useTeamNotes(team: string | null) {
  const api = useApi();
  return useQuery({
    queryKey: teamNotesKey(team ?? ""),
    queryFn: () => api.teamNotes(team ?? ""),
    enabled: team !== null,
  });
}

/** A team action that refreshes the teams and memory when it succeeds. */
export function useTeamAction<TVars, TResult = void>(action: (api: HedyApi, vars: TVars) => Promise<TResult>) {
  return useAction(action, { invalidate: [TEAMS_KEY, SYNC_KEY, ["memory"], OVERVIEW_KEY] });
}

/** "Sincronizar ahora" for one team. */
export function useTeamSync(team: string) {
  const api = useApi();
  const client = useQueryClient();
  const toast = useToast();
  const t = useT();
  return useMutation({
    mutationFn: () => api.teamSync(team),
    onSuccess: (report) => toast.info(t("sync.done", { sent: report.sent, received: report.received })),
    // Plain words ("Sin conexión"…); git's own message is in the section's "Detalles".
    onError: (error) => toast.error(t(syncErrorTitle(errorMessage(error, t)))),
    onSettled: () => refreshTeams(client),
  });
}

/** A team's name, or its address while this PC is still joining. */
export function teamLabel(team: { name: string; url: string }): string {
  return team.name.trim() || team.url;
}

export function roleLabel(role: TeamPerson["role"] | null, t: Translate): string {
  return role === "admin" ? t("team.role.admin") : role === "member" ? t("team.role.member") : "";
}

/** "Esta PC" first, then by name. */
export function isAdmin(status: TeamStatus | undefined): boolean {
  return status?.summary.role === "admin";
}

/** Tokens of a text, as the core estimates them (4 characters each). */
export function estimateTokens(text: string): number {
  return Math.ceil([...text].length / 4);
}
