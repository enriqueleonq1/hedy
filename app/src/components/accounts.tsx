import type { Account, Login } from "../api/types";
import { useT, type Translate } from "../i18n";
import { PROVIDER_NAMES } from "./providers";
import { ColorDot, type SelectOption } from "./ui/Select";

const BRANDS = ["claude", "chatgpt", "codex", "gemini", "kimi", "antigravity"];

/** "Claude personal" → "personal"; names without a brand prefix stay as they are. */
export function shortAccountLabel(account: Pick<Account, "name">): string {
  const [first = "", ...rest] = account.name.trim().split(/\s+/);
  return rest.length > 0 && BRANDS.includes(first.toLowerCase()) ? rest.join(" ") : account.name;
}

/** An account as an option of a `Select`: its color, its name and (below) its provider. */
export function accountOption(account: Account, detail: string = account.providerName): SelectOption {
  return { value: account.id, label: account.name, detail, icon: <ColorDot color={account.color} /> };
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
      return kind === "apiKey" ? t("accounts.login.keySaved") : t("accounts.login.connected");
    case "loggedOut":
      return kind === "apiKey" ? t("accounts.login.noKey") : t("accounts.login.noSession");
    case "unknown":
      return t("accounts.login.unknown");
  }
}

/** "● Conectada": the login state as a small colored dot and its word (the plan is in the subtitle). */
export function LoginStatus({ account }: { account: Account }) {
  const t = useT();
  return (
    <span className={`login-status login-status--${account.login.state}`}>
      <span className="login-status__dot" aria-hidden />
      {loginText(account.login, account.kind, t)}
    </span>
  );
}
