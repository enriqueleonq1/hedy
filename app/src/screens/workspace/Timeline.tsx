import { useEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";

import { useAction, useApi } from "../../api/context";
import type { Account, Decision, Task } from "../../api/types";
import { Mascot, type MascotState } from "../../components/Mascot/Mascot";
import { Button } from "../../components/ui/Button";
import { formatDuration, isOpen, PHASE_VERB, type Resolution, type TimelineItem } from "../../tasks/timeline";

interface TimelineProps {
  task: Task;
  items: TimelineItem[];
  agent: string;
  account: Account | undefined;
  accounts: Account[];
  mascot: MascotState;
}

export function Timeline({ task, items, agent, account, accounts, mascot }: TimelineProps) {
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
        if (item.type === "phase") {
          return (
            <p key={item.key} className="phase-divider">
              <span>
                <strong>{item.agent}</strong> {PHASE_VERB[item.phase]} · {item.account}
              </span>
            </p>
          );
        }
        if (item.type === "limit") {
          return <LimitCard key={item.key} task={task} item={item} accounts={accounts} />;
        }
        return (
          <div key={item.key} className="agent-row">
            <span className="agent-row__avatar">
              {(index === 0 || items[index - 1]?.type === "user") && (
                <Mascot color={color} state={index === lastAgent ? mascot : "idle"} size={30} label={agent} />
              )}
            </span>
            <div className="agent-row__body">
              <Item task={task} item={item} />
            </div>
          </div>
        );
      })}
      <div ref={end} />
    </div>
  );
}

function Item({ task, item }: { task: Task; item: TimelineItem }) {
  switch (item.type) {
    case "agent":
      return <p className="agent-text">{item.text}</p>;
    case "plan":
      return <PlanCard task={task} item={item} agent={item.agent} />;
    case "steps":
      return (
        <section className="card plan-card" aria-label="Pasos">
          <h3>Pasos</h3>
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
      return <PermissionCard task={task} item={item} />;
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

function PermissionCard({ task, item }: { task: Task; item: Extract<TimelineItem, { type: "permission" }> }) {
  const answer = useAction((api, decision: Decision) => api.answerPermission(task.id, item.requestId, decision));
  const name = item.agent.split(" ")[0];
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

function resetsIn(resetsAt: number | null): string | null {
  if (!resetsAt) return null;
  const minutes = Math.round((resetsAt * 1000 - Date.now()) / 60_000);
  if (minutes <= 0) return "Ya debería haberse renovado.";
  return minutes < 90 ? `Se renueva en ${minutes} min.` : `Se renueva en ${Math.round(minutes / 60)} h.`;
}

/**
 * Hedy never switches accounts by itself (spec rule 3): the card offers to
 * wait, or to hand the task to another provider or an API key.
 */
function LimitCard({
  task,
  item,
  accounts,
}: {
  task: Task;
  item: Extract<TimelineItem, { type: "limit" }>;
  accounts: Account[];
}) {
  const api = useApi();
  const [waiting, setWaiting] = useState(false);
  const actionable = item.latest && isOpen(task) && !waiting;
  const options = useQuery({
    queryKey: ["handoff", task.id, item.key],
    queryFn: () => api.handoffOptions(task.id),
    enabled: actionable,
  });
  const handoff = useAction((a, account: string) => a.handoffTask(task.id, account, null));
  const account = accounts.find((a) => a.id === item.account);
  const name = account?.name ?? item.account;
  const title =
    item.level === "reached"
      ? `${name} llegó a su límite`
      : item.percent != null
        ? `Tu cuenta ${name} va por el ${Math.round(item.percent)}%`
        : `Tu cuenta ${name} está cerca de su límite`;
  const candidates = (options.data ?? [])
    .map((id) => accounts.find((a) => a.id === id))
    .filter((a): a is Account => !!a);

  return (
    <section className="limit-card" aria-label={title}>
      <Mascot color={account?.color ?? "#E0AF68"} state="sleeping" size={36} label="" />
      <div className="limit-card__body">
        <h3>{title}</h3>
        <p>
          {resetsIn(item.resetsAt) ?? ""} {candidates.length > 0 && "Puedo seguir la tarea con otro modelo."}
        </p>
        {actionable && (
          <div className="actions">
            <Button variant="primary" onClick={() => setWaiting(true)}>
              Esperar
            </Button>
            {candidates.map((c) => (
              <Button key={c.id} className="btn--warn" disabled={handoff.isPending} onClick={() => handoff.mutate(c.id)}>
                Pasar la tarea a {c.name}
              </Button>
            ))}
          </div>
        )}
        {actionable && options.isSuccess && candidates.length === 0 && (
          <p className="small">
            No hay otro proveedor disponible aquí (o el proyecto está protegido). Puedes agregar Codex o una API key en
            Cuentas y modelos.
          </p>
        )}
        {waiting && <p className="small">De acuerdo, esperamos. La tarea sigue con esta cuenta.</p>}
      </div>
    </section>
  );
}
