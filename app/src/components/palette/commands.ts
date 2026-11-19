// What the command palette offers, built from the overview, the inbox and the
// task lists. Only explicit actions: changing a project's account is a choice
// the user makes here, never something Hedy does by itself (rule 3).

import { useQueries } from "@tanstack/react-query";

import { OVERVIEW_KEY, useAction, useApi } from "../../api/context";
import type { Overview, Project } from "../../api/types";
import type { Translate } from "../../i18n";
import { useCreate } from "../../screens/create/CreateProvider";
import { CONVERSATIONS_KEY, tasksKey } from "../../tasks/hooks";
import { useInbox } from "../../tasks/inbox";
import { STATUS_LABEL } from "../../tasks/timeline";
import { STATE_LABEL } from "../../screens/activity/ActivityScreen";
import { accountSubtitle } from "../accounts";
import { useFocusMode } from "../focusMode";
import { taskView, type View } from "../Layout";
import { headlineText, usePlanRefreshActions } from "../planUsage";
import { projectColor, useProjectActions } from "../projects";
import { useShortcuts } from "../shortcuts";
import { useToast } from "../ui/Toast";
import { modKey } from "./keys";
import type { Mode } from "./modes";

export type GroupKey = "pending" | "nav" | "code" | "projects" | "tasks" | "accounts" | "actions";

export const GROUP_ORDER: GroupKey[] = ["pending", "nav", "code", "projects", "accounts", "actions", "tasks"];

export interface Page {
  title: string;
  hint?: string;
  commands: Command[];
}

export interface Command {
  /** Stable, to remember recent ones. */
  id: string;
  group: GroupKey;
  label: string;
  /** Shown muted next to the label; also searched. */
  detail?: string;
  /** A dot of this color before the label (decorative). */
  color?: string;
  /** Its shortcut, shown on the right: the keys pressed together (`[modKey(), "P"]`). */
  keys?: string[];
  /** Runs it and closes the palette… */
  run?: () => void;
  /** …or opens a page of choices instead… */
  page?: () => Page;
  /** …or switches the palette to another mode (`files`, `line`, …). */
  switchTo?: Mode;
}

/** Most recent tasks offered per project. */
const TASKS_PER_PROJECT = 20;

