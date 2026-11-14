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
  antigravity: { plan: "Antigravity", agent: "Antigravity", vendor: "Google" },
};

/**
 * What to do inside the official login, for CLIs that do not say it plainly.
 * Shown next to "Iniciar sesión".
 */
export const LOGIN_HINT: Partial<Record<ProviderId, string>> = {
  kimi: "En la terminal escribe /login, elige «Kimi Code (OAuth)» y sal con /exit.",
  gemini: "En la terminal elige «Log in with Google» y sal con /quit.",
  antigravity: "Inicia sesión con Google y pega el código que te muestra la terminal.",
};
