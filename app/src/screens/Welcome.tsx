import { useQuery } from "@tanstack/react-query";
import type { CSSProperties, ReactNode } from "react";

import { useApi } from "../api/context";
import type { Account, ActivityItem, Overview, Project, UsageSummary, UsageWindow } from "../api/types";
import { AccountChip } from "../components/accounts";
import {
  barColor,
  formatBalance,
  headlineWindow,
  levelClass,
  planCardState,
  resetLabel,
  sharedWindows,
  shortWindowLabel,
  usageLevel,
  usePlanRetry,
  windowLabel,
  windowText,
} from "../components/planUsage";
import { taskView, type View } from "../components/Layout";
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
import { useCreate } from "./create/CreateProvider";

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
/** The inner ring is drawn a little lighter; its swatch matches. */
const INNER_OPACITY = 0.75;
const dash = (r: number, percent: number) => {
  const length = 2 * Math.PI * r;
  return `${((length * Math.min(percent, 100)) / 100).toFixed(1)} ${length.toFixed(1)}`;
};

/** Rings like the Activity app: one per window, the second (when there is one) inside. */
function PlanRings({ account, outer, inner }: { account: Account; outer: UsageWindow; inner?: UsageWindow }) {
  const ring = (r: number, w: UsageWindow, opacity: number) => (
    <>
      <circle cx="48" cy="48" r={r} className="rings__track" />
      {w.percent > 0 && (
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
      {inner && ring(INNER, inner, INNER_OPACITY)}
    </svg>
  );
}

/** A dot in the color (and lightness) of a window's ring, so each figure names its ring. */
function Swatch({ account, w, inner }: { account: Account; w: UsageWindow; inner: boolean }) {
  return (
    <span
      className="plan-usage__swatch"
      style={{ background: barColor(w.percent, account.color), opacity: inner ? INNER_OPACITY : 1 }}
      aria-hidden
    />
  );
}

function PlanCard({ account, onOpen }: { account: Account; onOpen: () => void }) {
  const t = useT();
  const language = useLanguage();
  const { retry, retrying } = usePlanRetry(account.id);
  const plan = account.usage.plan;
  const state = retrying ? "loading" : planCardState(account);
  const windows = plan.usage ? sharedWindows(plan.usage) : [];
  // The week outside, the 5-hour session inside.
  const inner = windows.length > 1 ? (windows.find((w) => w.kind === "session") ?? windows[0]) : undefined;
  const outer = windows.find((w) => w !== inner);
  // The ring closest to its limit leads (like `headlineWindow`); the other one follows, smaller.
  const top = inner && outer && inner.percent >= outer.percent ? inner : outer;
  const second = top === inner ? outer : inner;

  let body: ReactNode;
  let label: string;
  if (state === "windows" && top) {
    const level = usageLevel(top.percent);
    const others = windows.filter((w) => w !== outer && w !== inner).map((w) => windowText(w, t));
    const detail = [resetLabel(top, t, language), ...others].filter(Boolean).join(" · ");
    label = windows.map((w) => windowText(w, t)).join(", ");
    body = (
      <>
        <span className={`plan-usage__value plan-usage__value--${level === "ok" ? "ink" : level}`}>
          <Swatch account={account} w={top} inner={top === inner} />
          <span className="plan-usage__kind">{shortWindowLabel(top, t)}</span> {Math.round(top.percent)}%
        </span>
        {second && (
          <span className="plan-usage__second">
            <Swatch account={account} w={second} inner={second === inner} />
            {shortWindowLabel(second, t)}{" "}
            <span className={levelClass(second.percent)}>{Math.round(second.percent)}%</span>
          </span>
        )}
        {detail && <span className="muted small">{detail}</span>}
      </>
    );
  } else if (state === "balance" && plan.usage?.balance) {
    const balance = formatBalance(plan.usage.balance, language);
    label = `${t("accounts.plan.balance")} · ${balance}`;
    body = (
      <span className="plan-usage__value plan-usage__value--ink">
        <span className="plan-usage__kind">{t("accounts.plan.balance")}</span> {balance}
      </span>
    );
  } else if (state === "loading") {
    label = t("accounts.plan.checking");
    body = (
      <span className="plan-usage__skeletons" aria-hidden>
        <span className="plan-usage__skeleton plan-usage__skeleton--value" />
        <span className="plan-usage__skeleton plan-usage__skeleton--line" />
      </span>
    );
  } else if (state === "payAsYouGo") {
    label = t("welcome.plans.payAsYouGo");
    body = <span className="plan-usage__quiet">{label}</span>;
  } else {
    const why =
      state === "unsupported"
        ? t("accounts.plan.notReported", { provider: account.providerName })
        : plan.problem === "spendsTokens"
          ? t("accounts.plan.spendsTokens")
          : t("accounts.plan.unavailable");
    label = `${t("welcome.plans.noData")}: ${why}`;
    body = (
      <>
        <span className="plan-usage__quiet">{t("welcome.plans.noData")}</span>
        <span className="muted small">{why}</span>
      </>
    );
  }
  // Asking again would not help when reading the plan would spend tokens.
  const canRetry = state === "noData" && plan.problem !== "spendsTokens";

  return (
    <div className={`plan-usage-card${canRetry ? " plan-usage-card--retry" : ""}`}>
      <button type="button" className="plan-usage" onClick={onOpen} aria-label={`${account.name}: ${label}`}>
        {state === "windows" && outer && <PlanRings account={account} outer={outer} inner={inner} />}
        {state === "loading" && account.kind === "subscription" && (
          <span className="plan-usage__skeleton plan-usage__skeleton--ring" aria-hidden />
        )}
        <span className="plan-usage__text">
          <AccountChip account={account} />
          {body}
        </span>
      </button>
      {canRetry && (
        <Button
          small
          variant="ghost"
          className="plan-usage__retry"
          onClick={retry}
          aria-label={t("welcome.plans.retryLabel", { name: account.name })}
        >
          {t("welcome.plans.retry")}
        </Button>
      )}
    </div>
  );
}

type NoticeKind = "waiting" | "proposals" | "review" | "limit" | "health";
type Notice = { key: string; kind: NoticeKind; tone: "warn" | "danger" | "info"; text: string; meta?: string; go: () => void };

/** At most this many rows; the rest live in Activity. */
const INBOX_ROWS = 5;

const inboxIcon = {
  width: 16,
  height: 16,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 2,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  "aria-hidden": true,
  focusable: false,
};

/** A warning sign for what is close to a limit or waits on you; a quiet glyph for the rest. */
function NoticeIcon({ notice }: { notice: Notice }) {
  if (notice.tone !== "info") {
    return (
      <svg {...inboxIcon}>
        <path d="M12 4 2.8 19.5h18.4z" />
        <path d="M12 10v4.2M12 17h.01" />
      </svg>
    );
  }
  if (notice.kind === "review") {
    return (
      <svg {...inboxIcon}>
        <path d="M6 4v10M6 18h.01M18 20V10M18 6h.01M9 7h4a3 3 0 0 1 3 3" />
      </svg>
    );
  }
  return (
    <svg {...inboxIcon}>
      <path d="M6 3.5h9l3.5 3.5v13.5H6z" />
      <path d="M9.5 11h5M9.5 15h5" />
    </svg>
  );
}

/** What deserves a look right away, as an inbox: tasks waiting, proposals, plans near their limit. */
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
      kind: "waiting",
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
      kind: "proposals",
      tone: "info",
      text: proposals === 1 ? t("welcome.forYou.proposals.one") : t("welcome.forYou.proposals.other", { n: proposals }),
      go: toBoard,
    });
  }
  const review = board.filter((item) => item.state === "review").length;
  if (review > 0) {
    notices.push({
      key: "review",
      kind: "review",
      tone: "info",
      text: review === 1 ? t("welcome.forYou.review.one") : t("welcome.forYou.review.other", { n: review }),
      go: toBoard,
    });
  }
  for (const account of overview.accounts) {
    const w = headlineWindow(account);
    if (!w || w.percent < 80) continue;
    notices.push({
      key: `limit-${account.id}`,
      kind: "limit",
      tone: w.percent >= 95 ? "danger" : "warn",
      text: t("welcome.forYou.limit", {
        name: account.name,
        percent: Math.round(w.percent),
        label: windowLabel(w, t).toLowerCase(),
      }),
      meta: resetLabel(w, t, language) ?? undefined,
      go: () => onNavigate({ screen: "accounts", account: account.id }),
    });
  }
  // Only what is broken: version drift and other warnings live in Settings, under the diagnosis.
  if (doctor && doctor.errors > 0) {
    notices.push({
      key: "doctor",
      kind: "health",
      tone: "danger",
      text: t("welcome.forYou.doctor", { n: doctor.errors }),
      go: () => onNavigate({ screen: "settings" }),
    });
  }

  if (notices.length === 0) return <p className="for-you__calm">{t("welcome.forYou.calm")}</p>;
  return (
    <>
      <ul className="inbox" aria-label={t("welcome.forYou.title")}>
        {notices.slice(0, INBOX_ROWS).map((n) => (
          <li key={n.key}>
            <button type="button" className={`inbox__row inbox__row--${n.tone}`} onClick={n.go}>
              <span className="inbox__icon">
                <NoticeIcon notice={n} />
              </span>
              <span className="inbox__text">{n.text}</span>
              {n.meta && <span className="inbox__meta">{n.meta}</span>}
              <svg {...inboxIcon} className="inbox__chevron">
                <path d="m9 6 6 6-6 6" />
              </svg>
            </button>
          </li>
        ))}
      </ul>
      {notices.length > INBOX_ROWS && (
        <Button small variant="ghost" className="inbox__more" onClick={toBoard}>
          {t("welcome.forYou.more")}
        </Button>
      )}
    </>
  );
}

