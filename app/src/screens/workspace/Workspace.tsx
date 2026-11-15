import { useEffect, useId, useMemo, useRef, useState, type ReactNode } from "react";
import { useQuery, useQueryClient, type QueryKey } from "@tanstack/react-query";

import { errorMessage, useAction, useApi } from "../../api/context";
import type { HedyApi } from "../../api/HedyApi";
import type { Account, Overview, PromptTemplate, Task, TaskMode } from "../../api/types";
import { Mascot } from "../../components/Mascot/Mascot";
import { accountOption } from "../../components/accounts";
import { shortPath } from "../../components/projects";
import { headlineText } from "../../components/planUsage";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { AskIcon, CodeIcon, PlanIcon, ResearchIcon, SendIcon, TemplateIcon } from "../../components/ui/icons";
import { MenuButton, type MenuItem } from "../../components/ui/Menu";
import { Select } from "../../components/ui/Select";
import { useToast } from "../../components/ui/Toast";
import {
  ACTIVITY_KEY,
  CONVERSATIONS_KEY,
  isBusy,
  listKeyFor,
  tasksKey,
  useConversations,
  useQueue,
  useSetQueue,
  useTaskDiff,
  useTaskEvents,
  useTasks,
  useTemplates,
} from "../../tasks/hooks";
import {
  agentName,
  buildTimeline,
  isOpen,
  latestMilestone,
  MODE_LABEL,
  pendingCard,
  STATUS_LABEL,
  type Milestone,
  type PendingCard,
  type TimelineItem,
} from "../../tasks/timeline";
import { useMascot } from "../../tasks/useMascot";
import { MODEL_HINTS, MODEL_SUGGESTIONS, modelLabel } from "../../components/models";
import { useT, type MessageKey } from "../../i18n";
import { keyLabels } from "../../platform";
import { DEFAULT_ACCOUNT_COLOR } from "../../theme/tokens";
import { fillTemplate, filterTemplates, slashQuery } from "../../templates/placeholders";
import { TemplateDialog } from "../templates/TemplateDialog";
import { ChangesPanel } from "./ChangesPanel";
import { QueuedMessages } from "./QueuedMessages";
import { PanelIcon, PlusIcon, StopIcon } from "./icons";
import { jumpDigit, TaskSwitcher } from "./TaskSwitcher";
import { Timeline } from "./Timeline";

/** A card waits for the user's answer (which one, when the events say). */
type CardWaiting = PendingCard | "other" | null;

const MILESTONE: Record<Milestone, MessageKey> = {
  plan: "workspace.announce.plan",
  permission: "workspace.announce.permission",
  finished: "workspace.announce.finished",
  failed: "workspace.announce.failed",
  limit: "workspace.announce.limit",
};

type Said = { index: number; milestone: Milestone };

/**
 * What a screen reader hears about the open task: each new milestone once
 * (a card to answer, the end, a limit), never the stream of activity, and
 * nothing for what was already there when the task opened.
 */
function useMilestone(taskId: string | null, items: TimelineItem[], ready: boolean): Said | null {
  const [seen, setSeen] = useState<{ task: string | null; baseline: number; said: Said | null }>({
    task: null,
    baseline: -1,
    said: null,
  });
  const latest = ready ? latestMilestone(items) : null;
  if (ready && taskId && seen.task !== taskId) {
    setSeen({ task: taskId, baseline: latest?.index ?? -1, said: null });
  } else if (ready && latest && seen.task === taskId && latest.index > Math.max(seen.baseline, seen.said?.index ?? -1)) {
    setSeen({ ...seen, said: latest });
  }
  return seen.task === taskId ? seen.said : null;
}

/** The modes of a question outside any project: it never changes files. */
const CONVERSATION_MODES: TaskMode[] = ["ask", "research"];

/** A project's tasks, or (no project) the conversations. */
function useWorkspaceTasks(project: string | null) {
  const projectTasks = useTasks(project ?? "", project !== null);
  const conversations = useConversations(project === null);
  return project === null ? conversations : projectTasks;
}

/**
 * The account a new conversation uses: the one the user picked, else the
 * default one, else the first allowed. Never another one by itself (rule 3).
 */
