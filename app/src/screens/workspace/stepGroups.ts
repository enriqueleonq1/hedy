// Runs of tool steps in the chat, summed up into one line.

import type { TimelineItem } from "../../tasks/timeline";
import type { Translate } from "../../i18n";

export type StepItem = Extract<TimelineItem, { type: "activity" }>;

/** What the chat renders: an item on its own, or a run of consecutive tool steps. */
export type TimelineEntry =
  | { type: "item"; item: TimelineItem; index: number }
  | { type: "group"; key: string; steps: StepItem[]; first: number; last: number };

/**
 * Consecutive tool steps form one group; anything else (text, cards, phases,
 * limits, messages, the end of a turn) stands on its own and ends the run.
 */
export function groupSteps(items: TimelineItem[]): TimelineEntry[] {
  const entries: TimelineEntry[] = [];
  items.forEach((item, index) => {
    const previous = entries.at(-1);
    if (item.type !== "activity") {
      entries.push({ type: "item", item, index });
    } else if (previous?.type === "group" && previous.last === index - 1) {
      previous.steps.push(item);
      previous.last = index;
    } else {
      entries.push({ type: "group", key: item.key, steps: [item], first: index, last: index });
    }
  });
  return entries;
}

/** How the test runs of a group ended: by the latest one, remembering earlier failures. */
export type TestsOutcome = "passed" | "failed" | "recovered";

export interface StepSummary {
  count: number;
  /** Distinct files edited. */
  files: number;
  /** Commands run that were not tests. */
  commands: number;
  tests: TestsOutcome | null;
  /** Steps Hedy blocked. */
  blocked: number;
}

export function summarize(steps: StepItem[]): StepSummary {
  const files = new Set<string>();
  let commands = 0;
  let blocked = 0;
  let tests: TestsOutcome | null = null;
  let anyTestFailed = false;
  for (const step of steps) {
    if (step.step === "edit") files.add(step.detail);
    if (step.step === "command") commands++;
    if (step.step === "blocked") blocked++;
    if (step.step === "tests") {
      const failed = step.tone === "failed";
      anyTestFailed ||= failed;
      tests = failed ? "failed" : anyTestFailed ? "recovered" : "passed";
    }
  }
  return { count: steps.length, files: files.size, commands, tests, blocked };
}

/** One fact of the summary line; failures are shown in the error color. */
export interface SummaryFact {
  text: string;
  failed: boolean;
}

/** "3 pasos", "editó 1 archivo", "tests fallaron"...: the count first, then what matters. */
export function summaryFacts(summary: StepSummary, t: Translate): SummaryFact[] {
  const facts: SummaryFact[] = [
    {
      text: summary.count === 1 ? t("workspace.group.stepsOne") : t("workspace.group.steps", { count: summary.count }),
      failed: false,
    },
  ];
  if (summary.files > 0) {
    facts.push({
      text: summary.files === 1 ? t("workspace.group.editedOne") : t("workspace.group.edited", { count: summary.files }),
      failed: false,
    });
  }
  if (summary.commands > 0) {
    facts.push({
      text: summary.commands === 1 ? t("workspace.group.ranOne") : t("workspace.group.ran", { count: summary.commands }),
      failed: false,
    });
  }
  if (summary.tests === "passed") facts.push({ text: t("workspace.group.testsPassed"), failed: false });
  if (summary.tests === "recovered") facts.push({ text: t("workspace.group.testsRecovered"), failed: false });
  if (summary.tests === "failed") facts.push({ text: t("workspace.group.testsFailed"), failed: true });
  if (summary.blocked > 0) {
    facts.push({
      text: summary.blocked === 1 ? t("workspace.group.blockedOne") : t("workspace.group.blocked", { count: summary.blocked }),
      failed: true,
    });
  }
  return facts;
}
