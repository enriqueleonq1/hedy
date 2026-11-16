// Queries of "Llevar a un proyecto": what a conversation offers to carry and
// the tasks that continue it, plus the project option of its pickers.

import { useQuery } from "@tanstack/react-query";

import { useApi } from "../../../api/context";
import type { Account, Project, Task, TaskOrigin } from "../../../api/types";
import { projectColor, shortPath } from "../../../components/projects";
import { ColorDot, type SelectOption } from "../../../components/ui/Select";
import type { Translate } from "../../../i18n";

export const contextKey = (conversation: string) => ["conversationContext", conversation] as const;
export const continuationsKey = (conversation: string) => ["continuations", conversation] as const;

/** The conversation's questions and final answers. */
export function useConversationContext(conversation: string | null | undefined, enabled = true) {
  const api = useApi();
  return useQuery({
    queryKey: contextKey(conversation ?? ""),
    queryFn: () => api.conversationContext(conversation ?? ""),
    enabled: !!conversation && enabled,
  });
}

/** The tasks that continue a conversation, newest first. */
export function useContinuations(conversation: string | null | undefined) {
  const api = useApi();
  return useQuery({
    queryKey: continuationsKey(conversation ?? ""),
    queryFn: () => api.continuations(conversation ?? ""),
    enabled: !!conversation,
  });
}

/** A project in a picker: its name, where it is, its color. One without an account cannot take a task. */
export function projectOption(project: Project, accounts: Account[], t: Translate): SelectOption {
  return {
    value: project.path,
    label: project.name,
    detail: project.account ? shortPath(project.path) : t("common.noAccount"),
    icon: <ColorDot color={projectColor(project, accounts)} />,
    disabled: !project.account,
  };
}

/** What the first message of `task` shows: without the carried context, which `CarriedContext` shows. */
export function splitCarried(task: Task, text: string): { origin: TaskOrigin; rest: string } | null {
  const origin = task.origin;
  if (!origin || !text.startsWith(origin.context)) return null;
  return { origin, rest: text.slice(origin.context.length).trimStart() };
}
