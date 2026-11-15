import { useId, useMemo, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";

import { errorMessage, useAction } from "../../api/context";
import type { Overview, PromptTemplate, Task, TaskMode } from "../../api/types";
import { Mascot } from "../../components/Mascot/Mascot";
import { shortPath } from "../../components/projects";
import { Button } from "../../components/ui/Button";
import { Segmented } from "../../components/ui/controls";
import { isBusy, tasksKey, useQueue, useSetQueue, useTaskEvents, useTasks, useTemplates } from "../../tasks/hooks";
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
import { MODEL_SUGGESTIONS } from "../../components/models";
import { useT, type MessageKey } from "../../i18n";
import { DEFAULT_ACCOUNT_COLOR } from "../../theme/tokens";
import { fillTemplate, filterTemplates, slashQuery } from "../../templates/placeholders";
import { TemplateDialog } from "../templates/TemplateDialog";
import { ChangesPanel } from "./ChangesPanel";
import { QueuedMessages } from "./QueuedMessages";
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

export function Workspace({
  overview,
  project,
  task: chosen,
  onSelectTask,
  onOpenMemory,
  onOpenStatus,
}: {
  overview: Overview;
  project: string;
  /** The task to show: an id, `null` for a new one, or (unset) the most recent open one. */
  task?: string | null;
  /** Shows another task (`null`: a new one); `replace` keeps the step out of Back. */
  onSelectTask: (task: string | null, options?: { replace?: boolean }) => void;
  onOpenMemory?: () => void;
  onOpenStatus?: () => void;
}) {
  const t = useT();
  const tasks = useTasks(project);
  const info = overview.projects.find((p) => p.path === project);
  const account = overview.accounts.find((a) => a.id === info?.account);

  // By default, the most recent task that is still open.
  const list = tasks.data ?? [];
  const task =
    chosen === null ? null : (list.find((t) => t.id === chosen) ?? (chosen === undefined ? list.find(isOpen) : undefined) ?? null);

  const events = useTaskEvents(task?.id ?? null);
  const history = useMemo(() => events.data ?? [], [events.data]);
  const items = useMemo(() => buildTimeline(history, t), [history, t]);
  const queue = useQueue(task?.id ?? null).data ?? [];
  const agent = agentName(history);
  const mascot = useMascot(task, history);
  const name = info?.name ?? shortPath(project);
  const milestone = useMilestone(task?.id ?? null, items, events.isSuccess);
  // A message written now waits for the card's answer (and the end of the turn).
  const card: CardWaiting = isBusy(task) && task?.status === "waitingApproval" ? (pendingCard(history) ?? "other") : null;

  return (
    <div className="workspace">
      <section className="chat" aria-label={t("workspace.chat")}>
        <header className="chat__head">
          <div className="chat__title">
            <h1>{task ? task.title : name}</h1>
            <p className="muted">
              {name}
              {task?.workspace && <> · {t("common.branch", { branch: task.workspace.branch })}</>}
              {task && <> · {t(MODE_LABEL[task.mode])} · {t(STATUS_LABEL[task.status])}</>}
            </p>
          </div>
          <div className="chat__tools">
            {list.length > 0 && (
              <select
                className="pill-select"
                aria-label={t("workspace.task")}
                value={task?.id ?? ""}
                onChange={(e) => onSelectTask(e.target.value || null)}
              >
                <option value="">{t("workspace.newTask")}</option>
                {list.map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.id === task?.id
                      ? t("workspace.tasks", { n: list.length })
                      : `${item.title} · ${t(STATUS_LABEL[item.status])}`}
                  </option>
                ))}
              </select>
            )}
            {task?.live && <StopButton task={task} />}
            {onOpenStatus && (
              <Button small variant="ghost" onClick={onOpenStatus}>
                {t("workspace.status")}
              </Button>
            )}
            {onOpenMemory && (
              <Button small variant="ghost" onClick={onOpenMemory}>
                {t("common.memory")}
              </Button>
            )}
          </div>
        </header>

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
            footer={
              <QueuedMessages task={task} queue={queue} agent={agent} cardWaiting={task.status === "waitingApproval"} />
            }
          />
        ) : (
          <div className="chat__empty">
            <Mascot color={account?.color ?? DEFAULT_ACCOUNT_COLOR} state="idle" size={120} label="Hedy" />
            <p>
              {account
                ? t("workspace.empty", { name, account: account.name })
                : t("workspace.noAccount")}
            </p>
          </div>
        )}

        {/* The only live region of the chat: milestones, not the stream. */}
        <p className="sr-only chat__announce" role="status">
          {milestone && <span key={milestone.index}>{t(MILESTONE[milestone.milestone])}</span>}
        </p>

        <Composer
          key={task?.id ?? "new"}
          project={project}
          name={name}
          task={task}
          card={card}
          models={account ? MODEL_SUGGESTIONS[account.provider] : []}
          agent={agent}
          disabled={!account}
          onStarted={(created) => onSelectTask(created.id, { replace: true })}
        />
      </section>
      <ChangesPanel project={project} task={task} />
    </div>
  );
}

function StopButton({ task }: { task: Task }) {
  const t = useT();
  const stop = useAction((api) => api.stopTask(task.id), { invalidate: [tasksKey(task.project)] });
  return (
    <Button small variant="ghost" onClick={() => stop.mutate(undefined)} disabled={stop.isPending}>
      {t("workspace.stop")}
    </Button>
  );
}

