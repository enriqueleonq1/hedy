// Helpers of the skills section: its query, its actions, and the words for
// verdicts, findings and trials.

import { useQuery } from "@tanstack/react-query";

import { useAction, useApi } from "../../api/context";
import type { HedyApi } from "../../api/HedyApi";
import type { SkillFinding, SkillVerdict, TrialResult, TrialVoid } from "../../api/types";
import type { MessageKey, Translate } from "../../i18n";

/** Under `["skills"]`: what refreshes a project's skills (an agent's proposal) refreshes this too. */
export const SKILLS_OVERVIEW_KEY = ["skills", "*overview"] as const;

export function useSkillsOverview() {
  const api = useApi();
  return useQuery({ queryKey: SKILLS_OVERVIEW_KEY, queryFn: () => api.skillsOverview() });
}

/** An action on skills: refreshes the section and every project's skills tab. */
export function useLibraryAction<TVars, TResult>(action: (api: HedyApi, vars: TVars) => Promise<TResult>) {
  return useAction(action, { invalidate: [["skills"]] });
}

/** An action that changes no skill (a review, a preview): nothing to refresh. */
export function useReadAction<TVars, TResult>(action: (api: HedyApi, vars: TVars) => Promise<TResult>) {
  return useAction(action, { invalidate: [] });
}

export const verdictKey: Record<SkillVerdict, MessageKey> = {
  useful: "skillLib.verdict.useful",
  doubtful: "skillLib.verdict.doubtful",
  harmful: "skillLib.verdict.harmful",
  untested: "skillLib.verdict.untested",
};

/** How a finding weighs: good, a warning, bad, or only said. */
export function findingWeight(f: SkillFinding): "good" | "warning" | "bad" | "info" {
  switch (f.kind) {
    case "dangerous":
    case "contradicts":
      return "bad";
    case "safe":
      return "info";
    case "concreteSteps":
    case "helpful":
      return "good";
    default:
      return "warning";
  }
}

export function fromAi(f: SkillFinding): boolean {
  return f.kind === "contradicts" || f.kind === "vague" || f.kind === "helpful";
}

export function findingText(f: SkillFinding, t: Translate): string {
  switch (f.kind) {
    case "long":
      return t("skillLib.finding.long", { tokens: f.tokens.toLocaleString() });
    case "broadDescription":
      return t("skillLib.finding.broadDescription", { phrase: f.phrase });
    case "overlaps":
      return t("skillLib.finding.overlaps", { skill: f.skill, percent: f.percent });
    case "dangerous":
      return t("skillLib.finding.dangerous", { command: f.command });
    case "risky":
      return t("skillLib.finding.risky", { command: f.command });
    case "missingFiles":
      return t("skillLib.finding.missingFiles", { files: f.files.join(", ") });
    case "contradicts":
      return t("skillLib.finding.contradicts", { note: f.note, says: f.says });
    case "vague":
      return t("skillLib.finding.vague", { quote: f.quote, why: f.why });
    case "helpful":
      return t("skillLib.finding.helpful", { quote: f.quote, why: f.why });
    case "shortDescription":
      return t("skillLib.finding.shortDescription");
    case "noSteps":
      return t("skillLib.finding.noSteps");
    case "safe":
      return t("skillLib.finding.safe");
    case "concreteSteps":
      return t("skillLib.finding.concreteSteps");
  }
}

/** How much more (or less, negative) the runs with the skill spent; null when unknown. */
export function costPercent(r: TrialResult): number | null {
  const percent = (withIt: number, without: number) =>
    withIt > 0 && without > 0 ? Math.trunc(((withIt - without) * 100) / without) : null;
  return percent(r.tokensWith, r.tokensWithout) ?? percent(r.costWithMicros, r.costWithoutMicros);
}

/** A trial's result in a few sentences, as the CLI says it. */
export function trialSummary(r: TrialResult, t: Translate): string {
  const lines = [t("skillLib.trial.summary", { with: r.passedWith, without: r.passedWithout, n: r.tasks })];
  const cost = costPercent(r);
  if (cost !== null) {
    lines.push(
      cost > 0
        ? t("skillLib.trial.more", { p: cost })
        : cost < 0
          ? t("skillLib.trial.less", { p: -cost })
          : t("skillLib.trial.same"),
    );
  }
  if (!r.checked) lines.push(t("skillLib.trial.unchecked"));
  if (r.used === 0) lines.push(t("skillLib.trial.unused"));
  if (r.restored === 1) lines.push(t("skillLib.trial.restoredOne"));
  if (r.restored > 1) lines.push(t("skillLib.trial.restoredMany", { n: r.restored }));
  return lines.join(" ");
}

export const voidKey: Record<TrialVoid, MessageKey> = {
  stopped: "skillLib.trial.void.stopped",
  limit: "skillLib.trial.void.limit",
  notRun: "skillLib.trial.void.notRun",
  leaked: "skillLib.trial.void.leaked",
  nothingPassed: "skillLib.trial.void.nothingPassed",
};