function useConversationAccount(overview: Overview, enabled: boolean) {
  const api = useApi();
  const offered = useQuery({
    queryKey: ["conversationAccounts"],
    queryFn: () => api.conversationAccounts(),
    enabled,
  });
  const [picked, setPicked] = useState<string | null>(null);
  const accounts = overview.accounts.filter((a) => offered.data?.includes(a.id));
  const chosen =
    accounts.find((a) => a.id === picked) ?? accounts.find((a) => a.id === overview.defaultAccount) ?? accounts[0];
  return { accounts, chosen, pick: setPicked, ready: offered.isSuccess };
}

export function Workspace({
  overview,
  project,
  task: chosen,
  draft,
  onSelectTask,
  onOpenMemory,
  onOpenStatus,
}: {
  overview: Overview;
  /** `null`: no project, a quick question ("conversation"). */
  project: string | null;
  /** The task to show: an id, `null` for a new one, or (unset) the most recent open one. */
  task?: string | null;
  /** Text to start the composer of a new task with. */
  draft?: string;
  /** Shows another task (`null`: a new one); `replace` keeps the step out of Back. */
  onSelectTask: (task: string | null, options?: { replace?: boolean }) => void;
  onOpenMemory?: () => void;
  onOpenStatus?: () => void;
}) {
  const t = useT();
  const solo = project === null;
  const tasks = useWorkspaceTasks(project);
  const info = solo ? undefined : overview.projects.find((p) => p.path === project);
  const asking = useConversationAccount(overview, solo);

  // By default, the most recent task that is still open.
  const list = useMemo(() => tasks.data ?? [], [tasks.data]);
  const task =
    chosen === null ? null : (list.find((t) => t.id === chosen) ?? (chosen === undefined ? list.find(isOpen) : undefined) ?? null);

  // A conversation shows its own account; a new one, the one picked for it.
  const account: Account | undefined = solo
    ? (overview.accounts.find((a) => a.id === task?.account) ?? (task ? undefined : asking.chosen))
    : overview.accounts.find((a) => a.id === info?.account);

  const events = useTaskEvents(task?.id ?? null);
  const history = useMemo(() => events.data ?? [], [events.data]);
  const items = useMemo(() => buildTimeline(history, t), [history, t]);
  const queue = useQueue(task?.id ?? null).data ?? [];
  const agent = agentName(history);
  const mascot = useMascot(task, history);
  const name = solo ? t("conversation.title") : (info?.name ?? shortPath(project));
  const milestone = useMilestone(task?.id ?? null, items, events.isSuccess);
  // A message written now waits for the card's answer (and the end of the turn).
  const card: CardWaiting = isBusy(task) && task?.status === "waitingApproval" ? (pendingCard(history) ?? "other") : null;

  // Ctrl/Cmd + 1-9 jump to the project's tasks, from anywhere in the workspace.
  const taskId = task?.id;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const digit = jumpDigit(e);
      const target = digit ? list[digit - 1] : undefined;
      if (!target) return;
      e.preventDefault();
      if (target.id !== taskId) onSelectTask(target.id);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [list, taskId, onSelectTask]);

  const title = task ? task.title : name;
  const panel = useSidePanel(solo ? null : task);
  // The one switch for the side panel (a Hedy-wide focus mode can join it here).
  const panelShown = !solo && panel.shown;
  const [deleting, setDeleting] = useState(false);
  const more: MenuItem[] = [
    ...(onOpenStatus ? [{ label: t("workspace.status"), onSelect: onOpenStatus }] : []),
    ...(onOpenMemory ? [{ label: t("common.memory"), onSelect: onOpenMemory }] : []),
    ...(solo && task && !isBusy(task) ? [{ label: t("conversation.deleteLabel"), onSelect: () => setDeleting(true) }] : []),
  ];

  return (
    <div className={["workspace", solo && "workspace--solo", !panelShown && "workspace--no-panel"].filter(Boolean).join(" ")}>
      <section className="chat" aria-label={t("workspace.chat")}>
        <header className="chat__head">
          <div className="chat__title">
            <h1 title={title}>{title}</h1>
            <p className="muted">
              {solo ? t("conversation.subtitle") : name}
              {task?.workspace && <> · {t("common.branch", { branch: task.workspace.branch })}</>}
              {task && <> · {t(MODE_LABEL[task.mode])} · {t(STATUS_LABEL[task.status])}</>}
            </p>
          </div>
          <div className="chat__tools">
            {list.length > 0 && (
              <TaskSwitcher
                tasks={list}
                current={task?.id ?? null}
                accounts={overview.accounts}
                onSelect={onSelectTask}
                conversations={solo}
              />
            )}
            {task && (
              <button type="button" className="tool-btn" onClick={() => onSelectTask(null)}>
                <PlusIcon />
                {solo ? t("conversation.newLabel") : t("workspace.newTask")}
              </button>
            )}
            {!solo && (
              <button
                type="button"
                className="tool-btn tool-btn--icon"
                aria-pressed={panel.shown}
                title={t("workspace.panelToggle")}
                onClick={panel.toggle}
              >
                <PanelIcon />
                <span className="sr-only">
                  {t("workspace.panelToggle")}
                  {panel.waiting && `, ${t("workspace.panelHasChanges")}`}
                </span>
                {panel.waiting && <span className="tool-btn__dot" aria-hidden />}
              </button>
            )}
            {more.length > 0 && <MenuButton label={t("workspace.more")} items={more} />}
          </div>
        </header>
        {deleting && task && (
          <DeleteConversation task={task} onClose={() => setDeleting(false)} onDeleted={() => onSelectTask(null)} />
        )}

        {tasks.isError && (
          <p className="error-text" role="alert">
            {errorMessage(tasks.error, t)}
          </p>
        )}
        {task ? (
          <Timeline
            task={task}
            items={items}
            agent={agent}
            account={account}
            accounts={overview.accounts}
            mascot={mascot}
            scrollKey={queue.length}
          />
        ) : (
          <div className="chat__empty">
            <Mascot color={account?.color ?? DEFAULT_ACCOUNT_COLOR} state="idle" size={120} label="Hedy" />
            <p>
              {solo
                ? asking.ready && !account
                  ? t("conversation.noAccounts")
                  : t("conversation.empty")
                : account
                  ? t("workspace.empty", { name, account: account.name })
                  : t("workspace.noAccount")}
            </p>
          </div>
        )}

        {/* The only live region of the chat: milestones, not the stream. */}
        <p className="sr-only chat__announce" role="status">
          {milestone && <span key={milestone.index}>{t(MILESTONE[milestone.milestone])}</span>}
        </p>

        {task && <QueuedMessages task={task} queue={queue} agent={agent} cardWaiting={task.status === "waitingApproval"} />}

        <Composer
          key={task?.id ?? "new"}
          scope={project}
          name={name}
          task={task}
          card={card}
          models={account ? MODEL_SUGGESTIONS[account.provider] : []}
          agent={agent}
          disabled={!account}
          initialText={task ? "" : draft}
          autoFocus={chosen === null || (solo && !task)}
          start={
            solo
              ? (api, input) =>
                  api.startConversation({
                    prompt: input.prompt,
                    mode: input.mode,
                    account: account?.id ?? null,
                    model: input.model,
                  })
              : (api, input) => api.startTask({ project, ...input })
          }
          accountPicker={
            solo && !task && asking.accounts.length > 1 && account ? (
              <Select
                variant="chip"
                label={t("conversation.account")}
                title={t("common.account")}
                value={account.id}
                onChange={asking.pick}
                options={asking.accounts.map((a) =>
                  accountOption(a, [a.providerName, headlineText(a, t)].filter(Boolean).join(" · ")),
                )}
                footnote={t("workspace.composer.accountFootnote")}
              />
            ) : null
          }
          onStarted={(created) => onSelectTask(created.id, { replace: true })}
        />
      </section>
      {/* Kept mounted while hidden: a terminal opened there keeps running. */}
      {!solo && <ChangesPanel project={project} task={task} hidden={!panelShown} />}
    </div>
  );
}

