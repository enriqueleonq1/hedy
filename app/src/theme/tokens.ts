// Design tokens (spec §5). Single source of truth: applied as CSS variables by
// `applyTokens` and checked for WCAG AA contrast (`repair.ts`, `tokens.test.ts`).
// Other themes (presets, imported, described) are built from a seed by
// `derive.ts` into this same set of tokens.

import type { ThemeBase } from "../api/types";
import type { MessageKey } from "../i18n";

/** A theme is dark or light: it decides `color-scheme` and which mode setting uses it. */
export type { ThemeBase };
/** The base of the theme on screen (kept for the views that only need dark or light). */
export type ThemeName = ThemeBase;
/** Every token of a theme, by name (`panel2`, `diffAddBg`, `synKeyword`…), as hex. */
export type ThemeTokens = Record<string, string>;

/** Raw palette of the dark theme (Tokyo Night). */
const tokyoNight = {
  blue: "#7AA2F7",
  cyan: "#7DCFFF",
  magenta: "#BB9AF7",
  green: "#9ECE6A",
  orange: "#FF9E64",
  yellow: "#E0AF68",
  red: "#F7768E",
  teal: "#73DACA",
};

/**
 * Blob's violet, the one primary accent of both themes (buttons, switches, the
 * selected option): a little deeper than the brand's #7C5CFF so white text on it passes AA.
 */
const blobViolet = "#6847F5";

/** The themes Hedy shipped first, kept as presets with their exact colors. */
export const classicThemes = {
  dark: {
    bg: "#1A1B26",
    panel: "#16161E",
    panel2: "#1F2335",
    ink: "#C0CAF5",
    muted: "#9AA5CE",
    line: "#2F334D",
    // Hovered edges; and the soft shadow and dimmed backdrop of floating surfaces.
    lineStrong: "#414868",
    shadowColor: "#0000006B",
    scrim: "#0B0C1299",
    // A violet tint: the selected row, tab or option.
    selection: "#2E2A4F",
    // Selected text (also inside inputs): clearly visible, ink still readable on it.
    textSelection: "#463F80",
    diffAddBg: "#20303B",
    diffAddInk: "#9ECE6A",
    diffDelBg: "#37222C",
    diffDelInk: "#F7768E",
    // The changed words inside a changed line (a stronger tint of the same hue).
    diffAddEmph: "#2A4640",
    diffDelEmph: "#4D2836",
    warnBg: "#2E2A1F",
    warnInk: "#E0AF68",
    warnLine: "#5C4A2B",
    // Semantic roles shared by both themes. Blue stays for information (`activity`).
    action: blobViolet,
    actionInk: "#FFFFFF",
    // Violet text on dark surfaces: links, the active tab, a meaningful ghost action.
    actionText: "#B4A0FF",
    accent: "#B4A0FF",
    accentSoft: "#2D2640",
    modeBg: blobViolet,
    modeInk: "#FFFFFF",
    tipBg: "#2A2440",
    success: tokyoNight.green,
    activity: tokyoNight.cyan,
    warning: tokyoNight.yellow,
    danger: tokyoNight.red,
    focus: "#9D85FF",
    // The mascot's eyes and mouth: dark ink in both themes, as in the identity.
    face: "#1A1B26",
    cheek: "#F7768E",
    tongue: "#FF7AB6",
    star: tokyoNight.yellow,
    // Syntax: violet keywords, green strings, amber numbers, cyan functions and types.
    synKeyword: "#B4A0FF",
    synString: tokyoNight.green,
    synNumber: tokyoNight.yellow,
    synFunction: tokyoNight.cyan,
    synType: tokyoNight.cyan,
    synComment: "#9AA5CE",
    synTag: tokyoNight.red,
    // The terminal's ANSI colors that are not a semantic token already.
    termCursor: tokyoNight.cyan,
    termBlack: "#15161E",
    termYellow: tokyoNight.yellow,
    termBlue: tokyoNight.blue,
    termMagenta: tokyoNight.magenta,
    termWhite: "#A9B1D6",
    termBrightBlack: "#414868",
  },
  light: {
    // "Burbuja": warm cream, violet and pink.
    bg: "#FFF8F1",
    panel: "#FFFFFF",
    // Nested surfaces (a note inside a card): set apart by tone, not by a border.
    panel2: "#F9F3EB",
    ink: "#2B2340",
    muted: "#625A76",
    line: "#EFE3D6",
    lineStrong: "#DDCBB8",
    shadowColor: "#2B23402B",
    scrim: "#2B23405C",
    selection: "#EDE7FF",
    textSelection: "#CBBEFF",
    diffAddBg: "#E8FAF1",
    diffAddInk: "#116A43",
    diffDelBg: "#FFE9EE",
    diffDelInk: "#A1213F",
    diffAddEmph: "#BDEBD3",
    diffDelEmph: "#FFCCD6",
    warnBg: "#FFF4D6",
    warnInk: "#8A5200",
    warnLine: "#F2D58A",
    action: blobViolet,
    actionInk: "#FFFFFF",
    actionText: "#4B32C8",
    accent: "#4B32C8",
    accentSoft: "#EDE7FF",
    modeBg: blobViolet,
    modeInk: "#FFFFFF",
    tipBg: "#FFE3EC",
    success: "#116A43",
    activity: "#0B6A80",
    // A clear amber (not olive brown) that still reads as text on white and cream.
    warning: "#8A5200",
    danger: "#A1213F",
    focus: "#7C5CFF",
    face: "#2B2340",
    cheek: "#F7768E",
    tongue: "#FF7AB6",
    star: "#E0AF68",
    synKeyword: "#4B32C8",
    synString: "#116A43",
    synNumber: "#8A5200",
    synFunction: "#0B6A80",
    synType: "#0B6A80",
    synComment: "#625A76",
    synTag: "#A1213F",
    termCursor: "#7C5CFF",
    termBlack: "#2B2340",
    termYellow: "#7A5300",
    termBlue: "#3D5BD9",
    termMagenta: "#7C3AED",
    termWhite: "#625A76",
    termBrightBlack: "#625A76",
  },
} satisfies Record<ThemeBase, ThemeTokens>;

