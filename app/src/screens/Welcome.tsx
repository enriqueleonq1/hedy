import { useQuery } from "@tanstack/react-query";

import { useApi } from "../api/context";
import type { Account, Overview, Project, UsageSummary, UsageWindow } from "../api/types";
import { AccountChip } from "../components/accounts";
import {
  barColor,
  formatBalance,
  headlineWindow,
  resetLabel,
  sharedWindows,
  usageLevel,
  windowLabel,
} from "../components/planUsage";
import type { View } from "../components/Layout";
import { Mascot } from "../components/Mascot/Mascot";
import { projectColor, shortPath, useProjectActions } from "../components/projects";
import { Button } from "../components/ui/Button";
import { formatTokens, formatUsd, timeAgo } from "../format";
import { needsYou, useActivity } from "../tasks/hooks";
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

  let headline = "—";
  let detail: string;
  let tone = "muted";
  if (top) {
    headline = `${Math.round(top.percent)}%`;
    const level = usageLevel(top.percent);
    tone = level === "ok" ? "ink" : level;
    const resets = resetLabel(top, t, language);
    detail = `${windowLabel(top, t)}${resets ? ` · ${resets}` : ""}`;
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
  const label = windows.length
    ? windows.map((w) => `${windowLabel(w, t)} ${Math.round(w.percent)}%`).join(", ")
    : detail;

  return (
    <button type="button" className="plan-card" onClick={onOpen} aria-label={`${account.name}: ${label}`}>
      <PlanRings account={account} outer={outer} inner={inner} />
      <span className="plan-card__text">
        <AccountChip account={account} />
        <strong className={`plan-card__value plan-card__value--${tone}`}>{headline}</strong>
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
      go: () => onNavigate({ screen: "accounts" }),
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
            <span className={`small recent-tasks__meta recent-tasks__meta--${needsYou(item.state) ? "needs" : item.state}`}>
              {item.projectName} · {t(STATE_LABEL[item.state])} · {timeAgo(item.updatedAt, t)}
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
          <div className="plan-cards">
            {overview.accounts.map((account) => (
              <PlanCard key={account.id} account={account} onOpen={() => onNavigate({ screen: "accounts" })} />
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
                <ProjectCard
                  key={p.path}
                  project={p}
                  overview={overview}
                  onOpen={() => onNavigate({ screen: "workspace", project: p.path })}
                />
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

function ProjectCard({ project, overview, onOpen }: { project: Project; overview: Overview; onOpen: () => void }) {
  const t = useT();
  const actions = useProjectActions();
  const account = overview.accounts.find((a) => a.id === project.account);
  const runnable = account?.kind === "subscription";

  return (
    <article className="card project-card" style={{ borderTopColor: projectColor(project, overview.accounts) }}>
      <header>
        <h3>{project.name}</h3>
        {project.protected && <span className="badge badge--warn">{t("common.protected")}</span>}
      </header>
      <p className="mono muted">{shortPath(project.path)}</p>
      {account ? <AccountChip account={account} /> : <span className="muted">{t("common.noAccount")}</span>}
      <div className="actions">
        <Button small variant="primary" onClick={onOpen}>
          {t("welcome.card.workHere")}
        </Button>
        <Button small disabled={!runnable} onClick={() => actions.openCli(project)}>
          {t("welcome.card.cliInTerminal", { cli: account?.providerName ?? "CLI" })}
        </Button>
        <Button small onClick={() => actions.openShell(project)}>
          {t("common.terminal")}
        </Button>
        <Button small variant="ghost" onClick={() => actions.openEditor(project)}>
          {t("common.openInVsCode")}
        </Button>
      </div>
    </article>
  );
}