const PANEL_KEY = "hedy.sidePanel";

/**
 * Whether the side panel (changes, terminal, preview) shows: by itself only
 * while there is something to see (changes to review, a merge to undo); the
 * user's own choice from the toggle wins for the rest of the session.
 */
function useSidePanel(task: Task | null) {
  const editable = !!task?.workspace && isOpen(task);
  const diff = useTaskDiff(task?.id ?? null, editable);
  const hasContent = (editable && (diff.data?.length ?? 0) > 0) || (task?.status === "accepted" && !!task.merge);
  const [chosen, setChosen] = useState<boolean | null>(() => {
    try {
      const saved = sessionStorage.getItem(PANEL_KEY);
      return saved === null ? null : saved === "shown";
    } catch {
      return null;
    }
  });
  const shown = chosen ?? hasContent;
  return {
    shown,
    /** Hidden by the user while there are changes to look at. */
    waiting: !shown && hasContent,
    toggle: () => {
      setChosen(!shown);
      try {
        sessionStorage.setItem(PANEL_KEY, shown ? "hidden" : "shown");
      } catch {
        // Remembering the choice is only a convenience.
      }
    },
  };
}

/** Deletes a conversation (history and folder), once confirmed. */
function DeleteConversation({ task, onClose, onDeleted }: { task: Task; onClose: () => void; onDeleted: () => void }) {
  const t = useT();
  const toast = useToast();
  const remove = useAction((api) => api.deleteConversation(task.id), {
    invalidate: [CONVERSATIONS_KEY, ACTIVITY_KEY, ["usageSummary"]],
    onSuccess: () => {
      toast.info(t("conversation.deleted"));
      onClose();
      onDeleted();
    },
  });
  return (
    <Dialog
      title={t("conversation.deleteTitle")}
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button variant="danger" disabled={remove.isPending} onClick={() => remove.mutate(undefined)}>
            {t("conversation.delete")}
          </Button>
        </>
      }
    >
      <p>{t("conversation.deleteBody")}</p>
    </Dialog>
  );
}

