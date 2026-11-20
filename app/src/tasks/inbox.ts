// "Te necesitan": everything waiting for the user across projects — plans,
// permissions and limits of the tasks, the memory notes and skills the AIs
// proposed (rule 6: only the user approves them), sync conflicts and the
// suggestions a team admin may take. One inbox: Inicio, its badge and the
// palette all read it.

import { useQueries } from "@tanstack/react-query";

import { useApi, useOverview } from "../api/context";
import type { ActivityItem, AgentEvent, Proposal, SkillProposal } from "../api/types";
import { memoryKey } from "../screens/memory/notes";
import { skillsKey } from "../screens/memory/skills";
import { useSyncStatus } from "../sync/hooks";
import { teamSuggestionsKey, useTeams } from "../team/hooks";
import { needsYou, useActivity } from "./hooks";

export type InboxItem =
  | { kind: "task"; key: string; item: ActivityItem }
  /** `project`: where to open it (for a global proposal, the first project that shows it). */
  | { kind: "note"; key: string; project: string; proposal: Proposal }
  | { kind: "skill"; key: string; project: string; proposal: SkillProposal }
  /** Items changed on two computers: which version stays is the user's call. */
  | { kind: "sync"; key: string; conflicts: number }
  /** Suggestions to a team's memory this person may take (an admin). */
  | { kind: "team"; key: string; team: string; teamName: string; count: number };

/**
 * Every pending item: the tasks in board order, then the proposals, newest
 * first. A global proposal shows up in every project's memory; it counts once.
 */
export function useInbox(): { items: InboxItem[]; count: number; ready: boolean } {
  const api = useApi();
  const projects = (useOverview().data?.projects ?? []).map((p) => p.path);
  const board = useActivity();
  const memories = useQueries({
    queries: projects.map((project) => ({ queryKey: memoryKey(project), queryFn: () => api.memory(project) })),
  });
  const skills = useQueries({
    queries: projects.map((project) => ({ queryKey: skillsKey(project), queryFn: () => api.skills(project) })),
  });
  const sync = useSyncStatus().data;
  const teams = (useTeams().data ?? []).filter((team) => !team.waiting);
  const suggestions = useQueries({
    queries: teams.map((team) => ({
      queryKey: teamSuggestionsKey(team.id),
      queryFn: () => api.teamSuggestions(team.id),
    })),
  });

  const tasks: InboxItem[] = (board.data ?? [])
    .filter((item) => needsYou(item.state))
    .map((item) => ({ kind: "task", key: `task:${item.task.id}`, item }));

  const proposals: { at: number; entry: InboxItem }[] = [];
  const seen = new Set<string>();
  projects.forEach((project, i) => {
    for (const proposal of memories[i]?.data?.proposed ?? []) {
      const key = `note:${proposal.note.id}`;
      if (seen.has(key)) continue;
      seen.add(key);
      proposals.push({ at: proposal.note.createdAt, entry: { kind: "note", key, project, proposal } });
    }
    for (const proposal of skills[i]?.data?.proposed ?? []) {
      const key = `skill:${proposal.skill.id}`;
      if (seen.has(key)) continue;
      seen.add(key);
      proposals.push({ at: proposal.skill.createdAt, entry: { kind: "skill", key, project, proposal } });
    }
  });
  proposals.sort((a, b) => b.at - a.at);

  const shared: InboxItem[] = [];
  if (sync && sync.conflicts > 0) shared.push({ kind: "sync", key: "sync", conflicts: sync.conflicts });
  teams.forEach((team, i) => {
    const count = (suggestions[i]?.data ?? []).filter((s) => s.canReview).length;
    if (count > 0) shared.push({ kind: "team", key: `team:${team.id}`, team: team.id, teamName: team.name, count });
  });

  const items = [...tasks, ...shared, ...proposals.map((p) => p.entry)];
  return { items, count: items.length, ready: board.isSuccess };
}

/** The permission request still waiting for an answer, if any (the latest one). */
export function openPermission(
  events: AgentEvent[],
): Extract<AgentEvent, { kind: "permissionRequest" }> | null {
  const open = new Map<string, Extract<AgentEvent, { kind: "permissionRequest" }>>();
  for (const e of events) {
    if (e.kind === "permissionRequest") open.set(e.requestId, e);
    if (e.kind === "permissionResolved") open.delete(e.requestId);
  }
  return [...open.values()].at(-1) ?? null;
}
