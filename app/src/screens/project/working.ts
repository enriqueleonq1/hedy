// A project's ways of working with one AI, as the backend's `domain::working`
// (docs/decisions.md §60): three presets by name, any other set of stops is custom.

import type { WorkingStyle } from "../../api/types";

export type Preset = "now" | "afterOk" | "afterOkReview";

export const PRESETS: Preset[] = ["now", "afterOk", "afterOkReview"];

/** `style` with the preset's stops; its commands stay. */
export function applyPreset(preset: Preset, style: WorkingStyle): WorkingStyle {
  const checks = preset !== "now";
  return { ...style, plan: checks, tests: checks, e2e: checks, review: preset === "afterOkReview" };
}

/** The preset these stops are, or `null` («Personalizada»). */
export function presetOf(style: WorkingStyle): Preset | null {
  return (
    PRESETS.find((p) => {
      const as = applyPreset(p, style);
      return as.plan === style.plan && as.tests === style.tests && as.e2e === style.e2e && as.review === style.review;
    }) ?? null
  );
}