export function usePaletteCommands({
  overview,
  open,
  onNavigate,
  t,
  view,
}: {
  overview: Overview;
  open: boolean;
  onNavigate: (view: View) => void;
  t: Translate;
  /** The screen on screen: a conversation offers to be carried into a project. */
  view?: View;
}): Command[] {
  const api = useApi();
  const toast = useToast();
  const actions = useProjectActions();
  const inbox = useInbox();
  const create = useCreate();
  const focusMode = useFocusMode();
  const shortcuts = useShortcuts();
  const plans = usePlanRefreshActions();
  const assign = useAction(
    (a, vars: { project: Project; account: string }) => a.useAccount(vars.project.path, vars.account),
    {
      invalidate: [OVERVIEW_KEY, ["status"]],
      onSuccess: (_result, vars) => {
        const account = overview.accounts.find((a) => a.id === vars.account);
        toast.info(t("palette.changeAccount.done", { project: vars.project.name, account: account?.name ?? vars.account }));
      },
    },
  );
  // Task lists only while the palette is open.
  const taskLists = useQueries({
    queries: overview.projects.map((p) => ({
      queryKey: tasksKey(p.path),
      queryFn: () => api.tasks(p.path),
      enabled: open,
    })),
  });
  const conversations = useQueries({
    queries: [{ queryKey: CONVERSATIONS_KEY, queryFn: () => api.conversations(), enabled: open }],
  })[0];

  const commands: Command[] = [];
  const projectName = (path: string) => overview.projects.find((p) => p.path === path)?.name ?? path;

  for (const entry of inbox.items) {
    if (entry.kind === "task") {
      const { task, state } = entry.item;
      commands.push({
        id: `pending:${entry.key}`,
        group: "pending",
        label: t("palette.pending.task", { title: task.title }),
        detail: `${entry.item.projectName} · ${t(STATE_LABEL[state])}`,
        run: () => onNavigate(taskView(overview.projects, task.project, task.id)),
      });
    } else if (entry.kind === "note") {
      commands.push({
        id: `pending:${entry.key}`,
        group: "pending",
        label: t("palette.pending.note", { text: entry.proposal.note.text }),
        detail: projectName(entry.project),
        run: () => onNavigate({ screen: "memory", project: entry.project, tab: "notes" }),
      });
    } else {
      commands.push({
        id: `pending:${entry.key}`,
        group: "pending",
        label: t("palette.pending.skill", { name: entry.proposal.skill.name }),
        detail: projectName(entry.project),
        run: () => onNavigate({ screen: "memory", project: entry.project, tab: "skills" }),
      });
    }
  }

  commands.push(
    { id: "nav:welcome", group: "nav", label: t("shell.home"), run: () => onNavigate({ screen: "welcome" }) },
    { id: "nav:activity", group: "nav", label: t("activity.title"), run: () => onNavigate({ screen: "activity" }) },
    { id: "nav:ask", group: "nav", label: t("ask.nav"), run: () => onNavigate({ screen: "ask" }) },
    {
      id: "nav:accounts",
      group: "nav",
      label: t("common.accountsAndModels"),
      run: () => onNavigate({ screen: "accounts" }),
    },
    { id: "nav:settings", group: "nav", label: t("common.settings"), run: () => onNavigate({ screen: "settings" }) },
    {
      id: "view:theme",
      group: "actions",
      label: t("theme.palette"),
      detail: t("theme.paletteHint"),
      run: () => onNavigate({ screen: "settings", section: "themes" }),
    },
    {
      id: "view:focus",
      group: "actions",
      label: focusMode.focus ? t("shell.focus.exit") : t("shell.focus.enter"),
      keys: [modKey(), "\\"],
      run: focusMode.toggle,
    },
    {
      id: "help:shortcuts",
      group: "actions",
      label: t("shell.shortcuts.title"),
      keys: ["?"],
      run: shortcuts.show,
    },
  );

  if (overview.accounts.length > 0) {
    commands.push(
      {
        id: "create:project",
        group: "actions",
        label: t("create.newProject"),
        detail: t("create.newProjectHint"),
        run: () => create.newProject(),
      },
      {
        id: "create:folder",
        group: "actions",
        label: t("create.openFolder"),
        detail: t("create.openFolderHint"),
        run: create.openFolder,
      },
      {
        id: "create:question",
        group: "actions",
        label: t("create.quickQuestion"),
        detail: t("create.quickQuestionHint"),
        run: () => create.quickQuestion(),
      },
    );
  }

  // "Llevar a un proyecto": the conversation on screen, once it answered.
  const conversation =
    view?.screen === "conversation" && view.task ? conversations?.data?.find((c) => c.id === view.task) : undefined;
  if (conversation && conversation.status !== "running" && conversation.status !== "waitingApproval") {
    commands.push({
      id: "carry:conversation",
      group: "actions",
      label: t("carry.palette"),
      detail: conversation.title,
      page: () => ({
        title: t("carry.paletteTitle"),
        hint: t("carry.paletteHint"),
        commands: [
          {
            id: "carry:new",
            group: "actions",
            label: t("carry.newProject"),
            run: () => create.newProject({ carry: conversation.id }),
          },
          ...overview.projects
            .filter((p) => p.account)
            .map((p) => ({
              id: `carry:${p.path}`,
              group: "projects" as const,
              label: p.name,
              color: projectColor(p, overview.accounts),
              run: () => onNavigate({ screen: "workspace", project: p.path, task: null, carry: conversation.id }),
            })),
        ],
      }),
    });
  }

  for (const project of overview.projects) {
    const { path, name } = project;
    const account = overview.accounts.find((a) => a.id === project.account);
    const color = projectColor(project, overview.accounts);
    commands.push({
      id: `project:${path}`,
      group: "projects",
      label: name,
      detail: account ? `${t("palette.project.detail")} · ${account.name}` : t("palette.project.detail"),
      color,
      run: () => onNavigate({ screen: "workspace", project: path }),
    });
    commands.push({
      id: `new:${path}`,
      group: "actions",
      label: t("palette.project.newTask", { name }),
      color,
      run: () => onNavigate({ screen: "workspace", project: path, task: null }),
    });
    commands.push({
      id: `terminal:${path}`,
      group: "actions",
      label: t("palette.project.terminal", { name }),
      color,
      run: () => actions.openShell(project),
    });
    if (account?.kind === "subscription") {
      commands.push({
        id: `cli:${path}`,
        group: "actions",
        label: t("palette.project.cli", { name, cli: account.providerName }),
        color,
        run: () => actions.openCli(project),
      });
    }
    commands.push({
      id: `editor:${path}`,
      group: "actions",
      label: t("palette.project.editor", { name }),
      color,
      run: () => void actions.openEditor(project),
    });
    commands.push({
      id: `memory:${path}`,
      group: "actions",
      label: t("palette.project.memory", { name }),
      color,
      run: () => onNavigate({ screen: "memory", project: path }),
    });
    commands.push({
      id: `status:${path}`,
      group: "actions",
      label: t("palette.project.status", { name }),
      color,
      run: () => onNavigate({ screen: "status", project: path }),
    });
    // A protected project keeps its account; the others offer what the protection rules allow.
    const choices = project.protected
      ? []
      : overview.accounts.filter((a) => project.offerableAccounts.includes(a.id) && a.id !== project.account);
    if (choices.length > 0) {
      commands.push({
        id: `assign:${path}`,
        group: "actions",
        label: t("palette.project.changeAccount", { name }),
        color,
        page: () => ({
          title: t("palette.changeAccount.title", { name }),
          hint: t("palette.changeAccount.hint", { account: account?.name ?? t("palette.changeAccount.none") }),
          commands: choices.map((a) => ({
            id: `assign:${path}:${a.id}`,
            group: "accounts",
            label: a.name,
            detail: [accountSubtitle(a, t), headlineText(a, t)].filter(Boolean).join(" · "),
            color: a.color,
            run: () => assign.mutate({ project, account: a.id }),
          })),
        }),
      });
    }
  }

  if (overview.accounts.some((a) => a.usage.plan.supported)) {
    commands.push({
      id: "accounts:refreshUsage",
      group: "actions",
      label: t("palette.refreshUsage"),
      run: plans.refreshAll,
    });
  }

  for (const account of overview.accounts) {
    commands.push({
      id: `account:${account.id}`,
      group: "accounts",
      label: t("palette.account.open", { name: account.name }),
      detail: headlineText(account, t) ?? accountSubtitle(account, t),
      color: account.color,
      run: () => onNavigate({ screen: "accounts", account: account.id }),
    });
  }

  for (const task of (conversations?.data ?? []).slice(0, TASKS_PER_PROJECT)) {
    commands.push({
      id: `task:${task.id}`,
      group: "tasks",
      label: task.title,
      detail: `${t("conversation.place")} · ${t(STATUS_LABEL[task.status])}`,
      run: () => onNavigate({ screen: "conversation", task: task.id }),
    });
  }

  overview.projects.forEach((project, i) => {
    for (const task of (taskLists[i]?.data ?? []).slice(0, TASKS_PER_PROJECT)) {
      commands.push({
        id: `task:${task.id}`,
        group: "tasks",
        label: task.title,
        detail: `${project.name} · ${t(STATUS_LABEL[task.status])}`,
        color: projectColor(project, overview.accounts),
        run: () => onNavigate({ screen: "workspace", project: project.path, task: task.id }),
      });
    }
  });

  return commands;
}