/** The latest tasks of every project (and conversations); a row opens the task. */
function RecentTasks({ overview, onNavigate }: { overview: Overview; onNavigate: (v: View) => void }) {
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
            onClick={() => onNavigate(taskView(overview.projects, item.task.project, item.task.id))}
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
  const create = useCreate();
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
          {hasAccounts && (
            <MenuButton label={t("create.menuLabel")} items={create.menu} className="btn btn--secondary">
              <svg {...inboxIcon} className="icon">
                <path d="M12 5v14M5 12h14" />
              </svg>
              {t("create.menu")}
            </MenuButton>
          )}
        </div>
      </header>

      {hasAccounts && (
        <section className="today__section" aria-labelledby="plans-title">
          <h2 id="plans-title">{t("welcome.plans.title")}</h2>
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
            hasAccounts ? (
              <div className="start-choices">
                <button type="button" className="card start-choice" onClick={create.newProject}>
                  <strong>{t("create.newProject")}</strong>
                  <span className="muted small">{t("create.newProjectHint")}</span>
                </button>
                <button type="button" className="card start-choice" onClick={create.openFolder}>
                  <strong>{t("create.openFolder")}</strong>
                  <span className="muted small">{t("create.openFolderHint")}</span>
                </button>
              </div>
            ) : (
              <p className="empty">{t("welcome.empty.noAccounts")}</p>
            )
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
            <RecentTasks overview={overview} onNavigate={onNavigate} />
          </section>
        )}
      </div>

      {summary?.clients.some((c) => c.client !== null) && <SpendByClient />}
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
