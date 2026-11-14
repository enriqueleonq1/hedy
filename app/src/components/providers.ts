import type { ProviderId } from "../api/types";

/**
 * How each provider is named in the UI: the subscription people know
 * ("ChatGPT"), the agent that does the work ("Codex") and the company.
 */
export const PROVIDER_NAMES: Record<ProviderId, { plan: string; agent: string; vendor: string }> = {
  claude: { plan: "Claude", agent: "Claude", vendor: "Anthropic" },
  codex: { plan: "ChatGPT", agent: "Codex", vendor: "OpenAI" },
  gemini: { plan: "Gemini", agent: "Gemini", vendor: "Google" },
  kimi: { plan: "Kimi", agent: "Kimi", vendor: "Moonshot" },
};
