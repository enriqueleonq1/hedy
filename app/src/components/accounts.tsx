import type { Account, Login } from "../api/types";
import { formatTokens, formatUsd } from "../format";
import { PROVIDER_NAMES } from "./providers";

const BRANDS = ["claude", "chatgpt", "codex", "gemini", "kimi"];

/** "Claude personal" → "personal"; names without a brand prefix stay as they are. */
export function shortAccountLabel(account: Pick<Account, "name">): string {
  const [first = "", ...rest] = account.name.trim().split(/\s+/);
  return rest.length > 0 && BRANDS.includes(first.toLowerCase()) ? rest.join(" ") : account.name;
}

/** "Anthropic · Team", "OpenAI · Plus · Codex", "Moonshot · API key". */
export function accountSubtitle(account: Account): string {
  const plan =
    account.kind === "apiKey"
      ? "API key"
      : account.login.plan
        ? account.login.plan.charAt(0).toUpperCase() + account.login.plan.slice(1)
        : "Suscripción";
  const parts = [PROVIDER_NAMES[account.provider].vendor, plan];
  if (account.provider === "codex") parts.push("Codex");
  if (account.provider === "gemini" && account.kind === "subscription") parts.push("Gemini CLI");
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

function loginText(login: Login, kind: Account["kind"]): string {
  switch (login.state) {
    case "loggedIn":
      if (kind === "apiKey") return "Key guardada";
      return login.plan ? `Conectada · ${login.plan}` : "Conectada";
    case "loggedOut":
      return kind === "apiKey" ? "Sin key" : "Sin sesión";
    case "unknown":
      return "Estado desconocido";
  }
}

export function LoginBadge({ account }: { account: Account }) {
  const icon = { loggedIn: "✔", loggedOut: "✗", unknown: "?" }[account.login.state];
  return (
    <span className={`badge badge--${account.login.state}`}>
      <span aria-hidden>{icon}</span> {loginText(account.login, account.kind)}
    </span>
  );
}

const LIMIT_STATUS: Record<string, string> = {
  allowed: "Dentro del límite",
  allowed_warning: "Cerca del límite",
  rejected: "Límite alcanzado",
};

function resetText(resetsAt: number | null): string | null {
  if (!resetsAt) return null;
  const when = new Intl.DateTimeFormat("es", { weekday: "long", hour: "2-digit", minute: "2-digit" }).format(
    new Date(resetsAt * 1000),
  );
  return `Se renueva el ${when}`;
}

/**
 * What is actually known: the limit status (and % only when the CLI reports
 * it) and Hedy's own token count for the week. Nothing is guessed.
 */
export function UsageBars({ account }: { account: Account }) {
  const { usage } = account;
  const limit = usage.limit;
  const percent = limit?.percent ?? null;
  const tokens = usage.weekTokens > 0 ? `${formatTokens(usage.weekTokens)} tokens` : "sin uso";
  const cost = usage.weekCostUsd != null ? formatUsd(usage.weekCostUsd) : null;

  return (
    <div className="usage">
      {account.kind === "subscription" && (
        <div className="usage__row">
          <div className="row row--between">
            <span>Límite del plan</span>
            <span className="usage__value">
              {percent != null ? `${Math.round(percent)}%` : limit ? (LIMIT_STATUS[limit.status] ?? limit.status) : "sin datos"}
            </span>
          </div>
          <div
            className="bar"
            role="meter"
            aria-label={`Límite del plan: ${percent != null ? `${Math.round(percent)}%` : "sin porcentaje"}`}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={percent ?? 0}
          >
            <div className="bar__fill" style={{ width: `${Math.min(percent ?? 0, 100)}%`, background: account.color }} />
          </div>
          {resetText(limit?.resetsAt ?? null) && <span className="muted small">{resetText(limit?.resetsAt ?? null)}</span>}
        </div>
      )}
      <div className="usage__row">
        <div className="row row--between">
          <span>{account.kind === "apiKey" ? "Gasto de la semana" : "Esta semana"}</span>
          <span className="usage__value">
            {account.kind === "apiKey" ? (cost ?? "sin uso") : tokens}
          </span>
        </div>
        {account.kind === "apiKey" && <span className="muted small">{tokens}</span>}
      </div>
    </div>
  );
}
