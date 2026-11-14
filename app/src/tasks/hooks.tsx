import { useEffect } from "react";
import { useQuery, useQueryClient, type QueryKey } from "@tanstack/react-query";

import { OVERVIEW_KEY, useApi } from "../api/context";
import type { AgentEvent } from "../api/types";

export const tasksKey = (project: string) => ["tasks", project] as const;
const eventsKey = (taskId: string) => ["taskEvents", taskId] as const;
const diffKey =(taskId: string) => ["diff", taskId] as const;

export function useTasks(project: string) {
  const api = useApi();
  return useQuery({ queryKey: tasksKey(project), queryFn: () => api.tasks(project) });
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
      if (CHANGES_FILES.includes(event.kind)) refresh(diffKey(taskId));
      if (CHANGES_ACCOUNTS.includes(event.kind)) refresh(OVERVIEW_KEY);
      if (CHANGES_TASK.includes(event.kind)) refresh(ALL_TASKS_KEY);
      if (CHANGES_STATS.includes(event.kind)) {
        refresh(["status"]);
        refresh(["projectUsage"]);
        refresh(["usageSummary"]);
      }
    });
    const offTask = api.onTaskUpdated((task) => {
      void client.invalidateQueries({ queryKey: tasksKey(task.project) });
      void client.invalidateQueries({ queryKey: diffKey(task.id) });
    });
    return () => {
      offEvent();
      offTask();
    };
  }, [api, client]);

  return null;
}
