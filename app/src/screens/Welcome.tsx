import { useQuery } from "@tanstack/react-query";
import type { CSSProperties } from "react";

import { useApi } from "../api/context";
import type { Account, ActivityItem, Overview, Project, UsageSummary, UsageWindow } from "../api/types";
import { AccountChip } from "../components/accounts";
import {
  barColor,
  formatBalance,
  headlineWindow,
  resetLabel,
  sharedWindows,
  shortWindowLabel,
  usageLevel,
  windowLabel,
  windowText,
} from "../components/planUsage";
import type { View } from "../components/Layout";
import { Mascot } from "../components/Mascot/Mascot";
import { projectColor, shortPath, useProjectActions } from "../components/projects";
import { Button } from "../components/ui/Button";
import { MenuButton } from "../components/ui/Menu";
import { formatTokens, formatUsd, timeAgo } from "../format";
import { needsYou, useActivity } from "../tasks/hooks";
import { useInbox } from "../tasks/inbox";
import { STATE_LABEL } from "./activity/ActivityScreen";
import { SpendByClient } from "./accounts/SpendByClient";
import { useLanguage, useT, type Language, type Translate } from "../i18n";
import { DEFAULT_ACCOUNT_COLOR } from "../theme/tokens";
import { AddProjectDialog, useAddProject } from "./accounts/AddProjectDialog";

/** Hedy's one-liner about the week: tokens, estimated cost and the busiest project. */
function weekUsageText(summary: UsageSummary, t: Translate, language: Language): string | null {
  const tokens = summary.projects.reduce((sum, p) => sum + p.tokens, 0);
  if (tokens === 0) return null;
  const costs = summary.projects.map((p) => p.costUsd).filter((c): c is number => c !== null);
  const cost =
    costs.length > 0
      ? t("welcome.usage.cost", { cost: formatUsd(costs.reduce((sum, c) => sum + c, 0), language) })
      : "";
  const busiest = summary.projects[0];
  const where = busiest && summary.projects.length > 1 ? t("welcome.usage.busiest", { name: busiest.name }) : "";
  return t("welcome.usage.total", { tokens: formatTokens(tokens), cost, busiest: where });
}

/** One sentence under the greeting: where there is room, and what is close to its limit. */
function planSummary(accounts: Account[], t: Translate): string {
  const read = accounts
    .map((account) => ({ account, w: headlineWindow(account) }))
    .filter((x): x is { account: Account; w: UsageWindow } => x.w !== null);
  if (read.length === 0) return t("welcome.question");
  const high = read.filter((x) => x.w.percent >= 80).sort((a, b) => b.w.percent - a.w.percent)[0];
  const room = read.filter((x) => x.w.percent < 60).sort((a, b) => a.w.percent - b.w.percent)[0];
  const parts: string[] = [];
  if (room) parts.push(t("welcome.summary.room", { name: room.account.name }));
  if (high) {
    parts.push(
      t("welcome.summary.high", {
        name: high.account.name,
        percent: Math.round(high.w.percent),
        label: windowLabel(high.w, t).toLowerCase(),
      }),
    );
  }
  return parts.length > 0 ? parts.join(" ") : t("welcome.summary.steady");
}

const OUTER = 40;
const INNER = 26;
const dash = (r: number, percent: number) => {
  const length = 2 * Math.PI * r;
  return `${((length * Math.min(percent, 100)) / 100).toFixed(1)} ${length.toFixed(1)}`;
};

/** Two rings like the Activity app: the week outside, the 5-hour session inside. */
function PlanRings({ account, outer, inner }: { account: Account; outer?: UsageWindow; inner?: UsageWindow }) {
  const ring = (r: number, w: UsageWindow | undefined, opacity: number) => (
    <>
      <circle cx="48" cy="48" r={r} className="rings__track" />
      {w && w.percent > 0 && (
        <circle
          cx="48"
          cy="48"
          r={r}
          className="rings__fill"
          stroke={barColor(w.percent, account.color)}
          strokeDasharray={dash(r, w.percent)}
          opacity={opacity}
          transform="rotate(-90 48 48)"
        />
      )}
    </>
  );
  return (
    <svg className="rings" width="96" height="96" viewBox="0 0 96 96" aria-hidden="true">
      {ring(OUTER, outer, 1)}
      {ring(INNER, inner, 0.75)}
    </svg>
  );
}