const MODES = Object.keys(MODE_LABEL) as TaskMode[];

const MODE_ICON: Record<TaskMode, ReactNode> = {
  project: <CodeIcon />,
  ask: <AskIcon />,
  research: <ResearchIcon />,
};

const MODE_DETAIL: Record<TaskMode, MessageKey> = {
  project: "workspace.composer.modeProjectDetail",
  ask: "workspace.composer.modeAskDetail",
  research: "workspace.composer.modeResearchDetail",
};

/** What starts a new task from the composer: in a project, or as a conversation. */
type StartTask = (
  api: HedyApi,
  input: { prompt: string; mode: TaskMode; model: string | null; planFirst: boolean },
) => Promise<Task>;

const CARD_LABEL: Record<NonNullable<CardWaiting>, MessageKey> = {
  plan: "workspace.composer.cardPlan",
  permission: "workspace.composer.cardPermission",
  other: "workspace.composer.cardOther",
};

/**
 * The mode the composer starts new tasks in, remembered per project (and for
 * conversations); a convenience: storage may fail.
 */
function useProjectMode(project: string | null, modes: TaskMode[]): [TaskMode, (mode: TaskMode) => void] {
  const key = `hedy.mode.${project ?? "*conversation"}`;
  const fallback = modes[0] ?? "project";
  const [mode, setMode] = useState<TaskMode>(() => {
    try {
      const saved = localStorage.getItem(key);
      return modes.find((m) => m === saved) ?? fallback;
    } catch {
      return fallback;
    }
  });
  const choose = (next: TaskMode) => {
    setMode(next);
    try {
      localStorage.setItem(key, next);
    } catch {
      // Remembering the mode is only a convenience.
    }
  };
  return [mode, choose];
}

