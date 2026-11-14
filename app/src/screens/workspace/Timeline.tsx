import { useEffect, useRef, useState } from "react";

import { useAction } from "../../api/context";
import type { Account, Decision, Task } from "../../api/types";
import { Mascot, type MascotState } from "../../components/Mascot/Mascot";
import { Button } from "../../components/ui/Button";
import { formatDuration, type Resolution, type TimelineItem } from "../../tasks/timeline";

interface TimelineProps {
  task: Task;
  items: TimelineItem[];
  agent: string;
  account: Account | undefined;
  mascot: MascotState;
}

export function Timeline({ task, items, agent, account, mascot }: TimelineProps) {
  const end = useRef<HTMLDivElement>(null);
  useEffect(() => {
    end.current?.scrollIntoView?.({ block: "end" });
  }, [items.length]);

  const color = account?.color ?? "#7AA2F7";
  const lastAgent = items.findLastIndex((i) => i.type !== "user");

  return (
    <div className="timeline" aria-live="polite">
      {items.map((item, index) => {
        if (item.type === "user") {
          return (
            <div key={item.key} className="bubble">
              {item.text}
            </div>
          );
        }
        return (
          <div key={item.key} className="agent-row">
            <span className="agent-row__avatar">
              {(index === 0 || items[index - 1]?.type === "user") && (
                <Mascot color={color} state={index === lastAgent ? mascot : "idle"} size={30} label={agent} />
              )}
            </span>
            <div className="agent-row__body">
              <Item task={task} item={item} agent={agent} />
            </div>
          </div>
        );
      })}
      <div ref={end} />
    </div>
  );
}

function Item({ task, item, agent }: { task: Task; item: TimelineItem; agent: string }) {
  switch (item.type) {
    case "agent":
      return <p className="agent-text">{item.text}</p>;
    case "plan":
      return <PlanCard task={task} item={item} agent={agent} />;
    case "steps":
      return (
        <section className="card plan-card" aria-label="Pasos">
          <h3>Plan · {agent}</h3>
          <ul className="steps">
            {item.steps.map((s, i) => (
              <li key={i} className={`step step--${s.status}`}>
                <span className="step__mark" aria-hidden>
                  {s.status === "done" ? "✓" : s.status === "inProgress" ? "●" : "○"}
                </span>
                <span>{s.text}</span>
                <span className="sr-only">
                  {s.status === "done" ? " (hecho)" : s.status === "inProgress" ? " (en curso)" : " (pendiente)"}
                </span>
              </li>
            ))}
          </ul>
        </section>
      );
    case "permission":
      return <PermissionCard task={task} item={item} agent={agent} />;
    case "activity":
      return (
        <p className={`activity activity--${item.tone}`}>
          <span className="activity__mark" aria-hidden>
            {item.tone === "done" ? "✓" : item.tone === "failed" ? "✗" : "●"}
          </span>
          {item.text}
        </p>
      );
    case "finished":
      return (
        <p className={`activity activity--${item.ok ? "done" : "failed"}`}>
          <span className="activity__mark" aria-hidden>
            {item.ok ? "✓" : "✗"}
          </span>
          {item.ok ? "Terminó" : "Terminó con error"}
          {item.durationMs !== null && ` · ${formatDuration(item.durationMs)}`}
        </p>
      );
    case "error":
      return (
        <p className="activity activity--failed" role="alert">
          <span className="activity__mark" aria-hidden>
            ✗
          </span>
          {item.message}
        </p>
      );
    default:
      return null;
  }
}

function resolutionText(r: Resolution): string {
  if (r.automatic) return r.reason ?? (r.decision === "deny" ? "Hedy lo rechazó" : "Permitido por una regla");
  return { allow: "Permitido", allowForTask: "Permitido en este chat", deny: "Rechazado" }[r.decision];
}

const TOOL_TITLE: Record<string, string> = {
  Bash: "quiere correr un comando",
  Edit: "quiere editar un archivo",
  MultiEdit: "quiere editar un archivo",
  Write: "quiere crear un archivo",
  NotebookEdit: "quiere editar un notebook",
  WebFetch: "quiere abrir una página",
  WebSearch: "quiere buscar en la web",
};

function PermissionCard({ task, item, agent }: { task: Task; item: Extract<TimelineItem, { type: "permission" }>; agent: string }) {
  const answer = useAction((api, decision: Decision) => api.answerPermission(task.id, item.requestId, decision));
  const name = agent.split(" ")[0];
  const title = `${name} ${TOOL_TITLE[item.tool] ?? `quiere usar ${item.tool}`}`;

  if (item.resolution) {
    const denied = item.resolution.decision === "deny";
    return (
      <p className={`activity activity--${denied ? "failed" : "done"}`}>
        <span className="activity__mark" aria-hidden>
          {denied ? "✗" : "✓"}
        </span>
        {resolutionText(item.resolution)} · <code>{item.summary}</code>
      </p>
    );
  }
  const waiting = task.live;
  return (
    <section className="permission-card" aria-label={title}>
      <h3>{title}</h3>
      <code className="permission-card__what">{item.summary}</code>
      <div className="actions">
        <Button variant="primary" disabled={!waiting || answer.isPending} onClick={() => answer.mutate("allow")}>
          Permitir
        </Button>
        <Button className="btn--warn" disabled={!waiting || answer.isPending} onClick={() => answer.mutate("allowForTask")}>
          Permitir en este chat
        </Button>
        <Button className="btn--warn" disabled={!waiting || answer.isPending} onClick={() => answer.mutate("deny")}>
          Rechazar
        </Button>
      </div>
      {!waiting && <p className="small">El agente se detuvo; envía un mensaje para retomar.</p>}
    </section>
  );
}

function PlanCard({ task, item, agent }: { task: Task; item: Extract<TimelineItem, { type: "plan" }>; agent: string }) {
  const [changing, setChanging] = useState(false);
  const [feedback, setFeedback] = useState("");
  const answer = useAction((api, vars: { approve: boolean; feedback: string | null }) =>
    api.answerPlan(task.id, item.requestId ?? "", vars.approve, vars.feedback),
  );
  const pending = item.requestId !== null && item.resolution === null;

  return (
    <section className={`card plan-card${pending ? " plan-card--pending" : ""}`} aria-label={`Plan de ${agent}`}>
      <h3>
        Plan · {agent}
        {item.resolution && (
          <span className={`plan-card__state plan-card__state--${item.resolution.decision}`}>
            {item.resolution.decision === "deny" ? " · con cambios pedidos" : " · aprobado"}
          </span>
        )}
      </h3>
      <div className="plan-card__text">{item.text}</div>
      {pending && !changing && (
        <div className="actions">
          <Button variant="primary" disabled={!task.live || answer.isPending} onClick={() => answer.mutate({ approve: true, feedback: null })}>
            Aprobar plan
          </Button>
          <Button disabled={!task.live} onClick={() => setChanging(true)}>
            Pedir cambios
          </Button>
        </div>
      )}
      {pending && changing && (
        <form
          className="plan-card__feedback"
          onSubmit={(e) => {
            e.preventDefault();
            answer.mutate({ approve: false, feedback });
          }}
        >
          <label className="field">
            <span className="field__label">¿Qué cambiarías?</span>
            <textarea value={feedback} onChange={(e) => setFeedback(e.target.value)} rows={2} />
          </label>
          <div className="actions">
            <Button type="submit" variant="primary" disabled={!feedback.trim() || answer.isPending}>
              Enviar cambios
            </Button>
            <Button variant="ghost" onClick={() => setChanging(false)}>
              Cancelar
            </Button>
          </div>
        </form>
      )}
    </section>
  );
}