function PlanCard({ account, onOpen }: { account: Account; onOpen: () => void }) {
  const t = useT();
  const language = useLanguage();
  const usage = account.usage.plan.usage;
  const windows = usage ? sharedWindows(usage) : [];
  const inner = windows.find((w) => w.kind === "session") ?? (windows.length > 1 ? windows[0] : undefined);
  const outer = windows.find((w) => w !== inner);
  const top = headlineWindow(account);

  // The headline names its window ("Semana · 83%"), like the top bar and the sidebar.
  let kind: string | null = null;
  let headline = "—";
  let detail: string;
  let tone = "muted";
  if (top) {
    kind = shortWindowLabel(top, t);
    headline = `${Math.round(top.percent)}%`;
    const level = usageLevel(top.percent);
    tone = level === "ok" ? "ink" : level;
    const others = windows.filter((w) => w !== top).map((w) => windowText(w, t));
    detail = [resetLabel(top, t, language), ...others].filter(Boolean).join(" · ");
  } else if (usage?.balance) {
    headline = formatBalance(usage.balance, language);
    detail = t("accounts.plan.balance");
  } else if (account.kind === "apiKey") {
    detail = t("welcome.plans.apiKey");
  } else {
    detail = account.usage.plan.supported
      ? t("accounts.plan.checking")
      : t("accounts.plan.notReported", { provider: account.providerName });
  }
  const label = windows.length ? windows.map((w) => windowText(w, t)).join(", ") : detail;

  return (
    <button type="button" className="plan-usage" onClick={onOpen} aria-label={`${account.name}: ${label}`}>
      <PlanRings account={account} outer={outer} inner={inner} />
      <span className="plan-usage__text">
        <AccountChip account={account} />
        <strong className={`plan-usage__value plan-usage__value--${tone}`}>
          {kind && <span className="plan-usage__kind">{kind} · </span>}
          {headline}
        </strong>
        <span className="muted small">{detail}</span>
      </span>
    </button>
  );
}

type Notice = { key: string; tone: "warn" | "danger" | "info"; text: string; go?: () => void };

/** What deserves a look right away, as pills: tasks waiting, plans near their limit, setup problems. */
function ForYou({ overview, onNavigate }: { overview: Overview; onNavigate: (v: View) => void }) {
  const api = useApi();
  const t = useT();
  const language = useLanguage();
  const board = useActivity().data ?? [];
  const inbox = useInbox();
  const doctor = useQuery({ queryKey: ["doctor"], queryFn: () => api.doctor(), staleTime: 10 * 60_000 }).data;
  const toBoard = () => onNavigate({ screen: "activity" });

  const notices: Notice[] = [];
  const waiting = board.filter((item) => needsYou(item.state)).length;
  if (waiting > 0) {
    notices.push({
      key: "waiting",
      tone: "warn",
      text: waiting === 1 ? t("welcome.forYou.waiting.one") : t("welcome.forYou.waiting.other", { n: waiting }),
      go: toBoard,
    });
  }
  // The same inbox as Activity's "Te necesitan": proposals count too.
  const proposals = inbox.items.filter((item) => item.kind !== "task").length;
  if (proposals > 0) {
    notices.push({
      key: "proposals",
      tone: "info",
      text: proposals === 1 ? t("welcome.forYou.proposals.one") : t("welcome.forYou.proposals.other", { n: proposals }),
      go: toBoard,
    });
  }
  const review = board.filter((item) => item.state === "review").length;
  if (review > 0) {
    notices.push({
      key: "review",
      tone: "info",
      text: review === 1 ? t("welcome.forYou.review.one") : t("welcome.forYou.review.other", { n: review }),
      go: toBoard,
    });
  }
  for (const account of overview.accounts) {
    const w = headlineWindow(account);
    if (!w || w.percent < 80) continue;
    const resets = resetLabel(w, t, language);
    notices.push({
      key: `limit-${account.id}`,
      tone: w.percent >= 95 ? "danger" : "warn",
      text: t("welcome.forYou.limit", {
        name: account.name,
        percent: Math.round(w.percent),
        label: windowLabel(w, t).toLowerCase(),
        resets: resets ? ` · ${resets}` : "",
      }),
      go: () => onNavigate({ screen: "accounts", account: account.id }),
    });
  }
  if (doctor && doctor.errors + doctor.warnings > 0) {
    const first = doctor.checks.find((c) => c.level === "error") ?? doctor.checks.find((c) => c.level === "warning");
    notices.push({
      key: "doctor",
      tone: doctor.errors > 0 ? "danger" : "info",
      text: first
        ? t("welcome.forYou.doctorOne", { subject: first.subject, message: first.message })
        : t("welcome.forYou.doctor", { n: doctor.errors + doctor.warnings }),
    });
  }

  if (notices.length === 0) return <p className="for-you__calm">{t("welcome.forYou.calm")}</p>;
  return (
    <ul className="pills-row" aria-label={t("welcome.forYou.title")}>
      {notices.map((n) => (
        <li key={n.key}>
          {n.go ? (
            <button type="button" className={`notice-pill notice-pill--${n.tone}`} onClick={n.go}>
              {n.text}
            </button>
          ) : (
            <span className={`notice-pill notice-pill--${n.tone}`}>{n.text}</span>
          )}
        </li>
      ))}
    </ul>
  );
}

