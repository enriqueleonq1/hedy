// A whole theme from a few colors. A seed names what defines a theme (page,
// text, accent, syntax); everything else (selection, diff tints, warnings,
// focus, the terminal…) is mixed from those, so imported and described themes
// end up with the same tokens as Hedy's own.

import { contrast } from "./contrast";
import { mix, toHex, parseHex, withAlpha } from "./color";
import { repairContrast, type Repaired } from "./repair";
import { themes, type ThemeBase, type ThemeTokens } from "./tokens";

/** The colors a theme is built from; only `bg`, `ink` and `accent` are required. */
export const SEED_KEYS = [
  "bg",
  "panel",
  "panel2",
  "ink",
  "muted",
  "line",
  "accent",
  "action",
  "success",
  "warning",
  "danger",
  "info",
  "keyword",
  "string",
  "number",
  "function",
  "type",
  "comment",
  // Optional tints of a diff's lines (VS Code themes have them).
  "diffAddBg",
  "diffDelBg",
] as const;

export type SeedKey = (typeof SEED_KEYS)[number];
export type ThemeSeed = Partial<Record<SeedKey, string>> & { bg: string; ink: string; accent: string };

const WHITE = "#FFFFFF";
const BLACK = "#000000";

/** Hedy's own colors for what a seed leaves out. */
const FALLBACK: Record<ThemeBase, Record<"success" | "warning" | "danger" | "info", string>> = {
  dark: { success: themes.dark.success, warning: themes.dark.warning, danger: themes.dark.danger, info: themes.dark.activity },
  light: { success: themes.light.success, warning: themes.light.warning, danger: themes.light.danger, info: themes.light.activity },
};

/** Keeps the keys that hold a color, as `#RRGGBB`. */
export function cleanSeed(colors: Record<string, unknown>): Partial<Record<SeedKey, string>> {
  const seed: Partial<Record<SeedKey, string>> = {};
  for (const key of SEED_KEYS) {
    const value = colors[key];
    const color = typeof value === "string" ? parseHex(value) : null;
    if (color) seed[key] = toHex(color);
  }
  return seed;
}

/** A seed from loose colors (an AI's answer, a file), or null when `bg`, `ink` or `accent` is missing. */
export function asSeed(colors: Record<string, unknown>): ThemeSeed | null {
  const seed = cleanSeed(colors);
  return seed.bg && seed.ink && seed.accent ? (seed as ThemeSeed) : null;
}

/** A seed made into a whole theme that reads well (see `repairContrast`). */
export function buildTheme(seed: ThemeSeed, base: ThemeBase): Repaired {
  return repairContrast(deriveTokens(seed, base), base);
}

/** The ink that reads best on `background`: white when it passes AA, else the theme's dark ink. */
function inkOn(background: string, dark: string): string {
  if (contrast(WHITE, background) >= 4.5) return WHITE;
  if (contrast(dark, background) >= 4.5) return dark;
  return contrast(WHITE, background) >= contrast(BLACK, background) ? WHITE : BLACK;
}

/** Builds the full token map of a theme from its seed (not yet checked for contrast: see `repairContrast`). */
export function deriveTokens(input: ThemeSeed, base: ThemeBase): ThemeTokens {
  const seed = asSeed(input);
  if (!seed) throw new Error("a theme seed needs bg, ink and accent");
  const dark = base === "dark";
  const { bg, ink, accent } = seed;
  const toward = (amount: number) => mix(bg, ink, amount);

  const panel = seed.panel ?? (dark ? mix(bg, BLACK, 0.18) : mix(bg, WHITE, 0.7));
  const panel2 = seed.panel2 ?? toward(dark ? 0.06 : 0.04);
  const line = seed.line ?? toward(dark ? 0.14 : 0.12);
  const muted = seed.muted ?? mix(ink, bg, 0.3);
  const action = seed.action ?? accent;
  const actionInk = inkOn(action, dark ? bg : ink);
  const success = seed.success ?? FALLBACK[base].success;
  const warning = seed.warning ?? FALLBACK[base].warning;
  const danger = seed.danger ?? FALLBACK[base].danger;
  const info = seed.info ?? FALLBACK[base].info;
  const tint = (color: string, amount: number) => mix(panel, color, amount);

  return {
    bg,
    panel,
    panel2,
    ink,
    muted,
    line,
    lineStrong: toward(dark ? 0.24 : 0.22),
    shadowColor: dark ? "#0000006B" : withAlpha(ink, 0.17),
    scrim: dark ? withAlpha(mix(bg, BLACK, 0.55), 0.6) : withAlpha(ink, 0.36),
    selection: tint(accent, dark ? 0.1 : 0.06),
    textSelection: mix(bg, accent, 0.22),
    diffAddBg: seed.diffAddBg ?? tint(success, dark ? 0.1 : 0.09),
    diffAddInk: dark ? success : mix(success, ink, 0.15),
    diffDelBg: seed.diffDelBg ?? tint(danger, dark ? 0.1 : 0.08),
    diffDelInk: mix(danger, ink, 0.15),
    diffAddEmph: tint(success, dark ? 0.18 : 0.15),
    diffDelEmph: tint(danger, dark ? 0.16 : 0.12),
    warnBg: tint(warning, 0.1),
    warnInk: dark ? warning : mix(warning, ink, 0.2),
    warnLine: tint(warning, dark ? 0.35 : 0.45),
    action,
    actionInk,
    actionText: accent,
    accent,
    accentSoft: mix(bg, accent, dark ? 0.12 : 0.1),
    modeBg: action,
    modeInk: actionInk,
    tipBg: tint(accent, dark ? 0.12 : 0.1),
    success,
    activity: info,
    warning,
    danger,
    focus: accent,
    face: themes[base].face,
    cheek: themes[base].cheek,
    tongue: themes[base].tongue,
    star: themes[base].star,
    synKeyword: seed.keyword ?? accent,
    synString: seed.string ?? success,
    synNumber: seed.number ?? warning,
    synFunction: seed.function ?? info,
    synType: seed.type ?? seed.function ?? info,
    synComment: seed.comment ?? muted,
    synTag: danger,
    termCursor: accent,
    termBlack: dark ? mix(panel, BLACK, 0.2) : ink,
    termYellow: warning,
    termBlue: seed.function ?? info,
    termMagenta: seed.keyword ?? accent,
    termWhite: dark ? mix(ink, bg, 0.15) : muted,
    termBrightBlack: dark ? toward(0.3) : muted,
  };
}
