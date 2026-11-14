import type { Account, Login, UsageWindow } from "../api/types";
import { formatTokens, formatUsd } from "../format";
import { useLanguage, useT, type Language, type MessageKey, type Translate } from "../i18n";
import {
  barColor,
  formatBalance,
  resetLabel,
  sharedWindows,
  sourceLabel,
  usageLevel,
  windowLabel,
} from "./planUsage";
import { PROVIDER_NAMES } from "./providers";

const BRANDS = ["claude", "chatgpt", "codex", "gemini", "kimi", "antigravity"];

/** "Claude personal" → "personal"; names without a brand prefix stay as they are. */
export function shortAccountLabel(account: Pick<Account, "name">): string {
  const [first = "", ...rest] = account.name.trim().split(/\s+/);
  return rest.length > 0 && BRANDS.includes(first.toLowerCase()) ? rest.join(" ") : account.name;
}

/** "Anthropic · Team", "OpenAI · Plus · Codex", "Moonshot · API key". */
export function accountSubtitle(account: Account, t: Translate): string {
  const plan =
    account.kind === "apiKey"
      ? "API key"
      : account.login.plan
        ? account.login.plan.charAt(0).toUpperCase() + account.login.plan.slice(1)
        : t("accounts.plan.subscription");
  const parts = [PROVIDER_NAMES[account.provider].vendor, plan];
  if (account.provider === "codex") parts.push("Codex");
  if (account.provider === "gemini" && account.kind === "subscription") parts.push("Gemini CLI");
  if (account.provider === "kimi" && account.kind === "subscription") parts.push("Kimi Code");
  return parts.join(" · ");
}

export function AccountChip({ account, muted }: { account: Pick<Account, "name" | "color">; muted?: boolean }) {
  return (
    <span className={`chip${muted ? " chip--muted" : ""}`}>
      <span className="dot" style={{ background: account.color }} aria-hidden />
      {account.name}
    </span>
  );
}

function loginText(login: Login, kind: Account["kind"], t: Translate): string {
  switch (login.state) {
    case "loggedIn":
      if (kind === "apiKey") return t("accounts.login.keySaved");
      return login.plan ? t("accounts.login.connectedPlan", { plan: login.plan }) : t("accounts.login.connected");
    case "loggedOut":
      return kind === "apiKey" ? t("accounts.login.noKey") : t("accounts.login.noSession");
    case "unknown":
      return t("accounts.login.unknown");
  }
}

export function LoginBadge({ account }: { account: Account }) {
  const t = useT();
  const icon = { loggedIn: "✔", loggedOut: "✗", unknown: "?" }[account.login.state];
  return (
    <span className={`badge badge--${account.login.state}`}>
      <span aria-hidden>{icon}</span> {loginText(account.login, account.kind, t)}
    </span>
  );
}

const LIMIT_STATUS: Record<string, MessageKey> = {
  allowed: "accounts.limit.allowed",
  allowed_warning: "accounts.limit.warning",
  rejected: "accounts.limit.rejected",
};

function resetText(resetsAt: number | null, t: Translate, language: Language): string | null {
  if (!resetsAt) return null;
  const when = new Intl.DateTimeFormat(language, { weekday: "long", hour: "2-digit", minute: "2-digit" }).format(
    new Date(resetsAt * 1000),
  );
  return t("accounts.usage.resets", { when });
}

/** One limit window: label, percentage, bar and when it renews. */
function WindowRow({ window: w, color }: { window: UsageWindow; color: string }) {
  const t = useT();
  const language = useLanguage();
  const label = windowLabel(w, t);
  const percent = `${Math.round(w.percent)}%`;
  const resets = resetLabel(w, t, language);
  const level = usageLevel(w.percent);
  return (
    <div className="usage__row">
      <div className="row row--between">
        <span>{label}</span>
        <span className={`usage__value${level === "ok" ? "" : ` usage__value--${level}`}`}>{percent}</span>
      </div>
      <div
        className="bar"
        role="meter"
        aria-label={t("accounts.plan.meter", { label, value: percent })}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={w.percent}
      >
        <div className="bar__fill" style={{ width: `${Math.min(w.percent, 100)}%`, background: barColor(w.percent, color) }} />
      </div>
      {resets && <span className="muted small">{resets}</span>}
    </div>
  );
}

/** The plan's windows, or a short note on why there are none. */
function PlanRows({ account }: { account: Account }) {
  const t = useT();
  const language = useLanguage();
  const { plan, limit } = account.usage;
  const usage = plan.usage;
  const failed =
    plan.problem === "spendsTokens"
      ? t("accounts.plan.spendsTokens")
      : plan.problem === "unavailable"
        ? t("accounts.plan.unavailable")
        : null;

  if (usage && (usage.windows.length > 0 || usage.balance)) {
    const models = usage.windows.filter((w) => w.model !== null && w.percent > 0);
    return (
      <>
        {sharedWindows(usage).map((w) => (
          <WindowRow key={`${w.kind}-${w.minutes ?? ""}`} window={w} color={account.color} />
        ))}
        {usage.balance && (
          <div className="usage__row">
            <div className="row row--between">
              <span>{t("accounts.plan.balance")}</span>
              <span className="usage__value">{formatBalance(usage.balance, language)}</span>
            </div>
          </div>
        )}
        {models.length > 0 && (
          <span className="muted small">
            {t("accounts.plan.perModel", {
              list: models.map((w) => `${w.model} ${Math.round(w.percent)}%`).join(" · "),
            })}
          </span>
        )}
        <span className="muted small">
          {sourceLabel(usage, t)}
          {failed && ` · ${failed}`}
        </span>
      </>
    );
  }

  if (account.kind === "apiKey") return null;
  if (!plan.supported) {
    return <span className="muted small">{t("accounts.plan.notReported", { provider: account.providerName })}</span>;
  }
  // No reading yet: what the CLI said during Hedy's tasks, if anything.
  const limitStatus = limit && LIMIT_STATUS[limit.status];
  const resets = resetText(limit?.resetsAt ?? null, t, language);
  return (
    <div className="usage__row">
      <div className="row row--between">
        <span>{t("accounts.usage.planLimit")}</span>
        <span className="usage__value">
          {limit ? (limitStatus ? t(limitStatus) : limit.status) : (failed ?? t("accounts.plan.checking"))}
        </span>
      </div>
      {resets && <span className="muted small">{resets}</span>}
    </div>
  );
}

/**
 * What is actually known: the plan's limit windows as the provider reports
 * them (Claude Code's /usage, Codex…) and Hedy's own token count for the
 * week, labelled as such. Nothing is guessed.
 */
export function UsageBars({ account }: { account: Account }) {
  const t = useT();
  const language = useLanguage();
  const { usage } = account;
  const tokens = usage.weekTokens > 0 ? `${formatTokens(usage.weekTokens)} tokens` : t("accounts.usage.none");
  const cost = usage.weekCostUsd != null ? formatUsd(usage.weekCostUsd, language) : null;

  return (
    <div className="usage">
      <PlanRows account={account} />
      <div className="usage__row">
        <div className="row row--between">
          <span>{account.kind === "apiKey" ? t("accounts.usage.weekSpend") : t("accounts.usage.hedyWeek")}</span>
          <span className="usage__value">
            {account.kind === "apiKey" ? (cost ?? t("accounts.usage.none")) : tokens}
          </span>
        </div>
        {account.kind === "apiKey" && <span className="muted small">{tokens}</span>}
      </div>
    </div>
  );
}
