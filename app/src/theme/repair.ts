// Contrast that every theme Hedy shows must meet (presets, imported, described):
// the text/background pairs the UI uses pass WCAG AA, the focus ring stands out,
// nested surfaces are told apart. `repairContrast` moves what fails just enough.

import { lightness, withLightness } from "./color";
import { contrast, luminance } from "./contrast";
import type { ThemeBase, ThemeTokens } from "./tokens";

/** Text/background pairs used by the UI. All must pass WCAG AA (4.5:1). */
export const TEXT_PAIRS: readonly (readonly [fg: string, bg: string])[] = [
  ["ink", "bg"],
  ["ink", "panel"],
  ["ink", "panel2"],
  ["muted", "bg"],
  ["muted", "panel"],
  ["muted", "panel2"],
  ["actionInk", "action"],
  ["modeInk", "modeBg"],
  ["actionText", "selection"],
  // The active option of a select or menu: its label and its second line.
  ["ink", "selection"],
  ["muted", "selection"],
  ["ink", "textSelection"],
  ["ink", "tipBg"],
  ["actionText", "bg"],
  ["actionText", "panel"],
  ["accent", "bg"],
  ["accent", "panel"],
  ["success", "panel"],
  ["activity", "panel"],
  ["warning", "panel"],
  // Usage near a limit, on the page and inside a card's nested surface.
  ["warning", "bg"],
  ["warning", "panel2"],
  ["danger", "panel"],
  ["warnInk", "warnBg"],
  ["diffAddInk", "diffAddBg"],
  ["diffDelInk", "diffDelBg"],
  // A proposed change to a note: before and after.
  ["ink", "diffAddBg"],
  ["muted", "diffAddBg"],
  ["muted", "diffDelBg"],
  // Changed words inside a changed line of a diff.
  ["ink", "diffAddEmph"],
  ["diffAddInk", "diffAddEmph"],
  ["ink", "diffDelEmph"],
  ["diffDelInk", "diffDelEmph"],
  // A changed file's status letter, also on the selected row.
  ["success", "selection"],
  ["danger", "selection"],
  ["activity", "selection"],
  ["accent", "selection"],
  ["accent", "panel2"],
  // A ghost button inside a nested surface.
  ["actionText", "panel2"],
  // The palette's active row: changed file's status letters.
  ["warnInk", "selection"],
  ["warnInk", "panel"],
  // Code in the editor and in diffs (both on `panel`).
  ["synKeyword", "panel"],
  ["synString", "panel"],
  ["synNumber", "panel"],
  ["synFunction", "panel"],
  ["synType", "panel"],
  ["synComment", "panel"],
  ["synTag", "panel"],
];

export const AA = 4.5;
/** Non-text contrast (WCAG 1.4.11): the focus ring against the page. */
export const FOCUS_RATIO = 3;
/** Cards inside cards have no border: their tone alone tells them apart. */
export const NESTED_PANEL_RATIO = 1.05;
export const NESTED_BG_RATIO = 1.04;

export interface ContrastFix {
  token: string;
  /** The background it was checked against when it first failed. */
  against: string;
  from: string;
  to: string;
  /** Ratio against `against`, before and after. */
  before: number;
  after: number;
}

export interface Repaired {
  tokens: ThemeTokens;
  fixes: ContrastFix[];
}

/** Every check a theme must pass, as [fg, bg, minimum ratio]. */
export function contrastChecks(): [string, string, number][] {
  return [
    ...TEXT_PAIRS.map(([fg, bg]) => [fg, bg, AA] as [string, string, number]),
    ["focus", "bg", FOCUS_RATIO],
    ["panel2", "panel", NESTED_PANEL_RATIO],
    ["panel2", "bg", NESTED_BG_RATIO],
  ];
}

/** The checks a theme fails, with their ratios (empty when it reads well everywhere). */
export function contrastFailures(tokens: ThemeTokens): { fg: string; bg: string; ratio: number; min: number }[] {
  return contrastChecks().flatMap(([fg, bg, min]) => {
    const [f, b] = [tokens[fg], tokens[bg]];
    const ratio = f && b ? contrast(f, b) : min;
    return ratio < min ? [{ fg, bg, min, ratio }] : [];
  });
}

const STEP = 0.005;

/**
 * `fg` moved in lightness, away from `bg`, until it reaches `min` against it (or
 * as far as it goes). When that direction runs out, it tries the other one.
 */
function separate(fg: string, bg: string, min: number): string {
  const start = lightness(fg);
  const lighter = luminance(fg) >= luminance(bg);
  for (const up of [lighter, !lighter]) {
    for (let l = start; up ? l <= 1 : l >= 0; l += up ? STEP : -STEP) {
      const candidate = withLightness(fg, l);
      if (contrast(candidate, bg) >= min) return candidate;
    }
  }
  return contrast("#FFFFFF", bg) >= contrast("#000000", bg) ? "#FFFFFF" : "#000000";
}

/**
 * Makes a theme readable: each failing foreground moves its lightness away from
 * its background until it passes (AA for text, 3:1 for the focus ring), and a
 * nested surface (`panel2`) moves away from the card and the page. Backgrounds
 * never move, so the theme keeps its character. Returns what changed.
 */
export function repairContrast(input: ThemeTokens, base: ThemeBase): Repaired {
  const tokens = { ...input };
  const fixes = new Map<string, ContrastFix>();
  const record = (token: string, from: string, against: string, before: number, to: string) => {
    const known = fixes.get(token);
    fixes.set(token, {
      token,
      against: known?.against ?? against,
      from: known?.from ?? from,
      to,
      before: known?.before ?? before,
      after: 0,
    });
    tokens[token] = to;
  };

  // Surfaces first: the text is then checked on the final ones.
  for (let pass = 0; pass < 40; pass++) {
    const nested = tokens.panel2;
    if (!nested) break;
    const failing = (
      [
        ["panel", NESTED_PANEL_RATIO],
        ["bg", NESTED_BG_RATIO],
      ] as const
    )
      .map(([name, min]) => ({ name, other: tokens[name], min }))
      .find(({ other, min }) => other && contrast(nested, other) < min);
    if (!failing?.other) break;
    // Dark themes nest lighter surfaces; light ones nest darker (cream) ones.
    const l = lightness(nested) + (base === "dark" ? 0.01 : -0.01);
    record("panel2", nested, failing.name, contrast(nested, failing.other), withLightness(nested, l));
  }

  const checks = contrastChecks().filter(([fg]) => fg !== "panel2");
  for (let pass = 0; pass < 6; pass++) {
    let changed = false;
    for (const [fg, bg, min] of checks) {
      const [f, b] = [tokens[fg], tokens[bg]];
      if (!f || !b) continue;
      const ratio = contrast(f, b);
      if (ratio >= min) continue;
      record(fg, f, bg, ratio, separate(f, b, min));
      changed = true;
    }
    if (!changed) break;
  }

  for (const fix of fixes.values()) {
    fix.to = tokens[fix.token] ?? fix.to;
    const against = tokens[fix.against];
    fix.after = against ? contrast(fix.to, against) : fix.before;
  }
  return { tokens, fixes: [...fixes.values()] };
}
