// An account's plan in the accounts list: one meter per limit window, where
// the figures come from (with "Actualizar"), the week in Hedy, and the tasks
// paused at its limit, a click away. Hedy never moves them to another account.

import type { Account, ActivityItem, Project, Task, UsageWindow } from "../../api/types";
import { formatTokens, formatUsd } from "../../format";
import { useLanguage, useT, type Language, type MessageKey, type Translate } from "../../i18n";
import {
  formatBalance,
  planCardState,
  resetLabel,
  sharedWindows,
  sourceLabel,
  usePlanRefresh,
  usePlanRefreshActions,
  windowLabel,
} from "../../components/planUsage";
import { Button } from "../../components/ui/Button";
import { Meter, meterLevel } from "../../components/ui/Meter";

const LIMIT_STATUS: Record<string, MessageKey> = {
  allowed: "accounts.limit.allowed",
  allowed_warning: "accounts.limit.warning",
  rejected: "accounts.limit.rejected",
};

/** Why the last reading failed, if it did. */
function problemText(account: Account, t: Translate): string | null {
  switch (account.usage.plan.problem) {
    case "spendsTokens":
      return t("accounts.plan.spendsTokens");
    case "unavailable":
      return t("accounts.plan.unavailable");
    default:
      return null;
  }
}

/**
 * Where the plan's reading comes from and how fresh it is ("según /usage de
 * Claude Code · hace 2 min"), and whether the last refresh failed; `null`
 * without a reading.
 */
export function usageSource(account: Account, t: Translate, now?: number): string | null {
  const usage = account.usage.plan.usage;
  if (!usage || (usage.windows.length === 0 && !usage.balance)) return null;
  const failed = problemText(account, t);
  const source = sourceLabel(usage, t, now);
  return failed ? `${source} · ${failed}` : source;
}

/** "Actualizar": asks for this account's reading now, said in a toast. */
function RefreshButton({ account }: { account: Account }) {
  const t = useT();
  const { busy } = usePlanRefresh();
  const { refreshAccount } = usePlanRefreshActions();
  return (
    <Button
      small
      variant="ghost"
      className="account-plan__refresh"
      disabled={busy.has(account.id)}
      aria-label={t("accounts.plan.refreshLabel", { name: account.name })}
      onClick={() => refreshAccount(account)}
    >
      {t("accounts.plan.refresh")}
    </Button>
  );
}

/** One limit window: its name and figure, the meter, and when it renews. */
function WindowMeter({ window: w }: { window: UsageWindow }) {
  const t = useT();
  const language = useLanguage();
  const label = windowLabel(w, t);
  const percent = `${Math.round(w.percent)}%`;
  const level = meterLevel(w.percent);
  const resets = resetLabel(w, t, language);
  return (
    <div className="account-plan__window">
      <div className="account-plan__top">
        <span>{label}</span>
        <span className={`usage__value${level === "ok" ? "" : level === "warn" ? " usage__value--warning" : " usage__value--danger"}`}>
          {percent}
        </span>
      </div>
      <Meter percent={w.percent} label={t("accounts.plan.meter", { label, value: percent })} />
      {resets && <span className="usage__note">{resets}</span>}
    </div>
  );
}

/**
 * The plan's figures as the provider reports them, or a short line on why
 * there are none. Without a session there are no figures at all: signing in
 * is what the account needs.
 */
export function AccountPlan({ account }: { account: Account }) {
  const t = useT();
  const language = useLanguage();
  const { busy } = usePlanRefresh();
  const plan = account.usage.plan;

  if (account.kind === "subscription" && !plan.supported) {
    return <p className="usage__note">{t("accounts.plan.notReported", { provider: account.providerName })}</p>;
  }
  if (account.login.state === "loggedOut") return null;

  const state = planCardState(account);
  const usage = plan.usage;
  if ((state === "windows" || state === "balance") && usage) {
    const models = usage.windows.filter((w) => w.model !== null && w.percent > 0);
    return (
      <div className="account-plan" aria-busy={busy.has(account.id) || undefined}>
        <div className="account-plan__meters">
          {sharedWindows(usage).map((w) => (
            <WindowMeter key={`${w.kind}-${w.minutes ?? ""}`} window={w} />
          ))}
          {usage.balance && (
            <div className="account-plan__window">
              <div className="account-plan__top">
                <span>{t("accounts.plan.balance")}</span>
                <span className="usage__value">{formatBalance(usage.balance, language)}</span>
              </div>
            </div>
          )}
        </div>
        {models.length > 0 && (
          <span className="usage__note">
            {t("accounts.plan.perModel", {
              list: models.map((w) => `${w.model} ${Math.round(w.percent)}%`).join(" · "),
            })}
          </span>
        )}
        <p className="account-plan__source">
          <span>{usageSource(account, t)}</span>
          <RefreshButton account={account} />
        </p>
      </div>
    );
  }
  if (state === "loading") return <p className="usage__note">{t("accounts.plan.checking")}</p>;
  if (state !== "noData") return null;

  // Nothing read: what the CLI said during Hedy's tasks, if anything.
  const limit = account.usage.limit;
  const limitStatus = limit && LIMIT_STATUS[limit.status];
  const limitResets = limit?.resetsAt
    ? t("accounts.usage.resets", {
        when: new Intl.DateTimeFormat(language, { weekday: "long", hour: "2-digit", minute: "2-digit" }).format(
          new Date(limit.resetsAt * 1000),
        ),
      })
    : null;
  const failed = problemText(account, t);
  return (
    <div className="account-plan">
      {limit && (
        <div className="account-plan__top">
          <span>{t("accounts.usage.planLimit")}</span>
          <span className="usage__value">{limitStatus ? t(limitStatus) : limit.status}</span>
        </div>
      )}
      {limitResets && <span className="usage__note">{limitResets}</span>}
      <p className="account-plan__source">
        <span>{[t("accounts.plan.noData"), failed].filter(Boolean).join(" · ")}</span>
        <RefreshButton account={account} />
      </p>
    </div>
  );
}

