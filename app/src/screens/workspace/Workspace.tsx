import { useId, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";

import { errorMessage, useAction } from "../../api/context";
import type { Overview, PromptTemplate, Task, TaskMode } from "../../api/types";
import { Mascot } from "../../components/Mascot/Mascot";
import { shortPath } from "../../components/projects";
import { Button } from "../../components/ui/Button";
import { isBusy, tasksKey, useQueue, useSetQueue, useTaskEvents, useTasks, useTemplates } from "../../tasks/hooks";
import { agentName, buildTimeline, isOpen, MODE_LABEL, STATUS_LABEL } from "../../tasks/timeline";
import { useMascot } from "../../tasks/useMascot";
import { MODEL_SUGGESTIONS } from "../../components/models";
import { useT } from "../../i18n";
import { DEFAULT_ACCOUNT_COLOR } from "../../theme/tokens";
import { fillTemplate, filterTemplates, slashQuery } from "../../templates/placeholders";
import { TemplateDialog } from "../templates/TemplateDialog";
import { ChangesPanel } from "./ChangesPanel";
import { QueuedMessages } from "./QueuedMessages";
import { Timeline } from "./Timeline";

export function Workspace({
  overview,
  project,
  mode,
  initialTask,
  onOpenMemory,
  onOpenStatus,
}: {
  overview: Overview;
  project: string;
  mode: TaskMode;
  /** Open this task (from the task board or a notification); `null`: a new one. */
  initialTask?: string | null;
  onOpenMemory?: () => void;
  onOpenStatus?: () => void;
}) {
  const t = useT();
  const tasks = useTasks(project);
  const [chosen, setChosen] = useState<string | null | undefined>(initialTask);
  const info = overview.projects.find((p) => p.path === project);
  const account = overview.accounts.find((a) => a.id === info?.account);

  // By default, the most recent task that is still open.
  const list = tasks.data ?? [];
  const task =
    chosen === null ? null : (list.find((t) => t.id === chosen) ?? (chosen === undefined ? list.find(isOpen) : undefined) ?? null);

  const events = useTaskEvents(task?.id ?? null);
  const history = events.data ?? [];
  const queue = useQueue(task?.id ?? null).data ?? [];
  const agent = agentName(history);
  const mascot = useMascot(task, history);
  const name = info?.name ?? shortPath(project);

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
                onChange={(e) => setChosen(e.target.value || null)}
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
            items={buildTimeline(history, t)}
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

        <Composer
          key={task?.id ?? "new"}
          project={project}
          name={name}
          task={task}
          mode={mode}
          models={account ? MODEL_SUGGESTIONS[account.provider] : []}
          agent={agent}
          disabled={!account}
          onStarted={(t) => setChosen(t.id)}
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

function Composer({
  project,
  name,
  task,
  mode,
  models,
  agent,
  disabled,
  onStarted,
}: {
  project: string;
  name: string;
  task: Task | null;
  mode: TaskMode;
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
        {queueing
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
          {queueing ? t("queue.add") : t("workspace.composer.send")}
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
