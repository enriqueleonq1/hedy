import { useQuery } from "@tanstack/react-query";

import { errorMessage, useApi } from "../api/context";
import type { Account, Overview, Project, UsageSummary } from "../api/types";
import { AccountChip, UsageBars } from "../components/accounts";
import { headlineWindow, resetLabel, sharedWindows, windowLabel } from "../components/planUsage";
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
function weekUsageText(summary: UsageSummary, t: Translate, language: Language): string {
  const tokens = summary.projects.reduce((sum, p) => sum + p.tokens, 0);
  if (tokens === 0) return t("welcome.usage.empty");
  const costs = summary.projects.map((p) => p.costUsd).filter((c): c is number => c !== null);
  const cost =
    costs.length > 0
      ? t("welcome.usage.cost", { cost: formatUsd(costs.reduce((sum, c) => sum + c, 0), language) })
      : "";
  const busiest = summary.projects[0];
  const where = busiest && summary.projects.length > 1 ? t("welcome.usage.busiest", { name: busiest.name }) : "";
  return t("welcome.usage.total", { tokens: formatTokens(tokens), cost, busiest: where });
}

/** "Claude trabajo: 39% de la sesión (5 h) · 30% de esta semana", from the provider's own reading. */
function planLine(account: Account, t: Translate): string | null {
  const usage = account.usage.plan.usage;
  if (!usage) return null;
  const windows = sharedWindows(usage);
  if (windows.length === 0) return null;
  const parts = windows.map((w) => t("welcome.plan.window", { percent: Math.round(w.percent), label: windowLabel(w, t).toLowerCase() }));
  return t("welcome.plan.line", { name: account.name, windows: parts.join(" · ") });
}

function useWeekSummary() {
  const api = useApi();
  return useQuery({ queryKey: ["usageSummary", 7], queryFn: () => api.usageSummary(7) });
}

/** The plan's real figures first (they include what is done outside Hedy), then Hedy's own count. */
function WeekUsage({ accounts }: { accounts: Account[] }) {
  const t = useT();
  const language = useLanguage();
  const summary = useWeekSummary();
  const lines = accounts.map((a) => planLine(a, t)).filter((l): l is string => l !== null);
  const hedyTokens = summary.data?.projects.reduce((sum, p) => sum + p.tokens, 0) ?? 0;

  if (summary.isError && lines.length === 0) return <p className="error-text">{errorMessage(summary.error, t)}</p>;
  if (lines.length === 0) {
    if (!summary.data) return <p>{t("welcome.usage.loading")}</p>;
    return <p>{hedyTokens > 0 ? weekUsageText(summary.data, t, language) : t("welcome.usage.empty")}</p>;
  }
  return (
    <>
      {lines.map((line) => (
        <p key={line}>{line}</p>
      ))}
      {summary.data && hedyTokens > 0 && <p className="small">{weekUsageText(summary.data, t, language)}</p>}
    </>
  );
}

type Notice = { key: string; tone: "warn" | "danger" | "info"; text: string; action?: { label: string; go: () => void }; fix?: string };