/** What Hedy itself ran this week: tokens, or the estimated spend of an API key. */
export function HedyWeek({ account }: { account: Account }) {
  const t = useT();
  const language = useLanguage();
  const { usage } = account;
  const tokens = usage.weekTokens > 0 ? `${formatTokens(usage.weekTokens)} tokens` : t("accounts.usage.none");
  const apiKey = account.kind === "apiKey";
  const cost = usage.weekCostUsd != null ? formatUsd(usage.weekCostUsd, language) : null;
  return (
    <div className="account-plan__week">
      <div className="account-plan__top">
        <span>{apiKey ? t("accounts.usage.weekSpend") : t("accounts.usage.hedyWeek")}</span>
        <span className="usage__value">{apiKey ? (cost ?? t("accounts.usage.none")) : tokens}</span>
      </div>
      {apiKey && usage.weekTokens > 0 && <span className="usage__note">{tokens}</span>}
    </div>
  );
}

// ---------------------------------------------------------------- paused tasks

/** The tasks waiting for this account's limit, the latest first, and when it renews. */
export interface Paused {
  count: number;
  latest: Task;
  /** Unix seconds; `null` when unknown. */
  until: number | null;
}

/** The account a task is working with: the one it waits for, else its current step's. */
function taskAccount(task: Task): string {
  return task.wait?.account ?? task.stages[task.stage]?.account ?? task.account;
}

/**
 * The tasks stopped at this account's limit (board state "limited"), and when
 * they can go on: the time a task waits for, else the reset of the windows at
 * 100 %, else what the CLI reported. `null` when none is paused.
 */
export function pausedOn(account: Account, items: ActivityItem[] | undefined): Paused | null {
  const paused = (items ?? [])
    .filter((i) => i.state === "limited" && taskAccount(i.task) === account.id)
    .sort((a, b) => b.updatedAt - a.updatedAt);
  const latest = paused[0]?.task;
  if (!latest) return null;
  const waits = paused.map((i) => i.task.wait?.until).filter((u): u is number => u != null);
  const full = (account.usage.plan.usage?.windows ?? [])
    .filter((w) => w.model === null && w.percent >= 100 && w.resetsAt !== null)
    .map((w) => w.resetsAt as number);
  const until =
    waits.length > 0
      ? Math.min(...waits)
      : full.length > 0
        ? Math.max(...full)
        : (account.usage.limit?.resetsAt ?? null);
  return { count: paused.length, latest, until };
}

/** "19:40" today; "jueves 09:00" another day. */
export function untilLabel(until: number, language: Language, now = Date.now()): string {
  const at = new Date(until * 1000);
  const today = new Date(now);
  const sameDay = at.toDateString() === today.toDateString();
  return new Intl.DateTimeFormat(language, {
    ...(sameDay ? {} : { weekday: "long" }),
    hour: "2-digit",
    minute: "2-digit",
  }).format(at);
}

/** The task's address: in its project, or as a conversation outside any. */
export function taskHref(projects: Pick<Project, "path">[], task: Pick<Task, "id" | "project">): string {
  return projects.some((p) => p.path === task.project)
    ? `#p=${encodeURIComponent(task.project)}&t=${encodeURIComponent(task.id)}`
    : `#c=${encodeURIComponent(task.id)}`;
}

/** "1 tarea en pausa hasta 19:40 · Ver la tarea": the way out of a reached limit, never another account. */
export function PausedLine({ paused, projects }: { paused: Paused; projects: Pick<Project, "path">[] }) {
  const t = useT();
  const language = useLanguage();
  const time = paused.until === null ? null : untilLabel(paused.until, language);
  const text =
    paused.count === 1
      ? time
        ? t("accounts.paused.one", { time })
        : t("accounts.paused.oneNoTime")
      : time
        ? t("accounts.paused.many", { n: paused.count, time })
        : t("accounts.paused.manyNoTime", { n: paused.count });
  return (
    <p className="account-plan__paused" role="status">
      <span>{text}</span>
      <span aria-hidden> · </span>
      <a
        className="account-plan__paused-link"
        href={taskHref(projects, paused.latest)}
        aria-label={t("accounts.paused.openLabel", { title: paused.latest.title })}
      >
        {t("accounts.paused.open")}
      </a>
    </p>
  );
}