/** The latest tasks of every project; a row opens the task. */
function RecentTasks({ onNavigate }: { onNavigate: (v: View) => void }) {
  const t = useT();
  const items = (useActivity().data ?? []).slice().sort((a, b) => b.updatedAt - a.updatedAt).slice(0, 5);
  if (items.length === 0) return <p className="empty">{t("welcome.recent.empty")}</p>;
  return (
    <ul className="recent-tasks">
      {items.map((item) => (
        <li key={item.task.id}>
          <button
            type="button"
            className="recent-tasks__row"
            onClick={() => onNavigate({ screen: "workspace", project: item.task.project, task: item.task.id })}
          >
            <span className="recent-tasks__title">{item.task.title}</span>
            <span className="small recent-tasks__meta">
              {item.projectName} ·{" "}
              <span className={`recent-tasks__state recent-tasks__state--${needsYou(item.state) ? "needs" : item.state}`}>
                {t(STATE_LABEL[item.state])}
              </span>{" "}
              · {timeAgo(item.updatedAt, t)}
            </span>
          </button>
        </li>
      ))}
    </ul>
  );
}

export function Welcome({ overview, onNavigate }: { overview: Overview; onNavigate: (v: View) => void }) {
  const api = useApi();
  const t = useT();
  const language = useLanguage();
  const adding = useAddProject();
  const board = useActivity().data ?? [];
  const summary = useQuery({ queryKey: ["usageSummary", 7], queryFn: () => api.usageSummary(7) }).data;
  const color = overview.accounts.find((a) => a.id === overview.defaultAccount)?.color ?? DEFAULT_ACCOUNT_COLOR;
  const hasAccounts = overview.accounts.length > 0;
  const greeting = overview.userName ? t("welcome.greetingNamed", { name: overview.userName }) : t("welcome.greeting");
  const today = new Intl.DateTimeFormat(language, { weekday: "long", day: "numeric", month: "long" }).format(new Date());

  // The project of the latest task, else the first one.
  const latest = board.slice().sort((a, b) => b.updatedAt - a.updatedAt)[0];
  const current = overview.projects.find((p) => p.path === latest?.task.project) ?? overview.projects[0];
  const hedyWeek = summary ? weekUsageText(summary, t, language) : null;

  return (
    <div className="screen welcome">
      <header className="today">
        <Mascot color={color} state={hasAccounts ? "idle" : "sleeping"} size={112} label={t("welcome.mascot")} />
        <div className="today__text">
          <p className="today__date">{today}</p>
          <h1>{greeting}</h1>
          <p className="today__summary">
            {hasAccounts ? planSummary(overview.accounts, t) : t("welcome.empty.noAccounts")}
          </p>
        </div>
        <div className="today__actions">
          {!hasAccounts ? (
            <Button variant="primary" onClick={() => onNavigate({ screen: "accounts" })}>
              {t("welcome.addFirstAccount")}
            </Button>
          ) : current ? (
            <Button variant="primary" onClick={() => onNavigate({ screen: "workspace", project: current.path })}>
              {t("welcome.continueIn", { name: current.name })}
            </Button>
          ) : null}
          {hasAccounts && <Button onClick={adding.start}>{t("common.addProjectButton")}</Button>}
        </div>
      </header>

      {hasAccounts && (
        <section className="today__section" aria-labelledby="plans-title">
          <div className="section-head">
            <h2 id="plans-title">{t("welcome.plans.title")}</h2>
            <span className="muted small">{t("welcome.plans.legend")}</span>
          </div>
          <div className="plan-usage-grid">
            {overview.accounts.map((account) => (
              <PlanCard
                key={account.id}
                account={account}
                onOpen={() => onNavigate({ screen: "accounts", account: account.id })}
              />
            ))}
          </div>
          {hedyWeek && <p className="muted small">{hedyWeek}</p>}
        </section>
      )}

      {hasAccounts && (
        <section className="today__section" aria-labelledby="for-you-title">
          <h2 id="for-you-title">{t("welcome.forYou.title")}</h2>
          <ForYou overview={overview} onNavigate={onNavigate} />
        </section>
      )}

      <div className="today__columns">
        <section className="today__section today__projects" aria-labelledby="projects-title">
          <h2 id="projects-title">{t("common.projects")}</h2>
          {overview.projects.length === 0 ? (
            <p className="empty">{hasAccounts ? t("welcome.empty.hasAccounts") : t("welcome.empty.noAccounts")}</p>
          ) : (
            <div className="project-grid">
              {overview.projects.map((p) => (
                <ProjectCard key={p.path} project={p} overview={overview} board={board} onNavigate={onNavigate} />
              ))}
            </div>
          )}
        </section>
        {hasAccounts && (
          <section className="today__section today__recent" aria-labelledby="recent-title">
            <h2 id="recent-title">{t("welcome.recent.title")}</h2>
            <RecentTasks onNavigate={onNavigate} />
          </section>
        )}
      </div>

      {summary?.clients.some((c) => c.client !== null) && <SpendByClient />}
      {adding.path && <AddProjectDialog overview={overview} path={adding.path} onClose={adding.close} />}
    </div>
  );
}

