import type { ProviderId } from "../api/types";
import type { MessageKey } from "../i18n";

/**
 * How each provider is named in the UI: the subscription people know
 * ("ChatGPT"), the agent that does the work ("Codex") and the company.
 */
export const PROVIDER_NAMES: Record<ProviderId, { plan: string; agent: string; vendor: string }> = {
  claude: { plan: "Claude", agent: "Claude", vendor: "Anthropic" },
  codex: { plan: "ChatGPT", agent: "Codex", vendor: "OpenAI" },
  gemini: { plan: "Gemini", agent: "Gemini", vendor: "Google" },
  kimi: { plan: "Kimi", agent: "Kimi", vendor: "Moonshot" },
  antigravity: { plan: "Antigravity", agent: "Antigravity", vendor: "Google" },
};

/**
 * What to do inside the official login, for CLIs that do not say it plainly.
 * Shown next to the sign-in button.
 */
export const LOGIN_HINT: Partial<Record<ProviderId, MessageKey>> = {
  kimi: "accounts.loginHint.kimi",
  gemini: "accounts.loginHint.gemini",
  antigravity: "accounts.loginHint.antigravity",
};