/**
 * Hedy's own two themes, the default of each mode: near-black (or white)
 * neutrals and one light, Blob's violet, for the main action, focus and
 * selection only. Status colors are Tokyo Night's, as before.
 */
export const themes = {
  dark: {
    bg: "#0A0A0C",
    panel: "#0F0F12",
    // Nested surfaces and hovered rows: set apart by tone, not by a border.
    panel2: "#18181D",
    ink: "#EDEDF0",
    muted: "#A1A1AA",
    line: "#232329",
    lineStrong: "#34343D",
    shadowColor: "#00000099",
    scrim: "#000000A6",
    selection: "#1E1A33",
    textSelection: "#3B2F70",
    diffAddBg: "#12241A",
    diffAddInk: "#9ECE6A",
    diffDelBg: "#2A151B",
    diffDelInk: "#F7768E",
    diffAddEmph: "#1C3A26",
    diffDelEmph: "#4A1F2A",
    warnBg: "#2B2418",
    warnInk: "#E0AF68",
    warnLine: "#5C4A2B",
    action: blobViolet,
    actionInk: "#FFFFFF",
    actionText: "#B4A0FF",
    accent: "#B4A0FF",
    accentSoft: "#1E1A33",
    modeBg: blobViolet,
    modeInk: "#FFFFFF",
    tipBg: "#1E1A33",
    success: tokyoNight.green,
    activity: tokyoNight.cyan,
    warning: tokyoNight.yellow,
    danger: tokyoNight.red,
    focus: "#8E78FF",
    face: "#1A1B26",
    cheek: "#F7768E",
    tongue: "#FF7AB6",
    star: tokyoNight.yellow,
    synKeyword: "#B4A0FF",
    synString: tokyoNight.green,
    synNumber: tokyoNight.yellow,
    synFunction: tokyoNight.cyan,
    synType: tokyoNight.cyan,
    synComment: "#A1A1AA",
    synTag: tokyoNight.red,
    termCursor: tokyoNight.cyan,
    termBlack: "#1C1C22",
    termYellow: tokyoNight.yellow,
    termBlue: tokyoNight.blue,
    termMagenta: tokyoNight.magenta,
    termWhite: "#A1A1AA",
    termBrightBlack: "#52525B",
  },
  light: {
    bg: "#FFFFFF",
    panel: "#FAFAFA",
    panel2: "#F4F4F5",
    ink: "#18181B",
    muted: "#52525B",
    line: "#E4E4E7",
    lineStrong: "#D4D4D8",
    shadowColor: "#18181B1F",
    scrim: "#18181B59",
    selection: "#EEEAFE",
    textSelection: "#D5CCFF",
    diffAddBg: "#E8FAF1",
    diffAddInk: "#116A43",
    diffDelBg: "#FFE9EE",
    diffDelInk: "#A1213F",
    diffAddEmph: "#BDEBD3",
    diffDelEmph: "#FFCCD6",
    warnBg: "#FFF4D6",
    warnInk: "#8A5200",
    warnLine: "#F2D58A",
    action: blobViolet,
    actionInk: "#FFFFFF",
    actionText: "#4B32C8",
    accent: "#4B32C8",
    accentSoft: "#EEEAFE",
    modeBg: blobViolet,
    modeInk: "#FFFFFF",
    tipBg: "#EEEAFE",
    success: "#116A43",
    activity: "#0B6A80",
    warning: "#8A5200",
    danger: "#A1213F",
    focus: "#7C5CFF",
    face: "#18181B",
    cheek: "#F7768E",
    tongue: "#FF7AB6",
    star: "#E0AF68",
    synKeyword: "#4B32C8",
    synString: "#116A43",
    synNumber: "#8A5200",
    synFunction: "#0B6A80",
    synType: "#0B6A80",
    synComment: "#52525B",
    synTag: "#A1213F",
    termCursor: "#7C5CFF",
    termBlack: "#18181B",
    termYellow: "#7A5300",
    termBlue: "#3D5BD9",
    termMagenta: "#7C3AED",
    termWhite: "#52525B",
    termBrightBlack: "#52525B",
  },
} satisfies Record<ThemeBase, ThemeTokens>;

