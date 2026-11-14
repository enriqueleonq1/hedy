import { useEffect } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { OVERVIEW_KEY, useApi } from "../api/context";
import type { AgentEvent } from "../api/types";

export const tasksKey = (project: string) => ["tasks", project] as const;
export const eventsKey = (taskId: string) => ["taskEvents", taskId] as const;
export const diffKey = (taskId: string) => ["diff", taskId] as const;

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
const CHANGES_USAGE: AgentEvent["kind"][] = ["usage", "rateLimit"];

/**
 * Subscribes once to live task events and keeps the query cache current:
 * events are appended to the task's history; diffs, task lists and usage are
 * refetched when they change.
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
      if (CHANGES_FILES.includes(event.kind)) void client.invalidateQueries({ queryKey: diffKey(taskId) });
      if (CHANGES_USAGE.includes(event.kind)) void client.invalidateQueries({ queryKey: OVERVIEW_KEY });
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
