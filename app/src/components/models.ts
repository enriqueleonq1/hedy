// Model names and phase summaries shown in the UI.

import type { Account, Phase, Project, ProviderId } from "../api/types";
import type { MessageKey, Translate } from "../i18n";
import { PROVIDER_NAMES } from "./providers";

/** Suggestions only: any model name the provider accepts can be typed. */
export const MODEL_SUGGESTIONS: Record<ProviderId, string[]> = {
  claude: ["opus", "sonnet", "haiku"],
  codex: [],
  kimi: ["kimi-k3.1-pro", "kimi-k3.1-pro-highspeed", "kimi-k3"],
  gemini: ["gemini-4-hydrogen", "gemini-4-carbon", "gemini-3.8-flash"],
  // The ACP CLIs pick their own model.
  antigravity: [],
};

export const PHASES: { phase: Phase; label: MessageKey; optional: boolean }[] = [
  { phase: "plan", label: "accounts.phase.plan", optional: false },
  { phase: "implement", label: "accounts.phase.implement", optional: false },
  { phase: "tests", label: "accounts.phase.tests", optional: true },
  { phase: "review", label: "accounts.phase.review", optional: true },
];

/** Who works on `phase` in `project` (plan and implement default to its account). */
function phaseAccount(project: Project, phase: Phase, accounts: Account[]): Account | undefined {
  const assigned = project.phases[phase];
  if (assigned) return accounts.find((a) => a.id === assigned.account);
  return phase === "plan" || phase === "implement" ? accounts.find((a) => a.id === project.account) : undefined;
}

/** "Claude planifica · Codex implementa · Kimi tests", or just "Claude". */
export function phaseSummary(project: Project, accounts: Account[], t: Translate): string {
  const who = (phase: Phase) => {
    const account = phaseAccount(project, phase, accounts);
    return account ? PROVIDER_NAMES[account.provider].agent : null;
  };
  const [plan, implement, tests, review] = [who("plan"), who("implement"), who("tests"), who("review")];
  if (!tests && !review && plan === implement) return plan ?? "—";
  return [
    plan && t("accounts.phaseSummary.plan", { agent: plan }),
    implement && t("accounts.phaseSummary.implement", { agent: implement }),
    tests && t("accounts.phaseSummary.tests", { agent: tests }),
    review && t("accounts.phaseSummary.review", { agent: review }),
  ]
    .filter(Boolean)
    .join(" · ");
}
