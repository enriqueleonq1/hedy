import { useQuery } from "@tanstack/react-query";

import { useApi } from "../../api/context";
import type { Account, Overview, Phase, Task, UsagePoint } from "../../api/types";
import { usageLabel } from "../../components/Layout";
import { Mascot } from "../../components/Mascot/Mascot";
import { projectColor, shortPath } from "../../components/projects";
import { Button } from "../../components/ui/Button";
import { useTaskEvents, useTasks } from "../../tasks/hooks";
import { agentName, formatTokens, isOpen, mascotState, PHASE_VERB } from "../../tasks/timeline";
import { timeAgo } from "../memory/MemoryScreen";

const DAY_LETTER = ["D", "L", "M", "M", "J", "V", "S"];
const DAY_NAME = ["domingo", "lunes", "martes", "miércoles", "jueves", "viernes", "sábado"];

const TASK_BADGE: Record<Task["status"], { label: string; tone: string }> = {
  running: { label: "Trabajando", tone: "warn" },
  waitingApproval: { label: "Te espera", tone: "info" },
  review: { label: "Listo", tone: "ok" },
  accepted: { label: "Aceptada", tone: "muted" },
  discarded: { label: "Descartada", tone: "muted" },
  failed: { label: "Con error", tone: "danger" },
};

const NEXT_PHASE: Record<Phase, string> = {
  plan: "lo planifica",
  implement: "lo implementa",
  tests: "escribe los tests",
  review: "lo revisará",
};

export interface DayUsage {
  /** Local midnight. */
  date: Date;
  tokens: number;
}

/** The last 7 local days, oldest first, with the tokens of each. */
export function usageByDay(points: UsagePoint[], today = new Date()): DayUsage[] {
  const days: DayUsage[] = Array.from({ length: 7 }, (_, i) => {
    const date = new Date(today);
    date.setHours(0, 0, 0, 0);
    date.setDate(date.getDate() - (6 - i));
    return { date, tokens: 0 };
  });
  for (const p of points) {
    const at = new Date(p.at * 1000);
    const day = days.find((d) => d.date.toDateString() === at.toDateString());
    if (day) day.tokens += p.tokens;
  }
  return days;
}

/** One sentence about what is going on in the project right now. */
export function headline(task: Task | undefined, agent: string, accounts: Account[]): string {
  if (!task) return "No hay tareas en curso. Escribe un pedido en el chat para empezar.";
  const title = `«${task.title}»`;
  const stages = task.stages ?? [];
  const current = stages[task.stage ?? 0];
  switch (task.status) {
    case "running": {
      const doing = current ? `${agent} ${PHASE_VERB[current.phase]} ${title}` : `${agent} trabaja en ${title}`;
      const next = stages[(task.stage ?? 0) + 1];
      if (!next) return `${doing}.`;
      const who = accounts.find((a) => a.id === next.account)?.name ?? next.account;
      return `${doing}; después ${who} ${NEXT_PHASE[next.phase]}.`;
    }
    case "waitingApproval":
      return `${title} espera tu respuesta.`;
    case "review":
      return `${title} está lista para revisar.`;
    case "failed":
      return `${title} terminó con un error.`;
    default:
      return "No hay tareas en curso.";
  }
}