function Composer({
  scope,
  name,
  task,
  card,
  models,
  agent,
  disabled,
  initialText,
  autoFocus,
  start: startNew,
  accountPicker,
  onStarted,
}: {
  /** The project, or `null` for a conversation. */
  scope: string | null;
  name: string;
  task: Task | null;
  /** A plan or permission card waits: what is written now goes after its answer. */
  card: CardWaiting;
  models: string[];
  /** Who works on the task now, for the queue's label. */
  agent: string;
  disabled: boolean;
  initialText?: string;
  /** Takes the focus when it appears (a new task or question). */
  autoFocus?: boolean;
  start: StartTask;
  /** Shown in the bar of a new conversation. */
  accountPicker?: ReactNode;
  onStarted: (task: Task) => void;
}) {
  const t = useT();
  const client = useQueryClient();
  const setQueue = useSetQueue();
  const input = useRef<HTMLTextAreaElement>(null);
  const listId = useId();
  const solo = scope === null;
  const listKey: QueryKey = solo ? CONVERSATIONS_KEY : tasksKey(scope);
  const [text, setText] = useState(initialText ?? "");
  const [model, setModel] = useState("");
  const [planFirst, setPlanFirst] = useState(true);
  const [active, setActive] = useState(0);
  const [pickerClosed, setPickerClosed] = useState(false);
  const [filling, setFilling] = useState<PromptTemplate | null>(null);
  const [saving, setSaving] = useState(false);
  const followUp = isOpen(task);
  // While the agent works, a follow-up waits in the queue for the end of its turn.
  const queueing = followUp && isBusy(task);
  const [newMode, setNewMode] = useProjectMode(scope, solo ? CONVERSATION_MODES : MODES);
  // A follow-up stays in its task's mode; the selector is only for new tasks.
  const mode = followUp && task ? task.mode : newMode;
  const keys = keyLabels();
  const templates = useTemplates(scope);

  // After the dialogs that led here closed (they give the focus back first).
  useEffect(() => {
    if (!autoFocus || disabled) return;
    const frame = requestAnimationFrame(() => {
      const el = input.current;
      if (!el) return;
      el.focus();
      el.setSelectionRange(el.value.length, el.value.length);
    });
    return () => cancelAnimationFrame(frame);
  }, [autoFocus, disabled]);

  const query = slashQuery(text);
  const matches = query === null ? [] : filterTemplates(templates.data ?? [], query);
  const pickerOpen = query !== null && !pickerClosed && !filling;
  const current = pickerOpen ? matches[Math.min(active, matches.length - 1)] : undefined;
  const optionId = (index: number) => `${listId}-option-${index}`;

  const start = useAction((api, prompt: string) => startNew(api, { prompt, mode, model: model || null, planFirst }), {
    onSuccess: (created) => {
      setText("");
      client.setQueryData<Task[]>(listKey, (old) => [created, ...(old ?? [])]);
      void client.invalidateQueries({ queryKey: ACTIVITY_KEY });
      onStarted(created);
    },
  });
  const send = useAction((api, message: string) => api.queueMessage(task?.id ?? "", message), {
    invalidate: [listKey],
    onSuccess: (left) => {
      setText("");
      if (task) setQueue(task.id, left);
    },
  });
  const stop = useAction((api, id: string) => api.stopTask(id), { invalidate: task ? [listKeyFor(task)] : [] });
  const busy = start.isPending || send.isPending;
  // While the agent runs, an empty composer offers to stop it (Esc does too).
  const stoppable = !!task?.live && !text.trim();

  function stopAgent() {
    if (task?.live && !stop.isPending) stop.mutate(task.id);
  }

  function edit(value: string) {
    setText(value);
    setActive(0);
    if (slashQuery(value) === null) setPickerClosed(false);
  }

  function focusInput() {
    requestAnimationFrame(() => {
      const el = input.current;
      if (!el) return;
      el.focus();
      el.setSelectionRange(el.value.length, el.value.length);
    });
  }

  function choose(template: PromptTemplate) {
    if (template.placeholders.length > 0) {
      setFilling(template);
      return;
    }
    setText(template.text);
    focusInput();
  }

  const sendLabel =
    followUp && card ? t("workspace.composer.sendAfterCard") : queueing ? t("queue.add") : t("workspace.composer.send");

  function submit() {
    const value = text.trim();
    if (!value || busy || disabled) return;
    if (followUp) send.mutate(value);
    else start.mutate(value);
  }

  return (
    <form
      className="composer"
      onSubmit={(e) => {
        e.preventDefault();
        if (current) choose(current);
        else submit();
      }}
    >
      {pickerOpen && (
        <div className="template-picker">
          {matches.length > 0 ? (
            <ul className="template-picker__list" role="listbox" id={listId} aria-label={t("templates.picker")}>
              {matches.map((template, index) => (
                <li
                  key={template.id}
                  id={optionId(index)}
                  role="option"
                  aria-selected={template === current}
                  className={`template-option${template === current ? " template-option--active" : ""}`}
                  onMouseDown={(e) => {
                    e.preventDefault();
                    choose(template);
                  }}
                  onMouseEnter={() => setActive(index)}
                >
                  <span className="template-option__name">{template.name}</span>
                  <span className="template-option__text">{template.text}</span>
                </li>
              ))}
            </ul>
          ) : (
            <p className="template-picker__empty" role="status">
              {t("templates.noMatch")}
            </p>
          )}
          <p className="template-picker__hint" aria-hidden>
            {t("templates.pickerHint")}
          </p>
        </div>
      )}
      {filling && (
        <TemplateFill
          template={filling}
          onCancel={() => {
            setFilling(null);
            focusInput();
          }}
          onDone={(filled) => {
            setFilling(null);
            setText(filled);
            focusInput();
          }}
        />
      )}
      {/* The placeholder says what sending does now; the label says it in full. */}
      <label className="sr-only" htmlFor="composer-input">
        {followUp && card
          ? t(CARD_LABEL[card])
          : queueing
            ? t("queue.composerBusy", { agent })
            : followUp
              ? t("workspace.composer.followUp")
              : solo
                ? t("conversation.ask")
                : t("workspace.composer.ask", { name })}
      </label>
      <textarea
        ref={input}
        id="composer-input"
        className="composer__input"
        rows={2}
        value={text}
        disabled={disabled}
        aria-autocomplete="list"
        aria-haspopup="listbox"
        aria-controls={pickerOpen && matches.length > 0 ? listId : undefined}
        aria-activedescendant={current ? optionId(matches.indexOf(current)) : undefined}
        aria-keyshortcuts="Enter"
        aria-describedby={`${listId}-enter`}
        placeholder={
          followUp && card
            ? t("workspace.composer.placeholderCard")
            : queueing
              ? t("workspace.composer.placeholderQueue")
              : followUp
                ? t("workspace.composer.placeholderFollowUp")
                : solo
                  ? t("conversation.placeholder")
                  : mode === "project"
                    ? t("workspace.composer.placeholderProject")
                    : t("workspace.composer.placeholderAsk")
        }
        onChange={(e) => edit(e.target.value)}
        onKeyDown={(e) => {
          if (pickerOpen && matches.length > 0) {
            if (e.key === "ArrowDown" || e.key === "ArrowUp") {
              e.preventDefault();
              const step = e.key === "ArrowDown" ? 1 : -1;
              setActive((i) => (Math.min(i, matches.length - 1) + step + matches.length) % matches.length);
              return;
            }
            if ((e.key === "Enter" || e.key === "Tab") && !e.shiftKey && current) {
              e.preventDefault();
              choose(current);
              return;
            }
          }
          if (pickerOpen && e.key === "Escape") {
            e.preventDefault();
            setPickerClosed(true);
            return;
          }
          if (e.nativeEvent.isComposing) return;
          if (e.key === "Escape" && task?.live) {
            e.preventDefault();
            stopAgent();
            return;
          }
          // Enter sends (Ctrl/⌘ + Enter too); Shift + Enter is a new line.
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            submit();
          }
        }}
      />
      <p id={`${listId}-enter`} className="sr-only">
        {task?.live ? `${t("conversation.enterHint")} · ${t("workspace.composer.escStops")}` : t("conversation.enterHint")}
      </p>
      <div className="composer__bar">
        <div className="composer__chips">
          {!followUp && (
            <>
              <Select<TaskMode>
                variant="chip"
                label={t("workspace.composer.mode")}
                title={t("workspace.composer.mode")}
                value={mode}
                options={(solo ? CONVERSATION_MODES : MODES).map((value) => ({
                  value,
                  label: t(MODE_LABEL[value]),
                  detail: t(MODE_DETAIL[value]),
                  icon: MODE_ICON[value],
                }))}
                onChange={setNewMode}
              />
              {accountPicker}
              {models.length > 0 && (
                <Select
                  variant="chip"
                  label={t("workspace.composer.model")}
                  title={t("workspace.composer.model")}
                  value={model}
                  options={[
                    {
                      value: "",
                      label: solo ? t("conversation.defaultModel") : t("workspace.composer.modelAuto"),
                      detail: solo ? t("workspace.composer.modelAccountDetail") : t("workspace.composer.modelAutoDetail"),
                    },
                    ...models.map((m) => {
                      const hint = MODEL_HINTS[m];
                      return { value: m, label: modelLabel(m), detail: hint ? t(hint) : undefined };
                    }),
                  ]}
                  onChange={setModel}
                />
              )}
              {mode === "project" && (
                <button type="button" className="chip" aria-pressed={planFirst} onClick={() => setPlanFirst((on) => !on)}>
                  <PlanIcon />
                  {t("workspace.composer.planFirst")}
                </button>
              )}
            </>
          )}
          <MenuButton
            label={t("templates.picker")}
            title={t("templates.picker")}
            className="chip chip--icon"
            align="start"
            items={[
              {
                label: t("templates.use"),
                shortcut: "/",
                disabled: text.trim() !== "" || disabled,
                onSelect: () => {
                  edit("/");
                  focusInput();
                },
              },
              {
                label: t("templates.saveCurrent"),
                disabled: !text.trim() || query !== null,
                onSelect: () => setSaving(true),
              },
            ]}
          >
            <TemplateIcon />
          </MenuButton>
        </div>
        <span className="composer__hint" aria-hidden>
          {stoppable
            ? t("workspace.composer.hintStop")
            : t("workspace.composer.hintEnter", { enter: keys.enter, newline: keys.newline })}
        </span>
        {stoppable ? (
          <button
            type="button"
            className="composer__send composer__send--stop"
            aria-label={t("workspace.stop")}
            title={t("workspace.stop")}
            aria-keyshortcuts="Escape"
            disabled={stop.isPending}
            onClick={stopAgent}
          >
            <StopIcon />
          </button>
        ) : (
          <button
            type="submit"
            className="composer__send"
            aria-label={sendLabel}
            title={sendLabel}
            aria-keyshortcuts="Enter Control+Enter Meta+Enter"
            disabled={!text.trim() || busy || disabled}
          >
            <SendIcon />
          </button>
        )}
      </div>
      {saving && (
        <TemplateDialog
          title={t("templates.saveCurrentTitle")}
          initialText={text.trim()}
          projects={scope === null ? [] : [{ path: scope, name }]}
          defaultProject={scope}
          onClose={() => setSaving(false)}
        />
      )}
    </form>
  );
}

