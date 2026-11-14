// Model names and phase summaries shown in the UI.

import type { Account, Phase, Project, ProviderId } from "../api/types";

/** Suggestions only: any model name the provider accepts can be typed. */
export const MODEL_SUGGESTIONS: Record<ProviderId, string[]> = {
  claude: ["opus", "sonnet", "haiku"],
  codex: [],
  kimi: ["kimi-k3.1-pro", "kimi-k3.1-pro-highspeed", "kimi-k3"],
  gemini: ["gemini-4-hydrogen", "gemini-4-carbon", "gemini-3.8-flash"],
};

const PROVIDER_SHORT: Record<ProviderId, string> = {
  claude: "Claude",
  codex: "Codex",
  kimi: "Kimi",
  gemini: "Gemini",
};

export const PHASES: { phase: Phase; label: string; optional: boolean }[] = [
  { phase: "plan", label: "Planificar", optional: false },
  { phase: "implement", label: "Implementar", optional: false },
  { phase: "tests", label: "Escribir tests", optional: true },
  { phase: "review", label: "Revisar", optional: true },
];

/** Who works on `phase` in `project` (plan and implement default to its account). */
export function phaseAccount(project: Project, phase: Phase, accounts: Account[]): Account | undefined {
  const assigned = project.phases[phase];
  if (assigned) return accounts.find((a) => a.id === assigned.account);
  return phase === "plan" || phase === "implement" ? accounts.find((a) => a.id === project.account) : undefined;
}

/** "Claude planifica · Codex implementa · Kimi tests", or just "Claude". */
export function phaseSummary(project: Project, accounts: Account[]): string {
  const who = (phase: Phase) => {
    const account = phaseAccount(project, phase, accounts);
    return account ? PROVIDER_SHORT[account.provider] : null;
  };
  const [plan, implement, tests, review] = [who("plan"), who("implement"), who("tests"), who("review")];
  if (!tests && !review && plan === implement) return plan ?? "—";
  return [
    plan && `${plan} planifica`,
    implement && `${implement} implementa`,
    tests && `${tests} tests`,
    review && `${review} revisa`,
  ]
    .filter(Boolean)
    .join(" · ");
}
