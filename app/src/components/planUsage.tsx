// The plan's limit windows (5-hour session, week, per model…) as each provider
// reports them, and how the app keeps them fresh.

import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useQueryClient } from "@tanstack/react-query";

import { OVERVIEW_KEY, useApi } from "../api/context";
import type { Account, Overview, PlanState, PlanUsage, UsageWindow } from "../api/types";
import { useLanguage, useT, type Language, type MessageKey, type Translate } from "../i18n";
import { formatTokens, formatUsd, timeAgo } from "../format";
import { formatCountdown } from "../tasks/countdown";
import { useToast } from "./ui/Toast";

/** At or above: the warning color. */
export const WARNING_PERCENT = 80;
/** At or above: the danger color. */
export const DANGER_PERCENT = 95;
/**
 * The backend asks each provider at most this often and answers from its cache
 * otherwise (`PLAN_USAGE_MIN_INTERVAL_SECS` in hedy-core).
 */
export const PLAN_USAGE_MIN_INTERVAL_SECS = 180;
/**
 * While the window is focused, a reading this old is asked again: the
 * backend's minimum, so a figure is never older than it has to be.
 */
export const REFRESH_EVERY_MS = PLAN_USAGE_MIN_INTERVAL_SECS * 1000;
/**
 * How often the focused window looks for readings that are due. Checking the
 * age (rather than asking on a fixed beat) keeps the pace when a reading
 * came from elsewhere: a focus, a screen change, a task that ended.
 */
export const DUE_CHECK_MS = 15_000;
/** A figure older than this is shown as being read again until the new one comes. */
export const STALE_AFTER_MS = 2 * REFRESH_EVERY_MS;

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

/**
 * "Claude personal · Semana · 83% · actualizado hace 12 min": an account's
 * figure and how old its reading is, for tooltips.
 */
export function usageTitle(account: Account, t: Translate, language: Language, now = Date.now() / 1000): string {
  const usage = headlineText(account, t) ?? usageLabel(account, language);
  const checkedAt = account.usage.plan.usage?.checkedAt;
  const age = checkedAt == null ? null : t("shell.usage.updated", { when: timeAgo(checkedAt, t, now) });
  return [account.name, usage, age].filter(Boolean).join(" · ");
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

/** What asking for a reading gave. */
export type RefreshOutcome =
  /** The provider was asked just now (`plan.problem` says if it failed). */
  | { kind: "fresh"; plan: PlanState }
  /** The backend answered from its cache: it asked less than its minimum ago. */
  | { kind: "cached"; plan: PlanState; retryInSecs: number }
  /** The backend could not be reached. */
  | { kind: "failed" };

export interface PlanRefresh {
  /** Accounts whose figure is being read again (shown muted, `aria-busy`). */
  busy: ReadonlySet<string>;
  /** Asks for one account's reading now, showing its figure as busy meanwhile. */
  refreshNow(id: string): Promise<RefreshOutcome>;
  /** The same for every account with a usage source. */
  refreshAllNow(): Promise<RefreshOutcome[]>;
}

const NO_REFRESH: PlanRefresh = {
  busy: new Set(),
  refreshNow: async () => ({ kind: "failed" }),
  refreshAllNow: async () => [],
};

const PlanRefreshContext = createContext<PlanRefresh>(NO_REFRESH);

/** Keeps the plan usage fresh (`usePlanUsageRefresh`) for everything under it. */
export function PlanUsageProvider({ accounts, screen, children }: { accounts: Account[]; screen: string; children: ReactNode }) {
  const refresh = usePlanUsageRefresh(accounts, screen);
  return <PlanRefreshContext.Provider value={refresh}>{children}</PlanRefreshContext.Provider>;
}

/** The plan refresh of the nearest `PlanUsageProvider` (none: nothing is busy, nothing is asked). */
export function usePlanRefresh(): PlanRefresh {
  return useContext(PlanRefreshContext);
}

/** Whether a reading is due: the backend would ask the provider again. */
function isDue(plan: PlanState, now: number): boolean {
  return plan.attemptedAt === null || Math.floor(now) - plan.attemptedAt >= PLAN_USAGE_MIN_INTERVAL_SECS;
}

/** Whether a figure is old enough to show it as being read again. */
function isStale(plan: PlanState, now: number): boolean {
  const checkedAt = plan.usage?.checkedAt;
  return checkedAt != null && (now - checkedAt) * 1000 > STALE_AFTER_MS;
}

/**
 * Keeps the plan usage fresh while the shell is on screen: when it appears or
 * `screen` changes, whenever a reading is `REFRESH_EVERY_MS` old while the
 * window has focus, on focus, and when one of Hedy's tasks ends. The backend
 * asks each provider at most every `PLAN_USAGE_MIN_INTERVAL_SECS` and answers
 * from its cache otherwise, so this never blocks or floods anything. Failures
 * are quiet: the last reading stays. A figure is marked busy while it is read
 * again after the window was away longer than the interval, when it is older
 * than `STALE_AFTER_MS`, or when the user asked.
 */
export function usePlanUsageRefresh(accounts: Account[], screen: string): PlanRefresh {
  const api = useApi();
  const client = useQueryClient();
  const latest = useRef(accounts);
  useEffect(() => {
    latest.current = accounts;
  });
  const ids = accounts
    .filter((a) => a.usage.plan.supported)
    .map((a) => a.id)
    .join("\n");
  const inFlight = useRef(new Map<string, Promise<RefreshOutcome>>());
  const [busy, setBusy] = useState<ReadonlySet<string>>(() => new Set());

  const mark = useCallback((id: string, on: boolean) => {
    setBusy((old) => {
      if (old.has(id) === on) return old;
      const next = new Set(old);
      if (on) next.add(id);
      else next.delete(id);
      return next;
    });
  }, []);

  /** One reading at a time per account: a second ask joins the one on its way. */
  const ask = useCallback(
    (id: string, show: boolean): Promise<RefreshOutcome> => {
      if (show) mark(id, true);
      const running = inFlight.current.get(id);
      if (running) return running;
      const asked = Math.floor(Date.now() / 1000);
      const reading = api
        .refreshPlanUsage(id)
        .then((plan): RefreshOutcome => {
          client.setQueryData<Overview>(OVERVIEW_KEY, (old) => withPlan(old, id, plan));
          if (plan.attemptedAt === null || plan.attemptedAt >= asked) return { kind: "fresh", plan };
          const age = Date.now() / 1000 - plan.attemptedAt;
          return { kind: "cached", plan, retryInSecs: Math.max(1, Math.ceil(PLAN_USAGE_MIN_INTERVAL_SECS - age)) };
        })
        .catch((): RefreshOutcome => ({ kind: "failed" }))
        .finally(() => {
          inFlight.current.delete(id);
          mark(id, false);
        });
      inFlight.current.set(id, reading);
      return reading;
    },
    [api, client, mark],
  );

  const refresh = useCallback(
    ({ only, due = false, show = false }: { only?: string; due?: boolean; show?: boolean } = {}) => {
      const now = Date.now() / 1000;
      for (const account of latest.current) {
        const plan = account.usage.plan;
        if (!plan.supported || (only && account.id !== only) || (due && !isDue(plan, now))) continue;
        void ask(account.id, show || isStale(plan, now));
      }
    },
    [ask],
  );

  // `ids`: a new account gets its first reading right away.
  useEffect(() => refresh(), [refresh, screen, ids]);

  useEffect(() => {
    let leftAt: number | null = null;
    const timer = window.setInterval(() => {
      if (document.hasFocus()) refresh({ due: true });
    }, DUE_CHECK_MS);
    const onBlur = () => {
      leftAt = Date.now();
    };
    const onFocus = () => {
      const away = leftAt !== null && Date.now() - leftAt > REFRESH_EVERY_MS;
      leftAt = null;
      refresh({ show: away });
    };
    window.addEventListener("blur", onBlur);
    window.addEventListener("focus", onFocus);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener("blur", onBlur);
      window.removeEventListener("focus", onFocus);
    };
  }, [refresh]);

  // A task that stops running has just used the plan.
  useEffect(() => {
    const running = new Map<string, boolean>();
    return api.onTaskUpdated((task) => {
      const isRunning = task.status === "running";
      if (running.get(task.id) && !isRunning) refresh({ only: task.account });
      running.set(task.id, isRunning);
    });
  }, [api, refresh]);

  return useMemo<PlanRefresh>(
    () => ({
      busy,
      refreshNow: (id) => ask(id, true),
      refreshAllNow: () =>
        Promise.all(latest.current.filter((a) => a.usage.plan.supported).map((a) => ask(a.id, true))),
    }),
    [busy, ask],
  );
}