/** `panel2` → `--panel-2`, `diffAddBg` → `--diff-add-bg`. */
export function cssVar(token: string): string {
  return `--${token.replace(/([A-Z0-9])/g, (c) => `-${c.toLowerCase()}`)}`;
}

/** Sets one of Hedy's own themes on `root` (see `applyTokens`). */
export function applyTheme(name: ThemeBase, root: HTMLElement = document.documentElement): void {
  applyTokens(themes[name], name, root);
}

/** Sets a theme's tokens on `root`, which owns its inline custom properties: others are removed. */
export function applyTokens(tokens: ThemeTokens, base: ThemeBase, root: HTMLElement = document.documentElement): void {
  root.dataset.theme = base;
  root.style.colorScheme = base;
  const vars = new Map(Object.entries(tokens).map(([token, value]) => [cssVar(token), value]));
  const stale = Array.from({ length: root.style.length }, (_, i) => root.style.item(i)).filter(
    (property) => property.startsWith("--") && !vars.has(property),
  );
  for (const property of stale) root.style.removeProperty(property);
  for (const [property, value] of vars) root.style.setProperty(property, value);
}

/** A theme's tokens as inline CSS variables, to paint one element (a theme card) in it. */
export function tokenStyle(tokens: ThemeTokens): Record<string, string> {
  return Object.fromEntries(Object.entries(tokens).map(([token, value]) => [cssVar(token), value]));
}

/** Account colors offered to the user (spec §5), with the key of their name. */
export const accountPalette: { label: MessageKey; hex: string }[] = [
  { label: "color.blue", hex: tokyoNight.blue },
  { label: "color.purple", hex: tokyoNight.magenta },
  { label: "color.green", hex: tokyoNight.green },
  { label: "color.orange", hex: tokyoNight.orange },
  { label: "color.cyan", hex: tokyoNight.cyan },
  { label: "color.pink", hex: "#FF7AB6" },
  { label: "color.yellow", hex: tokyoNight.yellow },
  { label: "color.teal", hex: tokyoNight.teal },
];

// Mascot colors (fills, never text) where there is no account to take one from.
/** The first color of the palette; also the color of a new account. */
export const DEFAULT_ACCOUNT_COLOR = tokyoNight.blue;
/** Hedy's tip in the sidebar. */
export const TIP_MASCOT_COLOR = tokyoNight.magenta;
/** A limit warning about an unknown account. */
export const WARNING_MASCOT_COLOR = tokyoNight.yellow;
/** The app could not start. */
export const ERROR_MASCOT_COLOR = tokyoNight.red;

/** xterm.js colors of a theme: its semantic tokens plus its `term*` ones. */
export function terminalTheme(t: ThemeTokens) {
  return {
    background: t.panel,
    foreground: t.ink,
    cursor: t.termCursor ?? t.focus,
    selectionBackground: t.textSelection,
    black: t.termBlack ?? t.panel,
    red: t.danger,
    green: t.success,
    yellow: t.termYellow ?? t.warning,
    blue: t.termBlue ?? t.activity,
    magenta: t.termMagenta ?? t.accent,
    cyan: t.activity,
    white: t.termWhite ?? t.muted,
    brightBlack: t.termBrightBlack ?? t.muted,
    brightWhite: t.ink,
  };
}