export function StatusScreen({
  overview,
  project,
  onOpenChat,
  onOpenMemory,
}: {
  overview: Overview;
  project: string;
  onOpenChat: () => void;
  onOpenMemory: () => void;
}) {
  const api = useApi();
  const status = useQuery({ queryKey: ["status", project], queryFn: () => api.projectStatus(project) });
  const usage = useQuery({ queryKey: ["projectUsage", project], queryFn: () => api.projectUsage(project, 7) });
  const commits = useQuery({ queryKey: ["commits", project], queryFn: () => api.recentCommits(project) });
  const tasks = useTasks(project);
  const info = overview.projects.find((p) => p.path === project);
  const name = info?.name ?? shortPath(project);
  const list = tasks.data ?? [];
  const current = list.find(isOpen);
  const events = useTaskEvents(current?.id ?? null);
  const history = events.data ?? [];
  const account = overview.accounts.find((a) => a.id === (current?.account ?? info?.account));
  const color = account?.color ?? (info ? projectColor(info, overview.accounts) : "#7AA2F7");
  const agents = (status.data?.agents ?? [])
    .map((id) => overview.accounts.find((a) => a.id === id))
    .filter((a): a is Account => !!a);

  return (
    <div className="screen status">
      <section className="card status__hero" aria-label="Estado del proyecto">
        <Mascot color={color} state={mascotState(current, history)} size={120} label="" />
        <div className="status__hero-text">
          <p className="status__eyebrow">Estado del proyecto</p>
          <h1>{name}</h1>
          <p className="muted">{headline(current, agentName(history), overview.accounts)}</p>
          <div className="actions">
            <Button small variant="primary" onClick={onOpenChat}>
              Abrir chat
            </Button>
            <Button small onClick={onOpenMemory}>
              Memoria
            </Button>
          </div>
        </div>
        <div className="status__chips">
          {agents.map((a) => (
            <span key={a.id} className="chip">
              <span className="dot" style={{ background: a.color }} aria-hidden />
              {a.name}
            </span>
          ))}
          {status.data?.branch && (
            <span className="chip chip--muted">
              rama {status.data.branch}
              {status.data.uncommitted ? ` · ${status.data.uncommitted} cambios sin commit` : " · sin cambios"}
            </span>
          )}
          {info?.client && <span className="chip chip--muted">cliente: {info.client}</span>}
        </div>
      </section>

      {status.data && (
        <div className="status__stats">
          <Stat label="Tests" tone={status.data.lastTests ? (status.data.lastTests.passed ? "ok" : "danger") : "muted"}
            value={status.data.lastTests ? (status.data.lastTests.passed ? "Pasaron" : "Fallaron") : "Sin datos"}
            detail={
              status.data.lastTests
                ? `${timeAgo(status.data.lastTests.at)} · ${status.data.lastTests.command}`
                : "Aún ninguna tarea corrió tests aquí."
            }
          />
          <Stat
            label="Modelos"
            tone="info"
            value={`${agents.length} ${agents.length === 1 ? "activo" : "activos"}`}
            detail={agents.map((a) => `${a.name} ${usageLabel(a)}`).join(" · ") || "Asigna una cuenta al proyecto."}
          />
          <Stat
            label="Mapa del código"
            tone={status.data.codeMap ? "accent" : "muted"}
            value={status.data.codeMap ? "Activo" : "Apagado"}
            detail={status.data.codeMap ? "Tus IAs consultan el índice del código." : "Actívalo en Ajustes."}
          />
          <Stat
            label="Memoria"
            tone="warn"
            value={`${status.data.memoryNotes} ${status.data.memoryNotes === 1 ? "nota" : "notas"}`}
            detail={
              [
                status.data.memoryStale > 0 && `${status.data.memoryStale} para revisar`,
                status.data.memoryProposed > 0 &&
                  `${status.data.memoryProposed} ${status.data.memoryProposed === 1 ? "propuesta" : "propuestas"}`,
              ]
                .filter(Boolean)
                .join(" · ") || "Todo al día."
            }
          />
        </div>
      )}

      <div className="status__layout">
        <section className="card status__tasks" aria-labelledby="status-tasks">
          <h2 id="status-tasks">Tareas</h2>
          {list.length === 0 && <p className="muted">Todavía no hay tareas en este proyecto.</p>}
          <ul className="status__task-list">
            {list.slice(0, 6).map((t) => (
              <TaskRow key={t.id} task={t} accounts={overview.accounts} />
            ))}
          </ul>
        </section>
        <div className="status__side">
          <WeekChart name={name} points={usage.data ?? []} />
          <section className="card" aria-labelledby="status-commits">
            <h2 id="status-commits">Últimos commits</h2>
            {commits.data?.length === 0 && <p className="muted small">Sin commits todavía.</p>}
            <ul className="status__commits">
              {commits.data?.map((c) => (
                <li key={c.hash}>
                  <span className="mono status__hash">{c.hash}</span>
                  <span>{c.subject}</span>
                </li>
              ))}
            </ul>
          </section>
        </div>
      </div>
    </div>
  );
}