const MODES = Object.keys(MODE_LABEL) as TaskMode[];

/** The mode the composer starts new tasks in, remembered per project (a convenience: storage may fail). */
function useProjectMode(project: string): [TaskMode, (mode: TaskMode) => void] {
  const key = `hedy.mode.${project}`;
  const [mode, setMode] = useState<TaskMode>(() => {
    try {
      const saved = localStorage.getItem(key);
      return MODES.find((m) => m === saved) ?? "project";
    } catch {
      return "project";
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

const CARD_LABEL: Record<NonNullable<CardWaiting>, MessageKey> = {
  plan: "workspace.composer.cardPlan",
  permission: "workspace.composer.cardPermission",
  other: "workspace.composer.cardOther",
};

function Composer({
  project,
  name,
  task,
  card,
  models,
  agent,
  disabled,
  onStarted,
}: {
  project: string;
  name: string;
  task: Task | null;
  /** A plan or permission card waits: what is written now goes after its answer. */
  card: CardWaiting;
  models: string[];
  /** Who works on the task now, for the queue's label. */
  agent: string;
  disabled: boolean;
  onStarted: (task: Task) => void;
}) {
  const t = useT();
  const client = useQueryClient();
  const setQueue = useSetQueue();
  const input = useRef<HTMLTextAreaElement>(null);
  const listId = useId();
  const [text, setText] = useState("");
  const [model, setModel] = useState("");
  const [planFirst, setPlanFirst] = useState(true);
  const [active, setActive] = useState(0);
  const [pickerClosed, setPickerClosed] = useState(false);
  const [filling, setFilling] = useState<PromptTemplate | null>(null);
  const [saving, setSaving] = useState(false);
  const followUp = isOpen(task);
  // While the agent works, a follow-up waits in the queue for the end of its turn.
  const queueing = followUp && isBusy(task);
  const [newMode, setNewMode] = useProjectMode(project);
  // A follow-up stays in its task's mode; the selector is only for new tasks.
  const mode = followUp && task ? task.mode : newMode;
  const templates = useTemplates(project);

  const query = slashQuery(text);
  const matches = query === null ? [] : filterTemplates(templates.data ?? [], query);
  const pickerOpen = query !== null && !pickerClosed && !filling;
  const current = pickerOpen ? matches[Math.min(active, matches.length - 1)] : undefined;
  const optionId = (index: number) => `${listId}-option-${index}`;

  const start = useAction(
    (api, prompt: string) => api.startTask({ project, prompt, mode, model: model || null, planFirst }),
    {
      onSuccess: (created) => {
        setText("");
        client.setQueryData<Task[]>(tasksKey(project), (old) => [created, ...(old ?? [])]);
        onStarted(created);
      },
    },
  );
  const send = useAction((api, message: string) => api.queueMessage(task?.id ?? "", message), {
    invalidate: [tasksKey(project)],
    onSuccess: (left) => {
      setText("");
      if (task) setQueue(task.id, left);
    },
  });
  const busy = start.isPending || send.isPending;

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
      <label className="composer__label" htmlFor="composer-input">
        {followUp && card
          ? t(CARD_LABEL[card])
          : queueing
            ? t("queue.composerBusy", { agent })
            : followUp
              ? t("workspace.composer.followUp")
              : t("workspace.composer.ask", { name })}
      </label>
      <textarea
        ref={input}
        id="composer-input"
        className="composer__input"
        rows={3}
        value={text}
        disabled={disabled}
        aria-autocomplete="list"
        aria-haspopup="listbox"
        aria-controls={pickerOpen && matches.length > 0 ? listId : undefined}
        aria-activedescendant={current ? optionId(matches.indexOf(current)) : undefined}
        placeholder={
          mode === "project" ? t("workspace.composer.placeholderProject") : t("workspace.composer.placeholderAsk")
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
          if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
            e.preventDefault();
            submit();
          }
        }}
      />
      <div className="composer__bar">
        {!followUp && (
          <>
            <Segmented<TaskMode>
              label={t("workspace.composer.mode")}
              value={mode}
              options={MODES.map((value) => ({ value, label: t(MODE_LABEL[value]) }))}
              onChange={setNewMode}
            />
            <select className="pill-select" aria-label={t("common.models")} value={model} onChange={(e) => setModel(e.target.value)}>
              <option value="">{t("workspace.composer.autoModels")}</option>
              {models.map((m) => (
                <option key={m} value={m}>
                  {t("workspace.composer.onlyModel", { model: m })}
                </option>
              ))}
            </select>
            {mode === "project" && (
              <label className="check">
                <input type="checkbox" checked={planFirst} onChange={(e) => setPlanFirst(e.target.checked)} />
                {t("workspace.composer.planFirst")}
              </label>
            )}
          </>
        )}
        <Button small variant="ghost" disabled={!text.trim() || query !== null} onClick={() => setSaving(true)}>
          {t("templates.saveCurrent")}
        </Button>
        <span className="spacer" />
        <span className="muted small">Ctrl + Enter</span>
        <Button type="submit" variant="primary" disabled={!text.trim() || busy || disabled}>
          {followUp && card ? t("workspace.composer.sendAfterCard") : queueing ? t("queue.add") : t("workspace.composer.send")}
        </Button>
      </div>
      {saving && (
        <TemplateDialog
          title={t("templates.saveCurrentTitle")}
          initialText={text.trim()}
          projects={[{ path: project, name }]}
          defaultProject={project}
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
