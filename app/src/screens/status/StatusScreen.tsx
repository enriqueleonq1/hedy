import { useQuery } from "@tanstack/react-query";

import { errorMessage, useApi } from "../../api/context";
import type { Account, ActivityItem, Overview, Phase, Task, UsagePoint } from "../../api/types";
import type { View } from "../../components/Layout";
import { usageLabel } from "../../components/planUsage";
import { Mascot } from "../../components/Mascot/Mascot";
import { WorkingWord } from "../../components/Mascot/WorkingWord";
import { projectColor, shortPath } from "../../components/projects";
import { Button } from "../../components/ui/Button";
import { EmptyState } from "../../components/ui/EmptyState";
import { StatusPill } from "../../components/ui/StatusPill";
import { formatTokens, timeAgo } from "../../format";
import { useLanguage, useT, type Language, type MessageKey, type Translate } from "../../i18n";
import { useActivity, useTaskEvents, useTasks } from "../../tasks/hooks";
import { taskState } from "../../tasks/state";
import { agentName, isOpen, PHASE_VERB } from "../../tasks/timeline";
import { useMascot } from "../../tasks/useMascot";
import { DEFAULT_ACCOUNT_COLOR } from "../../theme/tokens";
import { OwnerBanner } from "../sync/ProjectSync";
import { ProjectHead, type ProjectTab } from "../workspace/ProjectHome";
import "./status.css";

const NEXT_PHASE: Record<Phase, MessageKey> = {
  plan: "status.next.plan",
  implement: "status.next.implement",
  tests: "status.next.tests",
  review: "status.next.review",
};

/** "lunes" / "Monday". */
function dayName(date: Date, language: Language): string {
  return new Intl.DateTimeFormat(language, { weekday: "long" }).format(date);
}

