// Design tokens (spec §5). Single source of truth: applied as CSS variables by
// `applyTheme` and checked for WCAG AA contrast in `tokens.test.ts`.

import type { MessageKey } from "../i18n";

export type ThemeName = "dark" | "light";

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

export const themes: Record<ThemeName, Record<string, string>> = {
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
    selection: "#283457",
    diffAddBg: "#20303B",
    diffAddInk: "#9ECE6A",
    diffDelBg: "#37222C",
    diffDelInk: "#F7768E",
    warnBg: "#2E2A1F",
    warnInk: "#E0AF68",
    warnLine: "#5C4A2B",
    // Semantic roles shared by both themes.
    action: tokyoNight.blue,
    actionInk: "#1A1B26",
    actionText: tokyoNight.blue,
    accent: tokyoNight.magenta,
    accentSoft: "#2D2640",
    modeBg: tokyoNight.magenta,
    modeInk: "#1A1B26",
    tipBg: "#2A2440",
    success: tokyoNight.green,
    activity: tokyoNight.cyan,
    warning: tokyoNight.yellow,
    danger: tokyoNight.red,
    focus: tokyoNight.cyan,
    // The mascot's eyes and mouth: dark ink in both themes, as in the identity.
    face: "#1A1B26",
    cheek: "#F7768E",
    tongue: "#FF7AB6",
    star: tokyoNight.yellow,
  },
  light: {
    // "Burbuja": warm cream, violet and pink.
    bg: "#FFF8F1",
    panel: "#FFFFFF",
    panel2: "#FFFCF8",
    ink: "#2B2340",
    muted: "#625A76",
    line: "#EFE3D6",
    lineStrong: "#DDCBB8",
    shadowColor: "#2B23402B",
    scrim: "#2B23405C",
    selection: "#EDE7FF",
    diffAddBg: "#E8FAF1",
    diffAddInk: "#116A43",
    diffDelBg: "#FFE9EE",
    diffDelInk: "#A1213F",
    warnBg: "#FFF4D6",
    warnInk: "#7A5300",
    warnLine: "#F2D58A",
    // A slightly deeper violet than the brand's #7C5CFF so white text on buttons passes AA.
    action: "#6847F5",
    actionInk: "#FFFFFF",
    actionText: "#4B32C8",
    accent: "#4B32C8",
    accentSoft: "#EDE7FF",
    modeBg: "#6847F5",
    modeInk: "#FFFFFF",
    tipBg: "#FFE3EC",
    success: "#116A43",
    activity: "#0B6A80",
    warning: "#7A5300",
    danger: "#A1213F",
    focus: "#7C5CFF",
    face: "#2B2340",
    cheek: "#F7768E",
    tongue: "#FF7AB6",
    star: "#E0AF68",
  },
};

/** `panel2` → `--panel-2`, `diffAddBg` → `--diff-add-bg`. */
export function cssVar(token: string): string {
  return `--${token.replace(/([A-Z0-9])/g, (c) => `-${c.toLowerCase()}`)}`;
}

/** Sets the theme's tokens on `root`, which owns its inline custom properties: others are removed. */
export function applyTheme(name: ThemeName, root: HTMLElement = document.documentElement): void {
  root.dataset.theme = name;
  root.style.colorScheme = name;
  const tokens = new Map(Object.entries(themes[name]).map(([token, value]) => [cssVar(token), value]));
  const stale = Array.from({ length: root.style.length }, (_, i) => root.style.item(i)).filter(
    (property) => property.startsWith("--") && !tokens.has(property),
  );
  for (const property of stale) root.style.removeProperty(property);
  for (const [property, value] of tokens) root.style.setProperty(property, value);
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

/** xterm.js colors for each theme. */
export function terminalTheme(name: ThemeName) {
  const t = themes[name];
  const dark = name === "dark";
  return {
    background: t.panel,
    foreground: t.ink,
    cursor: dark ? tokyoNight.cyan : "#7C5CFF",
    selectionBackground: t.selection,
    black: dark ? "#15161E" : "#2B2340",
    red: dark ? tokyoNight.red : "#A1213F",
    green: dark ? tokyoNight.green : "#116A43",
    yellow: dark ? tokyoNight.yellow : "#7A5300",
    blue: dark ? tokyoNight.blue : "#3D5BD9",
    magenta: dark ? tokyoNight.magenta : "#7C3AED",
    cyan: dark ? tokyoNight.cyan : "#0B6A80",
    white: dark ? "#A9B1D6" : "#625A76",
    brightBlack: dark ? "#414868" : "#625A76",
    brightWhite: dark ? "#C0CAF5" : "#2B2340",
  };
}