/** What deserves a look right away: tasks waiting for the user, plans near their limit, setup problems. */
function ForYou({ overview, onNavigate }: { overview: Overview; onNavigate: (v: View) => void }) {
  const api = useApi();
  const t = useT();
  const language = useLanguage();
  const board = useActivity().data ?? [];
  const doctor = useQuery({ queryKey: ["doctor"], queryFn: () => api.doctor(), staleTime: 10 * 60_000 }).data;

  const notices: Notice[] = [];
  const waiting = board.filter((item) => needsYou(item.state)).length;
  if (waiting > 0) {
    notices.push({
      key: "waiting",
      tone: "warn",
      text: waiting === 1 ? t("welcome.forYou.waiting.one") : t("welcome.forYou.waiting.other", { n: waiting }),
      action: { label: t("welcome.forYou.open"), go: () => onNavigate({ screen: "activity" }) },
    });
  }
  const review = board.filter((item) => item.state === "review").length;
  if (review > 0) {
    notices.push({
      key: "review",
      tone: "info",
      text: review === 1 ? t("welcome.forYou.review.one") : t("welcome.forYou.review.other", { n: review }),
      action: { label: t("welcome.forYou.open"), go: () => onNavigate({ screen: "activity" }) },
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
    });
  }
  if (doctor && doctor.errors + doctor.warnings > 0) {
    const first = doctor.checks.find((c) => c.level === "error") ?? doctor.checks.find((c) => c.level === "warning");
    notices.push({
      key: "doctor",
      tone: doctor.errors > 0 ? "danger" : "warn",
      text: t("welcome.forYou.doctor", {
        problems:
          doctor.errors + doctor.warnings === 1
            ? t("welcome.forYou.problems.one")
            : t("welcome.forYou.problems.other", { n: doctor.errors + doctor.warnings }),
        first: first ? `${first.subject}: ${first.message}` : "",
      }),
      fix: first?.fix ?? undefined,
    });
  }

  return (
    <div className="for-you" aria-label={t("welcome.forYou.title")}>
      {notices.length === 0 ? (
        <p className="for-you__calm">{t("welcome.forYou.calm")}</p>
      ) : (
        <ul className="for-you__list">
          {notices.map((n) => (
            <li key={n.key} className={`for-you__item for-you__item--${n.tone}`}>
              <span className="for-you__text">
                {n.text}
                {n.fix && <code className="for-you__fix">{n.fix}</code>}
              </span>
              {n.action && (
                <Button small variant="ghost" onClick={n.action.go}>
                  {n.action.label}
                </Button>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/** The latest tasks of every project; a row opens the task. */
function RecentTasks({ onNavigate }: { onNavigate: (v: View) => void }) {
  const t = useT();
  const items = (useActivity().data ?? []).slice().sort((a, b) => b.updatedAt - a.updatedAt).slice(0, 5);
  if (items.length === 0) return null;
  return (
    <>
      <h3 className="week-stats__sub">{t("welcome.recent.title")}</h3>
      <ul className="recent-tasks">
        {items.map((item) => (
          <li key={item.task.id}>
            <button
              type="button"
              className="recent-tasks__row"
              onClick={() => onNavigate({ screen: "workspace", project: item.task.project, task: item.task.id })}
            >
              <span className="recent-tasks__title">{item.task.title}</span>
              <span className="muted small">
                {item.projectName} · {t(STATE_LABEL[item.state])} · {timeAgo(item.updatedAt, t)}
              </span>
            </button>
          </li>
        ))}
      </ul>
    </>
  );
}

/** General figures for the week and one card per account; shown on the home screen. */
function WeekStats({ overview, onNavigate }: { overview: Overview; onNavigate: (v: View) => void }) {
  const t = useT();
  const language = useLanguage();
  const summary = useWeekSummary().data;
  const { accounts } = overview;

  const top = accounts
    .map((a) => ({ account: a, window: headlineWindow(a) }))
    .filter((x) => x.window !== null)
    .sort((a, b) => (b.window?.percent ?? 0) - (a.window?.percent ?? 0))[0];
  const tokens = summary?.projects.reduce((sum, p) => sum + p.tokens, 0) ?? 0;
  const costs = summary?.projects.map((p) => p.costUsd).filter((c): c is number => c !== null) ?? [];
  const busiest = summary?.projects.find((p) => p.tokens > 0);
  const connected = accounts.filter((a) => a.login.state === "loggedIn").length;
  const dash = "—";

  const tiles = [
    {
      label: t("welcome.stats.topPlan"),
      value: top?.window ? `${Math.round(top.window.percent)}%` : dash,
      note: top?.window
        ? `${top.account.name} · ${windowLabel(top.window, t).toLowerCase()}${
            resetLabel(top.window, t, language) ? ` · ${resetLabel(top.window, t, language)}` : ""
          }`
        : t("welcome.stats.topPlanNone"),
    },
    {
      label: t("welcome.stats.accounts"),
      value: `${connected}/${accounts.length}`,
      note: t("welcome.stats.accountsNote"),
    },
    {
      label: t("welcome.stats.hedyTokens"),
      value: tokens > 0 ? formatTokens(tokens) : dash,
      note:
        costs.length > 0
          ? t("welcome.stats.cost", { cost: formatUsd(costs.reduce((sum, c) => sum + c, 0), language) })
          : t("welcome.stats.hedyTokensNote"),
    },
    {
      label: t("welcome.stats.busiest"),
      value: busiest?.name ?? dash,
      note: busiest ? t("welcome.stats.busiestNote", { tokens: formatTokens(busiest.tokens) }) : t("welcome.stats.busiestNone"),
    },
  ];

  return (
    <section className="week-stats" aria-labelledby="week-stats-title">
      <h2 id="week-stats-title">{t("welcome.stats.title")}</h2>
      <ForYou overview={overview} onNavigate={onNavigate} />
      <div className="stat-tiles">
        {tiles.map((tile) => (
          <div key={tile.label} className="stat-tile">
            <span className="stat-tile__label">{tile.label}</span>
            <strong className="stat-tile__value">{tile.value}</strong>
            <span className="muted small">{tile.note}</span>
          </div>
        ))}
      </div>
      <h3 className="week-stats__sub">{t("welcome.stats.byAccount")}</h3>
      <div className="account-usage-grid">
        {accounts.map((account) => (
          <article key={account.id} className="card account-usage" aria-label={account.name}>
            <header className="row row--between">
              <AccountChip account={account} />
              <span className="muted small">{account.providerName}</span>
            </header>
            <UsageBars account={account} />
            <footer className="row row--between">
              <span className="muted small">
                {account.projects.length === 1
                  ? t("welcome.stats.projects.one")
                  : t("welcome.stats.projects.other", { n: account.projects.length })}
              </span>
              <Button small variant="ghost" onClick={() => onNavigate({ screen: "accounts" })}>
                {t("welcome.stats.seeAccount")}
              </Button>
            </footer>
          </article>
        ))}
      </div>
      <RecentTasks onNavigate={onNavigate} />
      {summary?.clients.some((c) => c.client !== null) && <SpendByClient />}
    </section>
  );
}

export function Welcome({ overview, onNavigate }: { overview: Overview; onNavigate: (v: View) => void }) {
  const t = useT();
  const adding = useAddProject();
  const color = overview.accounts.find((a) => a.id === overview.defaultAccount)?.color ?? DEFAULT_ACCOUNT_COLOR;
  const hasAccounts = overview.accounts.length > 0;
  const greeting = overview.userName ? t("welcome.greetingNamed", { name: overview.userName }) : t("welcome.greeting");

  return (
    <div className="screen welcome">
      <section className="hero">
        <Mascot color={color} state={hasAccounts ? "done" : "sleeping"} size={300} label={t("welcome.mascot")} />
        <div className="hero__text">
          <h1>
            {greeting}
            <br />
            <span className="hero__question">{t("welcome.question")}</span>
          </h1>
          <div className="balloon">
            <strong>{t("welcome.weekUsage")}</strong>
            <WeekUsage accounts={overview.accounts} />
          </div>
          {!hasAccounts && (
            <Button variant="primary" onClick={() => onNavigate({ screen: "accounts" })}>
              {t("welcome.addFirstAccount")}
            </Button>
          )}
        </div>
      </section>

      {hasAccounts && <WeekStats overview={overview} onNavigate={onNavigate} />}

      <section aria-labelledby="projects-title">
        <div className="section-head">
          <h2 id="projects-title">{t("common.projects")}</h2>
          <Button variant="primary" onClick={adding.start} disabled={!hasAccounts}>
            {t("common.addProjectButton")}
          </Button>
        </div>
        {overview.projects.length === 0 ? (
          <p className="empty">{hasAccounts ? t("welcome.empty.hasAccounts") : t("welcome.empty.noAccounts")}</p>
        ) : (
          <div className="project-grid">
            {overview.projects.map((p) => (
              <ProjectCard key={p.path} project={p} overview={overview} onOpen={() => onNavigate({ screen: "workspace", project: p.path })} />
            ))}
          </div>
        )}
      </section>
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
