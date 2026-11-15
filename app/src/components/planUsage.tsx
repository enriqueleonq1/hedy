// The plan's limit windows (5-hour session, week, per model…) as each provider
// reports them, and how the app keeps them fresh.

import { useCallback, useEffect, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";

import { OVERVIEW_KEY, useApi } from "../api/context";
import type { Account, Overview, PlanState, PlanUsage, UsageWindow } from "../api/types";
import type { Language, MessageKey, Translate } from "../i18n";
import { formatTokens, formatUsd, timeAgo } from "../format";

/** At or above: the warning color. */
export const WARNING_PERCENT = 80;
/** At or above: the danger color. */
export const DANGER_PERCENT = 95;
/** While the window is focused, readings are asked this often. */
export const REFRESH_EVERY_MS = 5 * 60 * 1000;

export type UsageLevel = "ok" | "warning" | "danger";

export function usageLevel(percent: number): UsageLevel {
  if (percent >= DANGER_PERCENT) return "danger";
  if (percent >= WARNING_PERCENT) return "warning";
  return "ok";
}

/** Fill color of a bar: the account's color until it gets close to the limit. */
export function barColor(percent: number, accountColor: string): string {
  const level = usageLevel(percent);
  return level === "ok" ? accountColor : `var(--${level})`;
}

const KIND_ORDER: Record<UsageWindow["kind"], number> = { session: 0, day: 1, week: 2, other: 3 };

/** Windows shared by every model, session first. */
export function sharedWindows(usage: PlanUsage): UsageWindow[] {
  return usage.windows.filter((w) => w.model === null).sort((a, b) => KIND_ORDER[a.kind] - KIND_ORDER[b.kind]);
}

/**
 * The figure for compact labels (header chips, sidebar): the shared window
 * closest to its limit, because that is the one that stops the work first.
 */
export function headlineWindow(account: Account): UsageWindow | null {
  const usage = account.usage.plan.usage;
  if (!usage) return null;
  return sharedWindows(usage).reduce<UsageWindow | null>((top, w) => (!top || w.percent > top.percent ? w : top), null);
}

/**
 * The plan's percentage for compact labels: the shared window closest to its
 * limit (see `headlineWindow`), or what a task reported; `null` when unknown.
 */
export function planPercent(account: Account): number | null {
  return headlineWindow(account)?.percent ?? account.usage.limit?.percent ?? null;
}

/** CSS class that colors a percentage near (or at) the limit. */
export function levelClass(percent: number | null): string | undefined {
  if (percent === null) return undefined;
  const level = usageLevel(percent);
  return level === "ok" ? undefined : `level--${level}`;
}

/** "34%" of the plan, "$2,10" for API spend, tokens otherwise. */
export function usageLabel(account: Account, language: Language): string {
  const { usage } = account;
  const percent = planPercent(account);
  if (percent !== null) return `${Math.round(percent)}%`;
  if (account.kind === "apiKey" && usage.weekCostUsd != null) return formatUsd(usage.weekCostUsd, language);
  if (usage.weekTokens > 0) return `${formatTokens(usage.weekTokens)} tok`;
  return "—";
}

const KIND_LABEL: Record<Exclude<UsageWindow["kind"], "other">, MessageKey> = {
  session: "accounts.plan.session",
  week: "accounts.plan.week",
  day: "accounts.plan.day",
};

export function windowLabel(w: UsageWindow, t: Translate): string {
  if (w.kind !== "other") return t(KIND_LABEL[w.kind]);
  if (w.minutes === null) return t("accounts.plan.other");
  return w.minutes % 60 === 0
    ? t("accounts.plan.hours", { n: w.minutes / 60 })
    : t("accounts.plan.minutes", { n: w.minutes });
}

const SHORT_LABEL: Record<Exclude<UsageWindow["kind"], "other">, MessageKey> = {
  session: "accounts.plan.short.session",
  week: "accounts.plan.short.week",
  day: "accounts.plan.short.day",
};

/** "Sesión 5 h", "Semana": the window's name in compact labels. */
export function shortWindowLabel(w: UsageWindow, t: Translate): string {
  if (w.kind !== "other") return t(SHORT_LABEL[w.kind]);
  if (w.minutes === null) return t("accounts.plan.short.other");
  return w.minutes % 60 === 0
    ? t("accounts.plan.short.hours", { n: w.minutes / 60 })
    : t("accounts.plan.short.minutes", { n: w.minutes });
}

/** "Semana · 83%": a window and its figure, the one pattern for compact plan labels. */
export function windowText(w: UsageWindow, t: Translate): string {
  return `${shortWindowLabel(w, t)} · ${Math.round(w.percent)}%`;
}

/**
 * The headline window (see `headlineWindow`) as `windowText`, wherever an
 * account's plan is summed up in one line; `null` when it is unknown.
 */
export function headlineText(account: Account, t: Translate): string | null {
  const w = headlineWindow(account);
  return w ? windowText(w, t) : null;
}

/** "se renueva hoy a las 19:00", in the active language; the source's text when the time is unknown. */
export function resetLabel(w: UsageWindow, t: Translate, language: Language, now = Date.now()): string | null {
  if (w.resetsAt === null) return w.resetsText ? t("accounts.plan.resetsRaw", { when: w.resetsText }) : null;
  const at = new Date(w.resetsAt * 1000);
  const time = new Intl.DateTimeFormat(language, { hour: "2-digit", minute: "2-digit" }).format(at);
  const day = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const days = Math.round((day(at) - day(new Date(now))) / 86_400_000);
  if (days === 0) return t("accounts.plan.resetsToday", { time });
  if (days === 1) return t("accounts.plan.resetsTomorrow", { time });
  const date = new Intl.DateTimeFormat(language, { weekday: "long", day: "numeric", month: "short" }).format(at);
  return t("accounts.plan.resetsOn", { date, time });
}

const SOURCE_LABEL: Record<PlanUsage["source"], MessageKey> = {
  claudeUsage: "accounts.plan.source.claudeUsage",
  codexRateLimits: "accounts.plan.source.codex",
  taskReport: "accounts.plan.source.task",
  moonshotBalance: "accounts.plan.source.moonshot",
};

/** "según /usage de Claude Code · hace 2 min". */
export function sourceLabel(usage: PlanUsage, t: Translate, now = Date.now() / 1000): string {
  return `${t(SOURCE_LABEL[usage.source])} · ${timeAgo(usage.checkedAt, t, now)}`;
}

/** "US$12,50" / "$12.50", "CN¥3.00". */
export function formatBalance(balance: { available: number; currency: string }, language: Language): string {
  try {
    return new Intl.NumberFormat(language, { style: "currency", currency: balance.currency }).format(balance.available);
  } catch {
    return `${balance.available.toFixed(2)} ${balance.currency}`;
  }
}

/**
 * What a plan card can show: the limit windows, a prepaid balance, "pay as you
 * go" for an API key without one, a reading on its way, nothing read (with a
 * retry), or a provider that reports nothing at all.
 */
export type PlanCardState = "windows" | "balance" | "payAsYouGo" | "loading" | "noData" | "unsupported";

export function planCardState(account: Account): PlanCardState {
  const plan = account.usage.plan;
  if (plan.usage && sharedWindows(plan.usage).length > 0) return "windows";
  if (plan.usage?.balance) return "balance";
  if (plan.supported && plan.attemptedAt === null && plan.problem === null) return "loading";
  if (account.kind === "apiKey") return "payAsYouGo";
  return plan.supported ? "noData" : "unsupported";
}

/** Asks for one account's reading again, on demand (the "retry" of a card without data). */
export function usePlanRetry(id: string): { retry: () => void; retrying: boolean } {
  const api = useApi();
  const client = useQueryClient();
  const [retrying, setRetrying] = useState(false);
  const retry = useCallback(() => {
    setRetrying(true);
    api
      .refreshPlanUsage(id)
      .then((plan) => client.setQueryData<Overview>(OVERVIEW_KEY, (old) => withPlan(old, id, plan)))
      .catch(() => {})
      .finally(() => setRetrying(false));
  }, [api, client, id]);
  return { retry, retrying };
}

function withPlan(overview: Overview | undefined, id: string, plan: PlanState): Overview | undefined {
  if (!overview) return overview;
  return {
    ...overview,
    accounts: overview.accounts.map((a) => (a.id === id ? { ...a, usage: { ...a.usage, plan } } : a)),
  };
}

/**
 * Keeps the plan usage fresh while the shell (and with it the sidebar) is on
 * screen: when it appears or `screen` changes, every five minutes while the
 * window has focus, on focus, and when one of Hedy's tasks ends. The backend
 * asks each provider at most every few minutes and answers from its cache
 * otherwise, so this never blocks or floods anything. Failures are quiet:
 * the last reading stays.
 */
export function usePlanUsageRefresh(accounts: Account[], screen: string) {
  const api = useApi();
  const client = useQueryClient();
  const ids = accounts
    .filter((a) => a.usage.plan.supported)
    .map((a) => a.id)
    .join("\n");
  const refreshing = useRef(new Set<string>());

  const refresh = useCallback(
    (only?: string) => {
      for (const id of ids ? ids.split("\n") : []) {
        if ((only && id !== only) || refreshing.current.has(id)) continue;
        refreshing.current.add(id);
        api
          .refreshPlanUsage(id)
          .then((plan) => client.setQueryData<Overview>(OVERVIEW_KEY, (old) => withPlan(old, id, plan)))
          .catch(() => {})
          .finally(() => refreshing.current.delete(id));
      }
    },
    [api, client, ids],
  );

  useEffect(() => refresh(), [refresh, screen]);

  useEffect(() => {
    const tick = () => {
      if (document.hasFocus()) refresh();
    };
    const timer = window.setInterval(tick, REFRESH_EVERY_MS);
    const onFocus = () => refresh();
    window.addEventListener("focus", onFocus);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener("focus", onFocus);
    };
  }, [refresh]);

  // A task that stops running has just used the plan.
  useEffect(() => {
    const running = new Map<string, boolean>();
    return api.onTaskUpdated((task) => {
      const isRunning = task.status === "running";
      if (running.get(task.id) && !isRunning) refresh(task.account);
      running.set(task.id, isRunning);
    });
  }, [api, refresh]);
}
