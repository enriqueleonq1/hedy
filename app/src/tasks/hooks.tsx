import { useEffect } from "react";
import { useQuery, useQueryClient, type QueryKey } from "@tanstack/react-query";

import { OVERVIEW_KEY, useApi } from "../api/context";
import type { AgentEvent, BoardState, QueuedMessage, Task } from "../api/types";
import { codeKeys } from "../files/navigator";

export const tasksKey = (project: string) => ["tasks", project] as const;
const eventsKey = (taskId: string) => ["taskEvents", taskId] as const;
export const diffKey = (taskId: string) => ["diff", taskId] as const;
const queueKey = (taskId: string) => ["queue", taskId] as const;

/** Every template list (the composer's per project, and the management one). */
export const TEMPLATES_KEY = ["templates"] as const;

/** The task board of every project ("Actividad"). */
export const ACTIVITY_KEY = ["activity"] as const;

export function useActivity() {
  const api = useApi();
  return useQuery({ queryKey: ACTIVITY_KEY, queryFn: () => api.activity() });
}

/** A plan or permission to answer, or a limit to decide on: the user's turn. */
export function needsYou(state: BoardState): boolean {
  return state === "plan" || state === "permission" || state === "limited";
}

export function useTasks(project: string, enabled = true) {
  const api = useApi();
  return useQuery({ queryKey: tasksKey(project), queryFn: () => api.tasks(project), enabled });
}

/** Questions outside any project; under `["tasks"]`, so task refreshes reach them too. */
export const CONVERSATIONS_KEY = ["tasks", "*conversations"] as const;

export function useConversations(enabled = true) {
  const api = useApi();
  return useQuery({ queryKey: CONVERSATIONS_KEY, queryFn: () => api.conversations(), enabled });
}

/** The task list a task belongs in: its project's, or the conversations. */
export function listKeyFor(task: Pick<Task, "project" | "conversation">) {
  return task.conversation ? CONVERSATIONS_KEY : tasksKey(task.project);
}

export function useTaskEvents(taskId: string | null) {
  const api = useApi();
  return useQuery({
    queryKey: eventsKey(taskId ?? ""),
    queryFn: () => api.taskEvents(taskId ?? ""),
    enabled: !!taskId,
    staleTime: Infinity,
  });
}

/** Messages waiting for the end of the agent's turn ("En cola"), kept live by the bridge. */
export function useQueue(taskId: string | null) {
  const api = useApi();
  return useQuery({
    queryKey: queueKey(taskId ?? ""),
    queryFn: () => api.queued(taskId ?? ""),
    enabled: !!taskId,
  });
}

/** The agent is in the middle of a turn, or a card waits: a new message is queued. */
export function isBusy(task: Task | null | undefined): boolean {
  return !!task && task.live && (task.status === "running" || task.status === "waitingApproval");
}

/** Puts a queue the backend returned in the cache (the event does too; this is faster). */
export function useSetQueue() {
  const client = useQueryClient();
  return (taskId: string, queue: QueuedMessage[]) => {
    client.setQueryData(queueKey(taskId), queue);
    void client.invalidateQueries({ queryKey: ACTIVITY_KEY });
  };
}

/** What the composer of `project` offers. */
export function useTemplates(project: string | null) {
  const api = useApi();
  return useQuery({ queryKey: [...TEMPLATES_KEY, project ?? ""], queryFn: () => api.templates(project) });
}

/** Every template of every project, for Settings. */
export function useAllTemplates() {
  const api = useApi();
  return useQuery({ queryKey: [...TEMPLATES_KEY, "*all"], queryFn: () => api.allTemplates() });
}

export function useTaskDiff(taskId: string | null, enabled: boolean) {
  const api = useApi();
  return useQuery({
    queryKey: diffKey(taskId ?? ""),
    queryFn: () => api.taskDiff(taskId ?? ""),
    enabled: !!taskId && enabled,
  });
}

const CHANGES_FILES: AgentEvent["kind"][] = ["fileEdited", "finished"];
const CHANGES_ACCOUNTS: AgentEvent["kind"][] = ["usage", "rateLimit"];
/** The backend notifies `task-updated` only on a new status; these change the rest of the task. */
const CHANGES_TASK: AgentEvent["kind"][] = ["phaseStarted", "sessionStarted", "finished"];
const CHANGES_STATS: AgentEvent["kind"][] = ["usage", "testsRun"];
/** A turn that ends may have proposed notes or skills (the inbox counts them). */
const CHANGES_PROPOSALS: AgentEvent["kind"][] = ["finished"];
/** What changes a task's row on the board besides its status. */
const CHANGES_BOARD: AgentEvent["kind"][] = [
  "phaseStarted",
  "plan",
  "permissionRequest",
  "permissionResolved",
  "limitWarning",
  "rateLimit",
  "userMessage",
  "finished",
];

/** Every project's task list (task events do not say their project). */
export const ALL_TASKS_KEY = ["tasks"] as const;

/**
 * Subscribes once to live task events and keeps the query cache current:
 * events are appended to the task's history; diffs, task lists, usage and
 * project status are refetched when they change.
 */
export function TaskEventsBridge() {
  const api = useApi();
  const client = useQueryClient();

  useEffect(() => {
    const offEvent = api.onTaskEvent((taskId, event) => {
      const key = eventsKey(taskId);
      if (client.getQueryData(key)) {
        client.setQueryData<AgentEvent[]>(key, (old) => [...(old ?? []), event]);
      } else {
        void client.invalidateQueries({ queryKey: key });
      }
      const refresh = (queryKey: QueryKey) => void client.invalidateQueries({ queryKey });
      if (CHANGES_FILES.includes(event.kind)) {
        refresh(diffKey(taskId));
        // The task's files changed: its tree, open file and markers (code views).
        refresh(codeKeys.copy({ kind: "task", task: taskId }));
      }
      if (CHANGES_ACCOUNTS.includes(event.kind)) refresh(OVERVIEW_KEY);
      if (CHANGES_TASK.includes(event.kind)) refresh(ALL_TASKS_KEY);
      if (CHANGES_BOARD.includes(event.kind)) refresh(ACTIVITY_KEY);
      if (CHANGES_PROPOSALS.includes(event.kind)) {
        refresh(["memory"]);
        refresh(["skills"]);
      }
      if (CHANGES_STATS.includes(event.kind)) {
        refresh(["status"]);
        refresh(["projectUsage"]);
        refresh(["usageSummary"]);
      }
    });
    const offTask = api.onTaskUpdated((task) => {
      void client.invalidateQueries({ queryKey: listKeyFor(task) });
      void client.invalidateQueries({ queryKey: diffKey(task.id) });
      void client.invalidateQueries({ queryKey: codeKeys.copy({ kind: "task", task: task.id }) });
      void client.invalidateQueries({ queryKey: ACTIVITY_KEY });
    });
    const offQueue = api.onTaskQueue((taskId, queue) => {
      client.setQueryData(queueKey(taskId), queue);
      void client.invalidateQueries({ queryKey: ACTIVITY_KEY });
    });
    return () => {
      offEvent();
      offTask();
      offQueue();
    };
  }, [api, client]);

  return null;
}