/** Asks for each placeholder of a template, in turn, before it goes in the composer. */
function TemplateFill({
  template,
  onCancel,
  onDone,
}: {
  template: PromptTemplate;
  onCancel: () => void;
  onDone: (text: string) => void;
}) {
  const t = useT();
  const baseId = useId();
  const [values, setValues] = useState<Record<string, string>>({});
  const fields = useRef<(HTMLInputElement | null)[]>([]);
  const done = () => onDone(fillTemplate(template.text, values));

  return (
    <div
      className="template-fill"
      role="group"
      aria-labelledby={`${baseId}-title`}
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.preventDefault();
          e.stopPropagation();
          onCancel();
        }
      }}
    >
      <p className="template-fill__title" id={`${baseId}-title`}>
        {t("templates.fill", { name: template.name })}
      </p>
      <div className="template-fill__fields">
        {template.placeholders.map((placeholder, index) => (
          <label key={placeholder} className="field" htmlFor={`${baseId}-${index}`}>
            <span className="field__label mono">{`{${placeholder}}`}</span>
            <input
              id={`${baseId}-${index}`}
              ref={(el) => {
                fields.current[index] = el;
              }}
              autoFocus={index === 0}
              value={values[placeholder] ?? ""}
              onChange={(e) => setValues((v) => ({ ...v, [placeholder]: e.target.value }))}
              onKeyDown={(e) => {
                if (e.key !== "Enter") return;
                e.preventDefault();
                const next = fields.current[index + 1];
                if (next) next.focus();
                else done();
              }}
            />
          </label>
        ))}
      </div>
      <div className="actions">
        <Button small variant="primary" onClick={done}>
          {t("templates.insert")}
        </Button>
        <Button small variant="ghost" onClick={onCancel}>
          {t("common.cancel")}
        </Button>
      </div>
    </div>
  );
}