function Stat({ label, value, detail, tone }: { label: string; value: string; detail: string; tone: string }) {
  return (
    <section className="card stat" aria-label={label}>
      <h2 className="stat__label">{label}</h2>
      <p className={`stat__value stat__value--${tone}`}>{value}</p>
      <p className="small muted">{detail}</p>
    </section>
  );
}

function TaskRow({ task, accounts }: { task: Task; accounts: Account[] }) {
  const events = useTaskEvents(task.id);
  const history = events.data ?? [];
  const account = accounts.find((a) => a.id === task.account);
  const badge = TASK_BADGE[task.status];
  const steps = [...history].reverse().find((e) => e.kind === "steps");
  const done = steps?.kind === "steps" ? steps.steps.filter((s) => s.status === "done").length : 0;
  const agent = agentName(history);
  const detail =
    task.status === "running" && steps?.kind === "steps"
      ? `${agent} · ${done} de ${steps.steps.length} pasos`
      : task.status === "waitingApproval"
        ? `${agent.split(" ")[0]} necesita tu respuesta`
        : `${agent} · ${timeAgo(task.createdAt)}`;
  return (
    <li className="status__task">
      <Mascot color={account?.color ?? "#7AA2F7"} state={mascotState(task, history)} size={48} label="" />
      <div className="status__task-text">
        <strong>{task.title}</strong>
        <span className="small muted">{detail}</span>
      </div>
      <span className={`status__badge status__badge--${badge.tone}`}>{badge.label}</span>
    </li>
  );
}

function WeekChart({ name, points }: { name: string; points: UsagePoint[] }) {
  const days = usageByDay(points);
  const max = Math.max(...days.map((d) => d.tokens), 1);
  const busiest = days.reduce((a, b) => (b.tokens > a.tokens ? b : a));
  const total = days.reduce((sum, d) => sum + d.tokens, 0);
  return (
    <section className="card" aria-labelledby="status-week">
      <h2 id="status-week">Uso de {name} esta semana</h2>
      <div className="week-chart" role="img" aria-label={`Tokens por día; total ${formatTokens(total)}`}>
        {days.map((d) => {
          const label = `${DAY_NAME[d.date.getDay()]} ${d.date.getDate()}: ${formatTokens(d.tokens)} tokens`;
          return (
            <div key={d.date.toISOString()} className="week-chart__day" title={label}>
              <div className="week-chart__track">
                <div
                  className={`week-chart__bar${d.tokens === 0 ? " week-chart__bar--empty" : ""}`}
                  style={{ height: `${d.tokens === 0 ? 0 : Math.max(4, (d.tokens / max) * 100)}%` }}
                />
                <span className="week-chart__tip" role="tooltip">
                  {formatTokens(d.tokens)}
                </span>
              </div>
              <span className="week-chart__letter" aria-hidden>
                {DAY_LETTER[d.date.getDay()]}
              </span>
            </div>
          );
        })}
      </div>
      <p className="small muted">
        {total === 0
          ? "Sin uso registrado esta semana."
          : `El ${DAY_NAME[busiest.date.getDay()]} fue el día más intenso · ${formatTokens(total)} tokens en total.`}
      </p>
      <table className="sr-only">
        <caption>Tokens por día</caption>
        <tbody>
          {days.map((d) => (
            <tr key={d.date.toISOString()}>
              <th scope="row">{`${DAY_NAME[d.date.getDay()]} ${d.date.getDate()}`}</th>
              <td>{d.tokens}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}