interface DayUsage {
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
export function headline(task: Task | undefined, agent: string, accounts: Account[], t: Translate): string {
  if (!task) return t("status.headline.none");
  const title = t("status.headline.quoted", { title: task.title });
  const { stages, stage } = task;
  const current = stages[stage];
  switch (task.status) {
    case "running": {
      const doing = current
        ? t("status.headline.doing", { agent, verb: t(PHASE_VERB[current.phase]), title })
        : t("status.headline.working", { agent, title });
      const next = stages[stage + 1];
      if (!next) return `${doing}.`;
      const who = accounts.find((a) => a.id === next.account)?.name ?? next.account;
      return t("status.headline.then", { doing, who, next: t(NEXT_PHASE[next.phase]) });
    }
    case "waitingApproval":
      return t("status.headline.waiting", { title });
    case "review":
      return t("status.headline.review", { title });
    case "failed":
      return t("status.headline.failed", { title });
    default:
      return t("status.headline.idle");
  }
}

export function StatusScreen({
  overview,
  project,
  onOpenChat,
  onOpenMemory,
  onOpenSettings,
  onNavigate,
}: {
  overview: Overview;
  project: string;
  onOpenChat: () => void;
  onOpenMemory: () => void;
  /** Where the code map is turned on; offered while it is off. */
  onOpenSettings?: () => void;
  /** Goes to another screen: the project's settings sheet, from its tabs. */
  onNavigate?: (view: View) => void;
}) {
  const api = useApi();
  const t = useT();
  const language = useLanguage();
  const status = useQuery({ queryKey: ["status", project], queryFn: () => api.projectStatus(project) });
  const usage = useQuery({ queryKey: ["projectUsage", project], queryFn: () => api.projectUsage(project, 7) });
  const commits = useQuery({ queryKey: ["commits", project], queryFn: () => api.recentCommits(project) });
  const tasks = useTasks(project);
  const board = useActivity().data;
  const info = overview.projects.find((p) => p.path === project);
  const name = info?.name ?? shortPath(project);
  const list = tasks.data ?? [];
  const current = list.find(isOpen);
  const events = useTaskEvents(current?.id ?? null);
  const history = events.data ?? [];
  const mascot = useMascot(current, history);
  const account = overview.accounts.find((a) => a.id === (current?.account ?? info?.account));
  const color = account?.color ?? (info ? projectColor(info, overview.accounts) : DEFAULT_ACCOUNT_COLOR);
  const agents = (status.data?.agents ?? [])
    .map((id) => overview.accounts.find((a) => a.id === id))
    .filter((a): a is Account => !!a);
  // What failed to load, said once: one alert for the screen (each box below
  // keeps its title, its reason and its Reintentar, without announcing itself).
  const failures = [
    status.isError && t("status.loadError"),
    tasks.isError && t("status.tasks.error"),
    usage.isError && t("status.week.error"),
    commits.isError && t("status.commits.error"),
  ].filter(Boolean);
  // The project's tabs: Tareas is its home, the others their own screens.
  const onTab = (tab: ProjectTab) => {
    if (tab === "tasks") onOpenChat();
    else if (tab === "memory") onOpenMemory();
    else if (tab === "settings") {
      if (onNavigate) onNavigate({ screen: "projectSettings", project });
      // App's route for the sheet (`#ps=`), when no navigation is handed in.
      else window.location.hash = `#ps=${encodeURIComponent(project)}`;
    }
  };

  return (
    <div className="screen status">
      <ProjectHead name={name} path={project} current="summary" onTab={onTab} />
      <section className="card status__hero" aria-label={t("status.title")}>
        <Mascot color={color} state={mascot.state} activity={mascot.activity} size={120} label="" />
        <div className="status__hero-text">
          <p className="status__eyebrow">{t("status.title")}</p>
          {/* Only from tasks that loaded: a failed list is not "no tasks". */}
          {tasks.isSuccess && (
            <p className="status__headline">{headline(current, agentName(history), overview.accounts, t)}</p>
          )}
          {mascot.activity && <WorkingWord activity={mascot.activity} className="working-word--status" />}
          <div className="actions">
            <Button small variant="primary" onClick={onOpenChat}>
              {t("status.openChat")}
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
          {info?.client && <span className="chip chip--muted">{t("status.client", { client: info.client })}</span>}
          {status.data && !status.data.codeMap && onOpenSettings && (
            <Button small variant="ghost" className="status__hint" onClick={onOpenSettings}>
              {t("status.codeMap.enable")}
            </Button>
          )}
        </div>
      </section>

      <OwnerBanner project={project} />

      <div role="alert" className="sr-only">
        {failures.join(" ")}
      </div>

      {status.isError && (
        <EmptyState
          error
          announce={false}
          title={t("status.loadError")}
          text={errorMessage(status.error, t)}
          onRetry={() => void status.refetch()}
        />
      )}
      {status.data && (
        <div className="status__stats">
          <Stat
            label={t("status.tests.label")}
            tone={status.data.lastTests ? (status.data.lastTests.passed ? "ok" : "danger") : "muted"}
            value={
              status.data.lastTests
                ? status.data.lastTests.passed
                  ? t("status.tests.passed")
                  : t("status.tests.failed")
                : t("status.tests.noData")
            }
            detail={
              status.data.lastTests
                ? `${timeAgo(status.data.lastTests.at, t)} · ${status.data.lastTests.command}`
                : t("status.tests.none")
            }
          />
          <Stat
            label={t("common.models")}
            tone={agents.length > 0 ? "ok" : "warn"}
            value={t(agents.length === 1 ? "status.models.active.one" : "status.models.active.other", { n: agents.length })}
            detail={agents.map((a) => `${a.name} ${usageLabel(a, language)}`).join(" · ") || t("status.models.none")}
          />
          <ChangesStat branch={status.data.branch} uncommitted={status.data.uncommitted} />
          <Stat
            label={t("common.memory")}
            tone={status.data.memoryStale + status.data.memoryProposed > 0 ? "warn" : "ok"}
            value={t(status.data.memoryNotes === 1 ? "status.memory.notes.one" : "status.memory.notes.other", {
              n: status.data.memoryNotes,
            })}
            detail={
              [
                status.data.memoryStale > 0 && t("status.memory.stale", { n: status.data.memoryStale }),
                status.data.memoryProposed > 0 &&
                  t(status.data.memoryProposed === 1 ? "status.memory.proposed.one" : "status.memory.proposed.other", {
                    n: status.data.memoryProposed,
                  }),
              ]
                .filter(Boolean)
                .join(" · ") || t("status.memory.upToDate")
            }
          />
        </div>
      )}

      <div className="status__layout">
        <section className="card status__tasks" aria-labelledby="status-tasks">
          <h2 id="status-tasks">{t("status.tasks")}</h2>
          {tasks.isError ? (
            <EmptyState
              error
              announce={false}
              title={t("status.tasks.error")}
              text={errorMessage(tasks.error, t)}
              onRetry={() => void tasks.refetch()}
            />
          ) : (
            tasks.isSuccess &&
            list.length === 0 && (
              <EmptyState
                title={t("status.tasks.empty")}
                actions={
                  <Button variant="primary" onClick={onOpenChat}>
                    {t("status.tasks.new")}
                  </Button>
                }
              />
            )
          )}
          <ul className="status__task-list">
            {list.slice(0, 6).map((task) => (
              <TaskRow key={task.id} task={task} board={board} />
            ))}
          </ul>
        </section>
        <div className="status__side">
          <WeekChart
            name={name}
            points={usage.data ?? null}
            error={usage.isError ? errorMessage(usage.error, t) : null}
            onRetry={() => void usage.refetch()}
          />
          <section className="card" aria-labelledby="status-commits">
            <h2 id="status-commits">{t("common.recentCommits")}</h2>
            {commits.isError && (
              <EmptyState
                error
                announce={false}
                title={t("status.commits.error")}
                text={errorMessage(commits.error, t)}
                onRetry={() => void commits.refetch()}
              />
            )}
            {commits.data?.length === 0 && <p className="muted small">{t("status.commits.empty")}</p>}
            <ul className="status__commits">
              {commits.data?.map((c) => (
                <li key={c.hash}>
                  <span className="mono muted">{c.hash}</span>
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

/** What a stat's dot says: fine, broken, worth a look, or nothing to judge. */
type StatTone = "ok" | "danger" | "warn" | "muted";

/** One indicator: the value in ink, its meaning in a small colored dot. */
function Stat({ label, value, detail, tone }: { label: string; value: string; detail: string; tone: StatTone }) {
  return (
    <section className="card stat" aria-label={label}>
      <h2 className="stat__label">
        <span className={`stat__dot stat__dot--${tone}`} aria-hidden />
        {label}
      </h2>
      <p className="stat__value">{value}</p>
      <p className="small muted">{detail}</p>
    </section>
  );
}

/** Work not yet committed on the project's branch. */
function ChangesStat({ branch, uncommitted }: { branch: string | null; uncommitted: number | null }) {
  const t = useT();
  if (branch === null || uncommitted === null) {
    return (
      <Stat
        label={t("status.changes")}
        tone="muted"
        value={t("status.changes.noRepo")}
        detail={t("status.changes.noRepoDetail")}
      />
    );
  }
  return (
    <Stat
      label={t("status.changes")}
      tone={uncommitted > 0 ? "warn" : "ok"}
      value={
        uncommitted > 0
          ? t(uncommitted === 1 ? "status.changes.pending.one" : "status.changes.pending.other", { n: uncommitted })
          : t("status.changes.clean")
      }
      detail={t("common.branch", { branch })}
    />
  );
}

/** One task: its title, what it is doing and the one state pill used everywhere. */
function TaskRow({ task, board }: { task: Task; board: ActivityItem[] | undefined }) {
  const t = useT();
  const events = useTaskEvents(task.id);
  const history = events.data ?? [];
  const mascot = useMascot(task, history);
  const state = taskState(task, board?.find((item) => item.task.id === task.id), history);
  const steps = [...history].reverse().find((e) => e.kind === "steps");
  const done = steps?.kind === "steps" ? steps.steps.filter((s) => s.status === "done").length : 0;
  const agent = agentName(history);
  const detail =
    task.status === "running" && steps?.kind === "steps"
      ? t("status.task.steps", { agent, done, total: steps.steps.length })
      : task.status === "waitingApproval"
        ? t("status.task.needsYou", { agent: agent.split(" ")[0] ?? agent })
        : `${agent} · ${timeAgo(task.createdAt, t)}`;
  return (
    <li className="status__task">
      <div className="status__task-text">
        <strong>{task.title}</strong>
        <span className="small muted">
          {detail}
          {mascot.activity && (
            <>
              <span aria-hidden> · </span>
              <WorkingWord activity={mascot.activity} />
            </>
          )}
        </span>
      </div>
      <StatusPill state={state} task={task} />
    </li>
  );
}

function WeekChart({
  name,
  points,
  error,
  onRetry,
}: {
  name: string;
  /** `null` while the week is still loading. */
  points: UsagePoint[] | null;
  error: string | null;
  onRetry: () => void;
}) {
  const t = useT();
  const language = useLanguage();
  const letters = t("status.week.letters");
  const loading = points === null && !error;
  const days = usageByDay(points ?? []);
  const max = Math.max(...days.map((d) => d.tokens), 1);
  const busiest = days.reduce((a, b) => (b.tokens > a.tokens ? b : a));
  const total = days.reduce((sum, d) => sum + d.tokens, 0);
  return (
    <section className="card" aria-labelledby="status-week">
      <h2 id="status-week">{t("status.week.title", { name })}</h2>
      {error ? (
        <EmptyState error announce={false} title={t("status.week.error")} text={error} onRetry={onRetry} />
      ) : loading ? (
        <p className="small muted" role="status">
          {t("status.week.loading")}
        </p>
      ) : (
        <div className="week-chart" role="img" aria-label={t("status.week.chart", { total: formatTokens(total) })}>
          {days.map((d) => {
            const label = `${dayName(d.date, language)} ${d.date.getDate()}: ${formatTokens(d.tokens)} tokens`;
            const height = d.tokens === 0 ? 0 : Math.max(4, (d.tokens / max) * 100);
            const top = d.tokens > 0 && d === busiest;
            return (
              <div key={d.date.toISOString()} className="week-chart__day" title={label}>
                <div className="week-chart__track">
                  <div
                    className={`week-chart__bar${d.tokens === 0 ? " week-chart__bar--empty" : ""}${top ? " week-chart__bar--top" : ""}`}
                    style={{ height: `${height}%` }}
                  />
                  {/* The value over its bar, in the bars' quiet ink. */}
                  {d.tokens > 0 && (
                    <span className="week-chart__value" aria-hidden style={{ bottom: `${height}%` }}>
                      {formatTokens(d.tokens)}
                    </span>
                  )}
                </div>
                <span className="week-chart__letter" aria-hidden>
                  {letters[d.date.getDay()]}
                </span>
              </div>
            );
          })}
        </div>
      )}
      {!error && !loading && (
        <p className="small muted">
          {total === 0
            ? t("common.noUsageThisWeek")
            : t("status.week.busiest", { day: dayName(busiest.date, language), total: formatTokens(total) })}
        </p>
      )}
      {!error && !loading && (
        <table className="sr-only">
          <caption>{t("status.week.caption")}</caption>
          <tbody>
            {days.map((d) => (
              <tr key={d.date.toISOString()}>
                <th scope="row">{`${dayName(d.date, language)} ${d.date.getDate()}`}</th>
                <td>{d.tokens}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}
