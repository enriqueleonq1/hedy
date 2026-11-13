import type { Account, Login, ProviderId } from "../api/types";

const VENDOR: Record<ProviderId, string> = {
  claude: "Anthropic",
  codex: "OpenAI",
  gemini: "Google",
  kimi: "Moonshot",
};

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
  const parts = [VENDOR[account.provider], plan];
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

export function loginText(login: Login, kind: Account["kind"]): string {
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

/**
 * Subscription windows (5 h and weekly) or API budget. Nothing is recorded
 * until the chat arrives (phase 3), and the UI says so instead of guessing.
 */
export function UsageBars({ account }: { account: Account }) {
  const rows = account.kind === "apiKey" ? ["Presupuesto del mes", "Hoy"] : ["Ventana de 5 h", "Semanal"];
  return (
    <div className="usage">
      {rows.map((label) => (
        <div className="usage__row" key={label}>
          <div className="row row--between">
            <span>{label}</span>
            <span className="usage__value">—</span>
          </div>
          <div
            className="bar"
            role="meter"
            aria-label={`${label}: sin datos`}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={0}
          >
            <div className="bar__fill" style={{ width: 0, background: account.color }} />
          </div>
        </div>
      ))}
    </div>
  );
}