/**
 * "Actualizar ahora" from a menu or the palette, said in a toast: the new
 * figure, or, when the backend answered from its cache, how old the reading
 * is and when it can be asked again (never a pretended refresh).
 */
export function usePlanRefreshActions() {
  const { refreshNow, refreshAllNow } = usePlanRefresh();
  const toast = useToast();
  const t = useT();
  const language = useLanguage();

  const cachedText = useCallback(
    (outcome: Extract<RefreshOutcome, { kind: "cached" }>) => {
      const read = outcome.plan.usage?.checkedAt ?? outcome.plan.attemptedAt ?? Date.now() / 1000;
      return t("shell.usage.cached", {
        when: timeAgo(read, t),
        wait: formatCountdown(outcome.retryInSecs, t) ?? t("wait.time.seconds", { s: 1 }),
      });
    },
    [t],
  );

  const refreshAccount = useCallback(
    (account: Account) => {
      void refreshNow(account.id).then((outcome) => {
        if (outcome.kind === "cached") toast.info(cachedText(outcome));
        else if (outcome.kind === "failed" || outcome.plan.problem !== null)
          toast.info(t("shell.usage.refreshFailed", { name: account.name }));
        else {
          const updated: Account = { ...account, usage: { ...account.usage, plan: outcome.plan } };
          const usage = headlineText(updated, t) ?? usageLabel(updated, language);
          toast.info(t("shell.usage.refreshed", { name: account.name, usage }));
        }
      });
    },
    [refreshNow, toast, t, language, cachedText],
  );

  const refreshAll = useCallback(() => {
    void refreshAllNow().then((outcomes) => {
      if (outcomes.length === 0) return;
      const fresh = outcomes.filter((o) => o.kind === "fresh");
      const cached = outcomes.filter((o) => o.kind === "cached");
      if (fresh.some((o) => o.plan.problem === null)) toast.info(t("shell.usage.refreshedAll"));
      else if (cached.length === 0 || fresh.length > 0) toast.info(t("shell.usage.refreshFailedAll"));
      else toast.info(cachedText(cached.reduce((a, b) => (b.retryInSecs < a.retryInSecs ? b : a))));
    });
  }, [refreshAllNow, toast, t, cachedText]);

  return { refreshAccount, refreshAll };
}