/** What a project's tasks are up to, from the task board: "1 tarea te espera · trabajando". */
export function projectLive(
  path: string,
  board: ActivityItem[],
  t: Translate,
  now = Date.now() / 1000,
): { text: string; tone: "needs" | "working" | "calm" } {
  const items = board.filter((item) => item.task.project === path);
  const waiting = items.filter((item) => needsYou(item.state)).length;
  const working = items.some((item) => item.state === "working");
  const parts: string[] = [];
  if (waiting > 0) {
    parts.push(waiting === 1 ? t("welcome.card.waiting.one") : t("welcome.card.waiting.other", { n: waiting }));
  }
  if (working) parts.push(t("welcome.card.working"));
  if (parts.length > 0) return { text: parts.join(" · "), tone: waiting > 0 ? "needs" : "working" };
  const last = items.reduce((max, item) => Math.max(max, item.updatedAt), 0);
  return {
    text: last > 0 ? t("welcome.card.lastActivity", { when: timeAgo(last, t, now) }) : t("welcome.card.quiet"),
    tone: "calm",
  };
}

function ProjectCard({
  project,
  overview,
  board,
  onNavigate,
}: {
  project: Project;
  overview: Overview;
  board: ActivityItem[];
  onNavigate: (v: View) => void;
}) {
  const t = useT();
  const actions = useProjectActions();
  const account = overview.accounts.find((a) => a.id === project.account);
  const runnable = account?.kind === "subscription";
  const live = projectLive(project.path, board, t);

  return (
    <article
      className="card project-card"
      aria-labelledby={`project-${project.path}`}
      style={{ "--project": projectColor(project, overview.accounts) } as CSSProperties}
    >
      <header>
        <h3 id={`project-${project.path}`}>{project.name}</h3>
        <span className="project-card__badges">
          {project.protected && <span className="badge badge--protected">{t("common.protected")}</span>}
          <MenuButton
            label={t("welcome.card.more", { name: project.name })}
            items={[
              {
                label: t("welcome.card.cliInTerminal", { cli: account?.providerName ?? "CLI" }),
                disabled: !runnable,
                onSelect: () => actions.openCli(project),
              },
              { label: t("common.terminal"), onSelect: () => actions.openShell(project) },
              { label: t("common.openInVsCode"), onSelect: () => void actions.openEditor(project) },
              { label: t("workspace.status"), onSelect: () => onNavigate({ screen: "status", project: project.path }) },
              { label: t("common.memory"), onSelect: () => onNavigate({ screen: "memory", project: project.path }) },
            ]}
          />
        </span>
      </header>
      <p className="mono muted">{shortPath(project.path)}</p>
      {account ? <AccountChip account={account} /> : <span className="muted">{t("common.noAccount")}</span>}
      <p className={`small project-card__live project-card__live--${live.tone}`}>{live.text}</p>
      <div className="actions">
        <Button small variant="primary" onClick={() => onNavigate({ screen: "workspace", project: project.path })}>
          {t("welcome.card.workHere")}
        </Button>
      </div>
    </article>
  );
}
