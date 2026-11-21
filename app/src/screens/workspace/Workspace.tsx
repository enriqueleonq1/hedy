import { useEffect, useId, useMemo, useRef, useState, type MouseEvent, type ReactNode } from "react";
import { useQuery, useQueryClient, type QueryKey } from "@tanstack/react-query";

import { errorMessage, useAction, useApi, useOverview } from "../../api/context";
import type { HedyApi } from "../../api/HedyApi";
import type {
  Account,
  AgentCommand,
  CommandSource,
  ImageInput,
  Overview,
  PermissionMode,
  PromptTemplate,
  Task,
  TaskMode,
} from "../../api/types";
import { Mascot } from "../../components/Mascot/Mascot";
import { accountOption } from "../../components/accounts";
import { useFocusMode } from "../../components/focusMode";
import type { View } from "../../components/Layout";
import { shortPath } from "../../components/projects";
import { headlineText } from "../../components/planUsage";
import { Button } from "../../components/ui/Button";
import { StatusPill } from "../../components/ui/StatusPill";
import { pressPanelShortcut } from "../../components/palette/codeCommands";
import { taskState } from "../../tasks/state";
import { Dialog } from "../../components/ui/Dialog";
import { EmptyState } from "../../components/ui/EmptyState";
import { AskIcon, CodeIcon, InfoIcon, PlanIcon, ResearchIcon, SendIcon, TemplateIcon } from "../../components/ui/icons";
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
  useFollowElsewhere,
  useQueue,
  useSetQueue,
  useTaskDiff,
  useTaskEvents,
  useTasks,
  useTemplates,
  useAccountCommands,
  useActivity,
} from "../../tasks/hooks";
import {
  agentName,
  buildTimeline,
  isOpen,
  latestMilestone,
  MODE_LABEL,
  pendingCard,
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
import { commandName, commandText, commandWithArguments, filterCommands } from "../../templates/commands";
import { TemplateDialog } from "../templates/TemplateDialog";
import "../../styles/composer.css";
import { ChangesPanel } from "./ChangesPanel";
import { useReviewTask } from "../../review/comments";
import { AttachButton, draggingFiles, ImageTray, transferFiles, useComposerImages } from "./ComposerImages";
import { insertAt, useComposerInsert } from "./composerBridge";
import { FilePicker } from "./FilePicker";
import { insertMention, mentionAt, rankFiles } from "./mentions";
import { Suggestions, useSuggestions, type Suggestion } from "./Suggestions";
import { QueuedMessages } from "./QueuedMessages";
import { CarryIcon, PanelIcon, PlusIcon, StopIcon } from "./icons";
import { CarryDialog } from "./carry/CarryDialog";
import { ContextCard, useCarryDraft } from "./carry/ContextCard";
import { CarrySuggestion, ContinuedIn } from "./carry/links";
import { NoteDialog } from "./carry/NoteDialog";
import { continuationsKey, contextKey, useContinuations } from "./carry/queries";
import { jumpDigit, TaskSwitcher } from "./TaskSwitcher";
import { Timeline } from "./Timeline";
import { ClosingBar } from "./ClosingBar";
import { EndCard } from "./EndCard";
import { OutputsCard } from "./OutputsCard";
import { ProjectHead, ProjectTasks, projectTabView } from "./ProjectHome";
import { TaskDetails } from "./TaskDetails";

/** A card waits for the user's answer (which one, when the events say). */
type CardWaiting = PendingCard | "other" | null;

const MILESTONE: Record<Milestone, MessageKey> = {
  plan: "workspace.announce.plan",
  permission: "workspace.announce.permission",
  approval: "workspace.announce.approval",
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
  carry,
  onNavigate,
  onSelectTask,
  onOpenMemory,
  onOpenStatus,
}: {
  overview: Overview;
  /** `null`: no project, a quick question ("conversation"). */
  project: string | null;
  /** The task to show: an id; `null` or unset, a new one (a project's home). */
  task?: string | null;
  /** Text to start the composer of a new task with. */
  draft?: string;
  /** A new task continues this conversation: its context goes in a card above the composer. */
  carry?: string;
  /** Goes to another screen (the links between a conversation and its tasks). */
  onNavigate?: (view: View) => void;
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

  // A project opens on its home (a new task); a task, only when asked for.
  const list = useMemo(() => tasks.data ?? [], [tasks.data]);
  const task = chosen ? (list.find((t) => t.id === chosen) ?? null) : null;

  // A conversation shows its own account; a new one, the one picked for it.
  const account: Account | undefined = solo
    ? (overview.accounts.find((a) => a.id === task?.account) ?? (task ? undefined : asking.chosen))
    : overview.accounts.find((a) => a.id === info?.account);

  useReviewTask(task);

  const events = useTaskEvents(task?.id ?? null);
  // A conversation answering in `hedy ask`: its history comes from the store.
  useFollowElsewhere(task);
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
      // Not behind a dialog (Detalles, a sheet): the task under it would change.
      if (document.querySelector('[aria-modal="true"]')) return;
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
  const suggestions = useSuggestions(project, account?.provider);
  // A suggestion fills the composer of a new task; `n` tells the same one apart.
  const [fill, setFill] = useState<(Suggestion & { n: number; scope: string | null }) | null>(null);
  const panel = useSidePanel(solo ? null : task);
  // The one switch for the side panel: focus mode puts it away too.
  const focusMode = useFocusMode();
  const panelShown = !solo && panel.shown && !focusMode.focus;
  const [deleting, setDeleting] = useState(false);
  // "Llevar a un proyecto" and "Guardar como nota" wait for the answer to end.
  const [carrying, setCarrying] = useState(false);
  const [noting, setNoting] = useState(false);
  const toast = useToast();
  const client = useQueryClient();
  const [details, setDetails] = useState(false);
  const boardItem = useActivity().data?.find((item) => item.task.id === task?.id);
  const state = task ? taskState(task, boardItem, history) : null;
  const openPanel = () => {
    if (focusMode.focus) focusMode.toggle();
    if (!panel.shown) panel.open();
  };
  // Shows or hides the side panel (changes, files, terminal, preview), on a task or the project's home.
  const panelToggle = (
      <button
        type="button"
        className="tool-btn tool-btn--icon"
        aria-pressed={panelShown}
        title={t("workspace.panelToggle")}
        onClick={() => (focusMode.focus && panel.shown ? focusMode.toggle() : panel.toggle())}
      >
        <PanelIcon />
        <span className="sr-only">
          {t("workspace.panelToggle")}
          {panel.waiting && `, ${t("workspace.panelHasChanges")}`}
        </span>
        {panel.waiting && <span className="tool-btn__dot" aria-hidden />}
      </button>
  );
  /** A new task in this project, its composer started with `prompt` when given. */
  const newTask = (prompt?: string) => {
    if (prompt && task) {
      setFill({ label: "", text: prompt, caret: prompt.length, mode: task.mode, n: (fill?.n ?? 0) + 1, scope: project });
    }
    onSelectTask(null);
  };
  const answering = !!task && (task.status === "running" || task.status === "waitingApproval");
  const carryable = solo && !!task && !!onNavigate;
  const continued = useContinuations(carryable ? task?.id : null);
  const startCarry = () => (answering ? toast.info(t("carry.busy")) : setCarrying(true));
  const startNote = () => (answering ? toast.info(t("carry.busy")) : setNoting(true));
  const more: MenuItem[] = [
    ...(onOpenStatus ? [{ label: t("workspace.status"), onSelect: onOpenStatus }] : []),
    ...(onOpenMemory ? [{ label: t("common.memory"), onSelect: onOpenMemory }] : []),
    ...(carryable
      ? [
          {
            label: t("carry.actionMenu"),
            hint: answering ? t("carry.busy") : t("carry.actionHint"),
            disabled: answering,
            onSelect: () => setCarrying(true),
          },
          {
            label: t("carry.saveNoteMenu"),
            hint: answering ? t("carry.busy") : t("carry.saveNoteHint"),
            disabled: answering,
            onSelect: () => setNoting(true),
          },
        ]
      : []),
    ...(solo && task && !isBusy(task)
      ? [{ label: t("conversation.deleteLabel"), onSelect: () => setDeleting(true), separatorBefore: carryable }]
      : []),
  ];
  // A new project task that continues a conversation.
  const carried = useCarryDraft(!solo && !task ? carry : undefined);
  const carriedFrom = useConversations(!!carried).data?.find((c) => c.id === carried?.conversation);

  return (
    <div className={["workspace", solo && "workspace--solo", !panelShown && "workspace--no-panel"].filter(Boolean).join(" ")}>
      <section className="chat" aria-label={t("workspace.chat")}>
        {!solo && !task && project ? (
          <ProjectHead
            name={name}
            path={project}
            current="tasks"
            tools={panelToggle}
            onTab={(tab) =>
              tab === "memory" ? onOpenMemory?.() : tab === "summary" ? onOpenStatus?.() : onNavigate?.(projectTabView(tab, project))
            }
          />
        ) : (
        <header className="chat__head">
          <div className="chat__title">
            {!solo && (
              <button type="button" className="chat__crumb" onClick={() => onSelectTask(null)} aria-label={t("project.home", { name })}>
                <span aria-hidden>{name} /</span>
              </button>
            )}
            <div className="chat__title-row">
              <h1 title={title}>{title}</h1>
              {task && state && <StatusPill state={state} task={task} />}
              {task && task.mode === "project" && isOpen(task) && task.permissions && task.permissions !== "manual" && (
                <span className="mode-tag">{t(PERMISSION_LABEL[task.permissions])}</span>
              )}
            </div>
            {solo && (
              <p className="muted">
                {/* Where it came from or went goes first: the line is cut at its end. */}
                {carryable && task && (continued.data?.length ?? 0) > 0 && onNavigate ? (
                  <ContinuedIn conversation={task.id} overview={overview} onNavigate={onNavigate} />
                ) : (
                  t("conversation.subtitle")
                )}
              </p>
            )}
          </div>
          <div className="chat__tools">
            {task && !solo && (
              <button type="button" className="tool-btn" onClick={() => setDetails(true)}>
                <InfoIcon />
                <span className="tool-btn__label">{t("details.open")}</span>
              </button>
            )}
            {list.length > 0 && (
              <TaskSwitcher
                tasks={list}
                current={task?.id ?? null}
                accounts={overview.accounts}
                onSelect={onSelectTask}
                conversations={solo}
              />
            )}
            {carryable && (
              <button
                type="button"
                className="tool-btn tool-btn--icon"
                aria-disabled={answering || undefined}
                title={`${t("carry.action")} · ${answering ? t("carry.busy") : t("carry.actionHint")}`}
                onClick={startCarry}
              >
                <CarryIcon />
                <span className="sr-only">{t("carry.action")}</span>
              </button>
            )}
            {task && solo && (
              <button type="button" className="tool-btn" title={t("conversation.newLabel")} onClick={() => onSelectTask(null)}>
                <PlusIcon />
                <span className="tool-btn__label">{t("conversation.newLabel")}</span>
              </button>
            )}
            {!solo && panelToggle}
            {more.length > 0 && <MenuButton label={t("workspace.more")} items={more} />}
          </div>
        </header>
        )}
        {details && task && (
          <TaskDetails task={task} events={history} overview={overview} onNavigate={onNavigate} onClose={() => setDetails(false)} />
        )}
        {deleting && task && (
          <DeleteConversation task={task} onClose={() => setDeleting(false)} onDeleted={() => onSelectTask(null)} />
        )}
        {carrying && task && onNavigate && (
          <CarryDialog overview={overview} conversation={task} onClose={() => setCarrying(false)} onNavigate={onNavigate} />
        )}
        {noting && task && onNavigate && (
          <NoteDialog
            overview={overview}
            conversation={task}
            defaultProject={continued.data?.[0]?.project}
            onClose={() => setNoting(false)}
            onNavigate={onNavigate}
          />
        )}

        {/* Tasks that did not load are not "no tasks": the error, and a way to try again. */}
        {tasks.isError && task && (
          <p className="error-text" role="alert">
            {errorMessage(tasks.error, t)}
          </p>
        )}
        {tasks.isError && !task ? (
          <div className="chat__empty">
            <EmptyState
              error
              title={t(solo ? "workspace.conversationsError" : "workspace.tasksError")}
              text={errorMessage(tasks.error, t)}
              onRetry={() => void tasks.refetch()}
            />
          </div>
        ) : task ? (
          <Timeline
            task={task}
            items={items}
            agent={agent}
            account={account}
            accounts={overview.accounts}
            mascot={mascot}
            scrollKey={queue.length}
            footer={
              <>
                <OutputsCard task={task} events={history} />
                {carryable && task?.status === "review" ? (
                  <CarrySuggestion onCarry={startCarry} onNote={startNote} />
                ) : !solo ? (
                  <EndCard
                    task={task}
                    events={history}
                    onReview={() => {
                      openPanel();
                      pressPanelShortcut("G");
                    }}
                  />
                ) : undefined}
              </>
            }
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
            {account && !carried && (
              <Suggestions
                items={suggestions}
                onPick={(s) => setFill({ ...s, n: (fill?.n ?? 0) + 1, scope: project })}
              />
            )}
            {!solo && !carried && <ProjectTasks tasks={list} onOpen={(id) => onSelectTask(id)} />}
          </div>
        )}

        {/* The only live region of the chat: milestones, not the stream. */}
        <p className="sr-only chat__announce" role="status">
          {milestone && <span key={milestone.index}>{t(MILESTONE[milestone.milestone])}</span>}
        </p>

        {task && <QueuedMessages task={task} queue={queue} agent={agent} cardWaiting={task.status === "waitingApproval"} />}

        {task && !solo && !isOpen(task) ? (
          <ClosingBar
            task={task}
            onOpenPanel={() => {
              // Straight to the accepted task's publish steps, with the focus there.
              openPanel();
              pressPanelShortcut("G");
            }}
            onNewTask={newTask}
          />
        ) : (
        <Composer
          key={task?.id ?? "new"}
          scope={project}
          name={name}
          task={task}
          card={card}
          models={account ? MODEL_SUGGESTIONS[account.provider] : []}
          agent={agent}
          disabled={!account}
          imageAccount={isOpen(task) && task ? task.account : (account?.id ?? null)}
          commandAccount={overview.accounts.find(
            (a) => a.id === (isOpen(task) && task ? task.account : (account?.id ?? null)),
          )}
          onCarry={carryable ? startCarry : undefined}
          initialText={task ? "" : draft}
          context={
            carried ? (
              <ContextCard
                draft={carried}
                account={overview.accounts.find((a) => a.id === carriedFrom?.account)}
                onRemove={() => onSelectTask(null, { replace: true })}
              />
            ) : undefined
          }
          sendBlocked={!!carried && (carried.empty || carried.busy || carried.loading)}
          placeholder={carried ? t("carry.placeholder", { name }) : undefined}
          fill={!task && fill?.scope === project ? fill : null}
          autoFocus={chosen === null || (solo && !task)}
          start={
            solo
              ? (api, input) =>
                  api.startConversation({
                    prompt: input.prompt,
                    mode: input.mode,
                    account: account?.id ?? null,
                    model: input.model,
                    ...(input.images ? { images: input.images } : {}),
                  })
              : (api, input) => api.startTask({ project, ...input, ...(carried ? { carry: carried.input } : {}) })
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
          onStarted={(created) => {
            setFill(null);
            if (created.origin) {
              void client.invalidateQueries({ queryKey: continuationsKey(created.origin.conversation) });
              void client.invalidateQueries({ queryKey: contextKey(created.origin.conversation) });
            }
            onSelectTask(created.id, { replace: true });
          }}
        />
        )}
      </section>
      {/* Kept mounted while hidden: a terminal opened there keeps running. */}
      {!solo && (
        <ChangesPanel
          project={project}
          task={task}
          hidden={!panelShown}
          // Opening a file shows the panel, out of focus mode too.
          onRequestOpen={openPanel}
        />
      )}
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
  const hasContent = (editable && (diff.data?.length ?? 0) > 0) || (task?.status === "accepted" && (!!task.merge || !!task.ship));
  const [chosen, setChosen] = useState<boolean | null>(() => {
    try {
      const saved = sessionStorage.getItem(PANEL_KEY);
      return saved === null ? null : saved === "shown";
    } catch {
      return null;
    }
  });
  const shown = chosen ?? hasContent;
  const remember = (on: boolean) => {
    setChosen(on);
    try {
      sessionStorage.setItem(PANEL_KEY, on ? "shown" : "hidden");
    } catch {
      // Remembering the choice is only a convenience.
    }
  };
  return {
    shown,
    open: () => remember(true),
    /** Hidden by the user while there are changes to look at. */
    waiting: !shown && hasContent,
    toggle: () => remember(!shown),
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

/** The mode menu's checkbox: not a mode, it toggles showing the plan first. */
const PLAN_FIRST = "plan-first" as const;

const MODE_DETAIL: Record<TaskMode, MessageKey> = {
  project: "workspace.composer.modeProjectDetail",
  ask: "workspace.composer.modeAskDetail",
  research: "workspace.composer.modeResearchDetail",
};

/** What starts a new task from the composer: in a project, or as a conversation. */
type StartTask = (
  api: HedyApi,
  input: {
    prompt: string;
    mode: TaskMode;
    model: string | null;
    planFirst: boolean;
    permissions?: PermissionMode;
    images?: ImageInput[];
  },
) => Promise<Task>;

const PERMISSION_MODES: PermissionMode[] = ["manual", "acceptEdits", "auto"];
const PERMISSION_LABEL: Record<PermissionMode, MessageKey> = {
  manual: "workspace.permissions.manual",
  acceptEdits: "workspace.permissions.acceptEdits",
  auto: "workspace.permissions.auto",
};
const PERMISSION_DETAIL: Record<PermissionMode, MessageKey> = {
  manual: "workspace.permissions.manualDetail",
  acceptEdits: "workspace.permissions.acceptEditsDetail",
  auto: "workspace.permissions.autoDetail",
};

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
  imageAccount,
  initialText,
  fill,
  autoFocus,
  start: startNew,
  accountPicker,
  context,
  sendBlocked,
  placeholder: placeholderOverride,
  commandAccount,
  onCarry,
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
  /** Who gets the message, to know whether it takes images. */
  imageAccount: string | null;
  initialText?: string;
  /** A suggestion picked in the empty task: its text (and mode) go in, the caret where it says. */
  fill?: (Suggestion & { n: number }) | null;
  /** Takes the focus when it appears (a new task or question). */
  autoFocus?: boolean;
  start: StartTask;
  /** Shown in the bar of a new conversation. */
  accountPicker?: ReactNode;
  /** Above the text of a new task: the context it carries (a conversation). */
  context?: ReactNode;
  /** Sending waits (the carried context is not ready). */
  sendBlocked?: boolean;
  /** Says what to write, instead of the default for the mode. */
  placeholder?: string;
  /** Who gets the message: its slash commands go in the "/" picker. */
  commandAccount?: Account;
  /** "Llevar a un proyecto…", for a conversation that has a task to carry. */
  onCarry?: () => void;
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
  // What the agent does without asking: a new task starts with the project's
  // default (or what was picked here); an open one changes its own.
  const overview = useOverview();
  const projectPermissions = overview.data?.projects.find((p) => p.path === scope)?.permissionMode ?? "manual";
  const [pickedPermissions, setPickedPermissions] = useState<PermissionMode | null>(null);
  const newPermissions = pickedPermissions ?? projectPermissions;
  const permissions: PermissionMode = followUp && task ? (task.permissions ?? "manual") : newPermissions;
  const setTaskPermissions = useAction((api, value: PermissionMode) => api.setTaskPermissions(task?.id ?? "", value), {
    invalidate: [listKey],
  });
  const keys = keyLabels();
  const templates = useTemplates(scope);
  const images = useComposerImages({ account: disabled ? null : imageAccount, queueing });
  const [dropping, setDropping] = useState(false);
  const api = useApi();
  // `@file` mentions: only in a project (a quick question has no files).
  const [caret, setCaret] = useState(0);
  const [fileActive, setFileActive] = useState(0);
  const [mentionClosed, setMentionClosed] = useState<number | null>(null);

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

  // A picked suggestion fills the composer; it is never sent by itself.
  const [filled, setFilled] = useState(0);
  if (fill && fill.n !== filled) {
    setFilled(fill.n);
    setText(fill.text);
    setCaret(fill.caret);
    if (fill.mode && !followUp && (solo ? CONVERSATION_MODES : MODES).includes(fill.mode)) setNewMode(fill.mode);
  }
  useEffect(() => {
    if (!fill) return;
    const frame = requestAnimationFrame(() => {
      const el = input.current;
      if (!el) return;
      el.focus();
      el.setSelectionRange(fill.caret, fill.caret);
    });
    return () => cancelAnimationFrame(frame);
  }, [fill]);

  const query = slashQuery(text);
  // The account's slash commands, read when a message starts with "/".
  const accountCommands = useAccountCommands(commandAccount?.id ?? null, scope, text.trimStart().startsWith("/"));
  const offered = accountCommands.data?.supported ? accountCommands.data : null;
  const known = offered?.commands ?? [];
  const commandsUnknown = !!offered && offered.commands === null;
  // Once a known command is followed by a space, its arguments are being written.
  const argsFor = commandWithArguments(text, known);
  const typedName = commandName(text);
  const typed = typedName ? (known.find((c) => c.name === typedName) ?? null) : null;
  const options: PickerOption[] =
    query === null || argsFor
      ? []
      : [
          ...filterTemplates(templates.data ?? [], query).map((template) => ({ kind: "template" as const, template })),
          ...(offered ? filterCommands(known, query) : []).map((command) => ({ kind: "command" as const, command })),
        ];
  const pickerOpen = query !== null && !pickerClosed && !filling && !argsFor;
  const current = pickerOpen ? options[Math.min(active, options.length - 1)] : undefined;
  const optionId = (index: number) => `${listId}-option-${index}`;

  const mention = solo || pickerOpen || filling ? null : mentionAt(text, caret);
  const mentionOpen = !!mention && mentionClosed !== mention.start;
  const projectFiles = useQuery({
    queryKey: ["projectFiles", scope],
    queryFn: () => api.projectFiles(scope ?? ""),
    enabled: mentionOpen,
    staleTime: 30_000,
  });
  const fileMatches = mentionOpen && mention ? rankFiles(projectFiles.data ?? [], mention.query) : [];
  const currentFile = fileMatches.length > 0 ? Math.min(fileActive, fileMatches.length - 1) : -1;
  const fileListId = `${listId}-files`;
  const fileOptionId = (index: number) => `${fileListId}-${index}`;

  function pickFile(path: string) {
    if (!mention) return;
    const next = insertMention(text, mention.start, caret, path);
    setText(next.text);
    setCaret(next.caret);
    setFileActive(0);
    requestAnimationFrame(() => {
      const el = input.current;
      if (!el) return;
      el.focus();
      el.setSelectionRange(next.caret, next.caret);
    });
  }

  const start = useAction(
    (api, message: { text: string; images: ImageInput[] }) =>
      startNew(api, {
        prompt: message.text,
        mode,
        model: model || null,
        planFirst,
        ...(mode === "project" ? { permissions: newPermissions } : {}),
        ...(message.images.length ? { images: message.images } : {}),
      }),
    {
      onSuccess: (created) => {
        setText("");
        images.clear();
        client.setQueryData<Task[]>(listKey, (old) => [created, ...(old ?? [])]);
        void client.invalidateQueries({ queryKey: ACTIVITY_KEY });
        onStarted(created);
      },
    },
  );
  const send = useAction(
    (api, message: { text: string; images: ImageInput[] }) =>
      message.images.length
        ? api.queueMessage(task?.id ?? "", message.text, message.images)
        : api.queueMessage(task?.id ?? "", message.text),
    {
      invalidate: [listKey],
      onSuccess: (left) => {
        setText("");
        images.clear();
        if (task) setQueue(task.id, left);
      },
    },
  );
  const stop = useAction((api, id: string) => api.stopTask(id), { invalidate: task ? [listKeyFor(task)] : [] });
  const busy = start.isPending || send.isPending;
  // While the agent runs, an empty composer offers to stop it (Esc does too).
  const stoppable = !!task?.live && !text.trim();

  function stopAgent() {
    if (task?.live && !stop.isPending) stop.mutate(task.id);
  }

  function edit(value: string, at?: number | null) {
    setText(value);
    setActive(0);
    setFileActive(0);
    setCaret(at ?? value.length);
    if (!mentionAt(value, at ?? value.length)) setMentionClosed(null);
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

  // Another view ("Preguntar a la IA sobre este archivo") puts a reference here.
  useComposerInsert((snippet) => {
    if (disabled) return;
    const next = insertAt(text, caret, snippet);
    edit(next.text, next.caret);
    requestAnimationFrame(() => {
      const el = input.current;
      if (!el) return;
      el.focus();
      el.setSelectionRange(next.caret, next.caret);
    });
  });

  function choose(template: PromptTemplate) {
    if (template.placeholders.length > 0) {
      setFilling(template);
      return;
    }
    setText(template.text);
    focusInput();
  }

  /** A template as before; a command goes in as `/name `, for its arguments. */
  function pick(option: PickerOption) {
    if (option.kind === "template") {
      choose(option.template);
      return;
    }
    const value = commandText(option.command);
    setText(value);
    setCaret(value.length);
    setActive(0);
    focusInput();
  }

  const sendLabel =
    followUp && card ? t("workspace.composer.sendAfterCard") : queueing ? t("queue.add") : t("workspace.composer.send");

  function submit() {
    const value = text.trim();
    if (!value || busy || disabled || sendBlocked) return;
    const message = { text: value, images: images.inputs() };
    if (followUp) send.mutate(message);
    else start.mutate(message);
  }

  return (
    <form
      className={dropping ? "composer composer--dropping" : "composer"}
      onSubmit={(e) => {
        e.preventDefault();
        if (current) pick(current);
        else submit();
      }}
      onDragOver={(e) => {
        if (disabled || !draggingFiles(e.dataTransfer)) return;
        e.preventDefault();
        e.dataTransfer.dropEffect = images.blocked ? "none" : "copy";
        setDropping(true);
      }}
      onDragLeave={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setDropping(false);
      }}
      onDrop={(e) => {
        if (disabled || !draggingFiles(e.dataTransfer)) return;
        e.preventDefault();
        setDropping(false);
        void images.add(transferFiles(e.dataTransfer));
      }}
    >
      {dropping && (
        <p className="composer__drop" aria-hidden>
          {images.blocked ?? t("composer.images.drop")}
        </p>
      )}
      {mentionOpen && (
        <FilePicker
          id={fileListId}
          files={fileMatches}
          active={currentFile}
          loading={projectFiles.isPending}
          optionId={fileOptionId}
          onPick={pickFile}
          onHover={setFileActive}
        />
      )}
      {pickerOpen && (
        <TemplatePicker
          id={listId}
          options={options}
          current={current}
          optionId={optionId}
          account={offered ? (commandAccount?.name ?? null) : null}
          unknown={commandsUnknown}
          onPick={pick}
          onHover={setActive}
        />
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
      {!followUp && context}
      <ImageTray images={images.images} onRemove={images.remove} onEmptied={focusInput} />
      {argsFor?.argumentHint && (
        <p className="composer__command" id={`${listId}-command`}>
          {t("commands.takes", { name: argsFor.name, hint: argsFor.argumentHint })}
        </p>
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
        aria-controls={
          pickerOpen && options.length > 0 ? listId : mentionOpen && fileMatches.length > 0 ? fileListId : undefined
        }
        aria-activedescendant={
          current ? optionId(options.indexOf(current)) : currentFile >= 0 ? fileOptionId(currentFile) : undefined
        }
        aria-keyshortcuts="Enter"
        aria-describedby={argsFor?.argumentHint ? `${listId}-enter ${listId}-command` : `${listId}-enter`}
        placeholder={
          !followUp && placeholderOverride
            ? placeholderOverride
            : followUp && card
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
        onChange={(e) => edit(e.target.value, e.target.selectionStart)}
        onSelect={(e) => setCaret(e.currentTarget.selectionStart)}
        onPaste={(e) => {
          const files = transferFiles(e.clipboardData);
          if (files.length === 0) return;
          // A copied picture comes as a file; text copied with it still pastes.
          if (!e.clipboardData.getData("text/plain")) e.preventDefault();
          void images.add(files);
        }}
        onKeyDown={(e) => {
          if (mentionOpen && mention) {
            if (fileMatches.length > 0 && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
              e.preventDefault();
              const step = e.key === "ArrowDown" ? 1 : -1;
              setFileActive((i) => (Math.min(i, fileMatches.length - 1) + step + fileMatches.length) % fileMatches.length);
              return;
            }
            const picked = fileMatches[currentFile];
            if ((e.key === "Enter" || e.key === "Tab") && !e.shiftKey && picked) {
              e.preventDefault();
              pickFile(picked);
              return;
            }
            if (e.key === "Escape") {
              e.preventDefault();
              setMentionClosed(mention.start);
              return;
            }
          }
          if (pickerOpen && options.length > 0) {
            if (e.key === "ArrowDown" || e.key === "ArrowUp") {
              e.preventDefault();
              const step = e.key === "ArrowDown" ? 1 : -1;
              setActive((i) => (Math.min(i, options.length - 1) + step + options.length) % options.length);
              return;
            }
            if ((e.key === "Enter" || e.key === "Tab") && !e.shiftKey && current) {
              e.preventDefault();
              pick(current);
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
          {!solo && mode === "project" && (
            <Select<PermissionMode>
              variant="chip"
              label={t("workspace.permissions.label")}
              title={t("workspace.permissions.label")}
              value={permissions}
              options={PERMISSION_MODES.map((value) => ({
                value,
                label: t(PERMISSION_LABEL[value]),
                detail: t(PERMISSION_DETAIL[value]),
              }))}
              onChange={(value) => (followUp && task ? setTaskPermissions.mutate(value) : setPickedPermissions(value))}
            />
          )}
          {!followUp && (
            <>
              <Select<TaskMode | typeof PLAN_FIRST>
                variant="chip"
                label={t("workspace.composer.mode")}
                title={t("workspace.composer.mode")}
                value={mode}
                sections={[
                  {
                    options: (solo ? CONVERSATION_MODES : MODES).map((value) => ({
                      value,
                      label: t(MODE_LABEL[value]),
                      detail: t(MODE_DETAIL[value]),
                      icon: MODE_ICON[value],
                    })),
                  },
                  // "Show me the plan first" is a checkbox in the same menu, for changing code only.
                  ...(mode === "project"
                    ? [
                        {
                          options: [
                            {
                              value: PLAN_FIRST,
                              label: t("workspace.composer.planFirst"),
                              detail: t(planFirst ? "workspace.composer.planFirstOn" : "workspace.composer.planFirstOff"),
                              icon: (
                                <input type="checkbox" checked={planFirst} readOnly tabIndex={-1} style={{ pointerEvents: "none" }} />
                              ),
                            },
                          ],
                        },
                      ]
                    : []),
                ]}
                renderValue={(option) => (
                  <>
                    {option?.icon && (
                      <span className="select__icon" aria-hidden>
                        {option.icon}
                      </span>
                    )}
                    <span className="select__text">{option?.label ?? ""}</span>
                    {mode === "project" && planFirst && (
                      <span className="select__icon" aria-hidden>
                        <PlanIcon />
                      </span>
                    )}
                  </>
                )}
                tooltip={mode === "project" && planFirst ? t("workspace.composer.planFirst") : undefined}
                onChange={(value) => (value === PLAN_FIRST ? setPlanFirst((on) => !on) : setNewMode(value))}
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
          <AttachButton
            blocked={images.blocked}
            supported={images.supported}
            disabled={disabled}
            onFiles={(files) => void images.add(files)}
          />
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
            disabled={!text.trim() || busy || disabled || sendBlocked}
          >
            <SendIcon />
          </button>
        )}
      </div>
      {solo && typed && USERS_SOURCES.includes(typed.source) && (
        <p className="composer__note" role="note">
          <span>{t("commands.readOnly")}</span>
          {onCarry && (
            <button type="button" className="link-button" onClick={onCarry}>
              {t("carry.actionMenu")}
            </button>
          )}
        </p>
      )}
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

/** A template, or one of the account's slash commands, in the "/" picker. */
type PickerOption = { kind: "template"; template: PromptTemplate } | { kind: "command"; command: AgentCommand };

/** Sources the user added: in a quick question these cannot write what they may want to. */
const USERS_SOURCES: CommandSource[] = ["plugin", "skill", "custom"];

const SOURCE_LABEL: Partial<Record<CommandSource, MessageKey>> = {
  plugin: "commands.source.plugin",
  skill: "commands.source.skill",
  custom: "commands.source.custom",
  builtin: "commands.source.builtin",
};

/**
 * The "/" picker: the templates, then (when the account tells Hedy its
 * commands) a group with the account's commands. One active option across
 * both, announced through `aria-activedescendant` on the composer.
 */
function TemplatePicker({
  id,
  options,
  current,
  optionId,
  account,
  unknown,
  onPick,
  onHover,
}: {
  id: string;
  options: PickerOption[];
  current: PickerOption | undefined;
  optionId: (index: number) => string;
  /** The account whose commands are offered; `null`: templates only. */
  account: string | null;
  /** The account has not answered in Hedy yet: its commands are unknown. */
  unknown: boolean;
  onPick: (option: PickerOption) => void;
  onHover: (index: number) => void;
}) {
  const t = useT();
  const item = (option: PickerOption) => {
    const index = options.indexOf(option);
    const active = option === current;
    const props = {
      id: optionId(index),
      role: "option",
      "aria-selected": active,
      onMouseDown: (e: MouseEvent) => {
        e.preventDefault();
        onPick(option);
      },
      onMouseEnter: () => onHover(index),
    } as const;
    if (option.kind === "template") {
      const template = option.template;
      return (
        <li key={`t-${template.id}`} {...props} className={`template-option${active ? " template-option--active" : ""}`}>
          <span className="template-option__name">{template.name}</span>
          <span className="template-option__text">{template.text}</span>
        </li>
      );
    }
    const command = option.command;
    const source = SOURCE_LABEL[command.source];
    return (
      <li
        key={`c-${command.name}`}
        {...props}
        className={`template-option command-option${active ? " template-option--active" : ""}`}
      >
        <span className="command-option__head">
          <span className="template-option__name mono">/{command.name}</span>
          {command.argumentHint && <span className="command-option__hint mono">{command.argumentHint}</span>}
          {source && <span className="command-option__source">{t(source)}</span>}
        </span>
        {command.description && <span className="template-option__text">{command.description}</span>}
      </li>
    );
  };
  const templates = options.filter((o) => o.kind === "template");
  const commands = options.filter((o) => o.kind === "command");
  const label = t("templates.picker");
  return (
    <div className="template-picker">
      {account === null ? (
        templates.length > 0 && (
          <ul className="template-picker__list" role="listbox" id={id} aria-label={label}>
            {templates.map(item)}
          </ul>
        )
      ) : (
        options.length > 0 && (
          <div className="template-picker__list" role="listbox" id={id} aria-label={t("commands.pickerLabel")}>
            {templates.length > 0 && (
              <ul className="template-picker__group" role="group" aria-labelledby={`${id}-templates`}>
                <li className="template-picker__head" role="presentation" id={`${id}-templates`}>
                  {label}
                </li>
                {templates.map(item)}
              </ul>
            )}
            {commands.length > 0 && (
              <ul className="template-picker__group" role="group" aria-labelledby={`${id}-commands`}>
                <li className="template-picker__head" role="presentation" id={`${id}-commands`}>
                  {t("commands.group", { account })}
                </li>
                {commands.map(item)}
              </ul>
            )}
          </div>
        )
      )}
      {options.length === 0 && (
        <p className="template-picker__empty" role="status">
          {account === null ? t("templates.noMatch") : t("commands.noMatch")}
        </p>
      )}
      {account !== null && unknown && (
        <p className="template-picker__note" role="status">
          <span className="template-picker__head">{t("commands.group", { account })}</span>
          {t("commands.unknown")}
        </p>
      )}
      <p className="template-picker__hint" aria-hidden>
        {account === null ? t("templates.pickerHint") : t("commands.pickerHint")}
      </p>
    </div>
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
