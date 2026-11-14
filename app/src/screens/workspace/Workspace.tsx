import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";

import { errorMessage, useAction } from "../../api/context";
import type { Overview, Task, TaskMode } from "../../api/types";
import { Mascot } from "../../components/Mascot/Mascot";
import { shortPath } from "../../components/projects";
import { Button } from "../../components/ui/Button";
import { tasksKey, useTaskEvents, useTasks } from "../../tasks/hooks";
import { agentName, buildTimeline, isOpen, mascotState, MODE_LABEL, STATUS_LABEL } from "../../tasks/timeline";
import { MODEL_SUGGESTIONS } from "../../components/models";
import { DEFAULT_ACCOUNT_COLOR } from "../../theme/tokens";
import { ChangesPanel } from "./ChangesPanel";
import { Timeline } from "./Timeline";

export function Workspace({
  overview,
  project,
  mode,
  onOpenMemory,
  onOpenStatus,
}: {
  overview: Overview;
  project: string;
  mode: TaskMode;
  onOpenMemory?: () => void;
  onOpenStatus?: () => void;
}) {
  const tasks = useTasks(project);
  const [chosen, setChosen] = useState<string | null | undefined>(undefined);
  const info = overview.projects.find((p) => p.path === project);
  const account = overview.accounts.find((a) => a.id === info?.account);

  // By default, the most recent task that is still open.
  const list = tasks.data ?? [];
  const task =
    chosen === null ? null : (list.find((t) => t.id === chosen) ?? (chosen === undefined ? list.find(isOpen) : undefined) ?? null);

  const events = useTaskEvents(task?.id ?? null);
  const history = events.data ?? [];
  const agent = agentName(history);
  const name = info?.name ?? shortPath(project);

  return (
    <div className="workspace">
      <section className="chat" aria-label="Chat">
        <header className="chat__head">
          <div className="chat__title">
            <h1>{task ? task.title : name}</h1>
            <p className="muted">
              {name}
              {task?.workspace && <> · rama {task.workspace.branch}</>}
              {task && <> · {MODE_LABEL[task.mode]} · {STATUS_LABEL[task.status]}</>}
            </p>
          </div>
          <div className="chat__tools">
            {list.length > 0 && (
              <select
                className="pill-select"
                aria-label="Tarea"
                value={task?.id ?? ""}
                onChange={(e) => setChosen(e.target.value || null)}
              >
                <option value="">+ Nueva tarea</option>
                {list.map((t) => (
                  <option key={t.id} value={t.id}>
                    {t.id === task?.id ? `Tareas (${list.length})` : `${t.title} · ${STATUS_LABEL[t.status]}`}
                  </option>
                ))}
              </select>
            )}
            {task?.live && <StopButton task={task} />}
            {onOpenStatus && (
              <Button small variant="ghost" onClick={onOpenStatus}>
                Estado
              </Button>
            )}
            {onOpenMemory && (
              <Button small variant="ghost" onClick={onOpenMemory}>
                Memoria
              </Button>
            )}
          </div>
        </header>

        {tasks.isError && (
          <p className="error-text" role="alert">
            {errorMessage(tasks.error)}
          </p>
        )}
        {task ? (
          <Timeline
            task={task}
            items={buildTimeline(history)}
            agent={agent}
            account={account}
            accounts={overview.accounts}
            mascot={mascotState(task, history)}
          />
        ) : (
          <div className="chat__empty">
            <Mascot color={account?.color ?? DEFAULT_ACCOUNT_COLOR} state="idle" size={120} label="Hedy" />
            <p>
              {account
                ? `¿Qué hacemos en ${name}? Trabajaré con ${account.name} en una rama aparte; nada toca tu carpeta hasta que aceptes.`
                : "Este proyecto no tiene cuenta. Asígnale una en Cuentas y modelos."}
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
          disabled={!account}
          onStarted={(t) => setChosen(t.id)}
        />
      </section>
      <ChangesPanel project={project} task={task} />
    </div>
  );
}

function StopButton({ task }: { task: Task }) {
  const stop = useAction((api) => api.stopTask(task.id), { invalidate: [tasksKey(task.project)] });
  return (
    <Button small variant="ghost" onClick={() => stop.mutate(undefined)} disabled={stop.isPending}>
      Detener
    </Button>
  );
}

function Composer({
  project,
  name,
  task,
  mode,
  models,
  disabled,
  onStarted,
}: {
  project: string;
  name: string;
  task: Task | null;
  mode: TaskMode;
  models: string[];
  disabled: boolean;
  onStarted: (task: Task) => void;
}) {
  const client = useQueryClient();
  const [text, setText] = useState("");
  const [model, setModel] = useState("");
  const [planFirst, setPlanFirst] = useState(true);
  const followUp = isOpen(task);

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
  const send = useAction((api, message: string) => api.sendMessage(task?.id ?? "", message), {
    invalidate: [tasksKey(project)],
    onSuccess: () => setText(""),
  });
  const busy = start.isPending || send.isPending;

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
        submit();
      }}
    >
      <label className="composer__label" htmlFor="composer-input">
        {followUp ? "Responde o pide algo más" : `Pídele algo a ${name}`}
      </label>
      <textarea
        id="composer-input"
        className="composer__input"
        rows={3}
        value={text}
        disabled={disabled}
        placeholder={mode === "project" ? "Ej.: agrega un reporte semanal de comisiones" : "Ej.: ¿cómo se calculan las comisiones?"}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
            e.preventDefault();
            submit();
          }
        }}
      />
      <div className="composer__bar">
        {!followUp && (
          <>
            <select className="pill-select" aria-label="Modelos" value={model} onChange={(e) => setModel(e.target.value)}>
              <option value="">Modelos: auto por fase</option>
              {models.map((m) => (
                <option key={m} value={m}>
                  Solo {m}
                </option>
              ))}
            </select>
            {mode === "project" && (
              <label className="check">
                <input type="checkbox" checked={planFirst} onChange={(e) => setPlanFirst(e.target.checked)} />
                Planificar primero
              </label>
            )}
          </>
        )}
        <span className="spacer" />
        <span className="muted small">Ctrl + Enter</span>
        <Button type="submit" variant="primary" disabled={!text.trim() || busy || disabled}>
          Enviar
        </Button>
      </div>
    </form>
  );
}
