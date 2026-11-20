// The themes Hedy ships: its own two (Hedy and Hedy claro, exactly the tokens
// of `tokens.ts`), the classic Tokyo Night and Burbuja it shipped first, and
// well-known editor themes built from a seed.
// Each passes every contrast check on its own (`tokens.test.ts`).

import type { MessageKey } from "../i18n";
import { deriveTokens, type ThemeSeed } from "./derive";
import { classicThemes, themes, type ThemeBase, type ThemeTokens } from "./tokens";

export interface ThemeDef {
  /** A preset's id (`tokyo-night`) or `custom:<id>` for the user's own. */
  id: string;
  name: string;
  /** One line about it (presets). */
  note?: MessageKey;
  base: ThemeBase;
  tokens: ThemeTokens;
}

export const DEFAULT_THEME: Record<ThemeBase, string> = { dark: "hedy", light: "hedy-light" };

const SEEDS: { id: string; name: string; note: MessageKey; base: ThemeBase; seed: ThemeSeed }[] = [
  {
    id: "one-dark-pro",
    name: "One Dark Pro",
    note: "theme.note.oneDarkPro",
    base: "dark",
    seed: {
      bg: "#282C34",
      panel: "#21252B",
      panel2: "#2C313A",
      ink: "#B0B7C3",
      muted: "#9DA5B4",
      line: "#3E4451",
      accent: "#61AFEF",
      action: "#4D78CC",
      keyword: "#C678DD",
      string: "#98C379",
      number: "#D19A66",
      function: "#61AFEF",
      type: "#E5C07B",
      comment: "#8E949F",
      danger: "#E57E86",
      info: "#56B6C2",
      warning: "#E5C07B",
      success: "#98C379",
    },
  },
  {
    id: "dracula",
    name: "Dracula",
    note: "theme.note.dracula",
    base: "dark",
    seed: {
      bg: "#282A36",
      panel: "#21222C",
      panel2: "#2F3142",
      ink: "#F8F8F2",
      muted: "#B6BAD4",
      line: "#44475A",
      accent: "#BD93F9",
      action: "#BD93F9",
      keyword: "#FF79C6",
      string: "#F1FA8C",
      number: "#BD93F9",
      function: "#50FA7B",
      type: "#8BE9FD",
      comment: "#8C98C4",
      danger: "#FF6464",
      success: "#50FA7B",
      warning: "#FFB86C",
      info: "#8BE9FD",
    },
  },
  {
    id: "catppuccin-mocha",
    name: "Catppuccin Mocha",
    note: "theme.note.catppuccinMocha",
    base: "dark",
    seed: {
      bg: "#1E1E2E",
      panel: "#181825",
      panel2: "#28283B",
      ink: "#CDD6F4",
      muted: "#A6ADC8",
      line: "#45475A",
      accent: "#CBA6F7",
      action: "#CBA6F7",
      keyword: "#CBA6F7",
      string: "#A6E3A1",
      number: "#FAB387",
      function: "#89B4FA",
      type: "#F9E2AF",
      comment: "#9399B2",
      danger: "#F38BA8",
      success: "#A6E3A1",
      warning: "#F9E2AF",
      info: "#89DCEB",
    },
  },
  {
    id: "nord",
    name: "Nord",
    note: "theme.note.nord",
    base: "dark",
    seed: {
      bg: "#2E3440",
      panel: "#272C36",
      panel2: "#353C4A",
      ink: "#ECEFF4",
      muted: "#B8C0CF",
      line: "#434C5E",
      accent: "#88C0D0",
      action: "#88C0D0",
      keyword: "#81A1C1",
      string: "#A3BE8C",
      number: "#B48EAD",
      function: "#88C0D0",
      type: "#8FBCBB",
      comment: "#97A3B9",
      danger: "#D7939A",
      success: "#A3BE8C",
      warning: "#EBCB8B",
      info: "#88C0D0",
    },
  },
  {
    id: "github-dark",
    name: "GitHub Dark",
    note: "theme.note.githubDark",
    base: "dark",
    seed: {
      bg: "#0D1117",
      panel: "#010409",
      panel2: "#161B22",
      ink: "#E6EDF3",
      muted: "#9198A1",
      line: "#30363D",
      accent: "#4493F8",
      action: "#238636",
      keyword: "#FF7B72",
      string: "#A5D6FF",
      number: "#79C0FF",
      function: "#D2A8FF",
      type: "#FFA657",
      comment: "#8B949E",
      danger: "#F85149",
      success: "#3FB950",
      warning: "#D29922",
      info: "#58A6FF",
    },
  },
  {
    id: "github-light",
    name: "GitHub Light",
    note: "theme.note.githubLight",
    base: "light",
    seed: {
      bg: "#FFFFFF",
      panel: "#F6F8FA",
      panel2: "#EFF2F5",
      ink: "#1F2328",
      muted: "#59636E",
      line: "#D1D9E0",
      accent: "#0866D6",
      action: "#1F883D",
      keyword: "#CF222E",
      string: "#0A3069",
      number: "#0550AE",
      function: "#8250DF",
      type: "#953800",
      comment: "#59636E",
      danger: "#CF222E",
      success: "#197A34",
      warning: "#946300",
      info: "#0866D6",
    },
  },
  {
    id: "catppuccin-latte",
    name: "Catppuccin Latte",
    note: "theme.note.catppuccinLatte",
    base: "light",
    seed: {
      bg: "#EFF1F5",
      panel: "#FFFFFF",
      panel2: "#E6E9EF",
      ink: "#4C4F69",
      muted: "#5C5F77",
      line: "#CCD0DA",
      accent: "#7029D6",
      action: "#8839EF",
      keyword: "#8839EF",
      string: "#2F7A1F",
      number: "#C2410C",
      function: "#1A62F0",
      type: "#955B07",
      comment: "#6C6F85",
      danger: "#D20F39",
      success: "#2F7A1F",
      warning: "#955B07",
      info: "#1A62F0",
    },
  },
  {
    id: "solarized-light",
    name: "Solarized Light",
    note: "theme.note.solarizedLight",
    base: "light",
    seed: {
      bg: "#FDF6E3",
      panel: "#FFFBF0",
      panel2: "#EEE8D5",
      ink: "#073642",
      muted: "#566B72",
      line: "#E4DCC3",
      accent: "#1D6BA3",
      action: "#1D6BA3",
      keyword: "#697900",
      string: "#1E7F78",
      number: "#C02C6E",
      function: "#1D6BA3",
      type: "#806300",
      comment: "#64767C",
      danger: "#C9302C",
      success: "#5F7000",
      warning: "#806300",
      info: "#1D6BA3",
    },
  },
];

/** Hedy's own themes: the default of each mode, and where a missing or deleted theme falls back. */
export const HEDY_THEMES: Record<ThemeBase, ThemeDef> = {
  dark: { id: DEFAULT_THEME.dark, name: "Hedy Dark", note: "theme.note.hedyDark", base: "dark", tokens: themes.dark },
  light: { id: DEFAULT_THEME.light, name: "Hedy Light", note: "theme.note.hedyLight", base: "light", tokens: themes.light },
};

/** The themes Hedy shipped first, with their exact tokens. */
const CLASSIC: Record<ThemeBase, ThemeDef> = {
  dark: { id: "tokyo-night", name: "Tokyo Night", note: "theme.note.tokyoNight", base: "dark", tokens: classicThemes.dark },
  light: { id: "burbuja", name: "Burbuja", note: "theme.note.burbuja", base: "light", tokens: classicThemes.light },
};

/** Every preset, Hedy's own first, then its classic one; dark ones, then light ones. */
export const PRESETS: ThemeDef[] = [
  HEDY_THEMES.dark,
  CLASSIC.dark,
  ...SEEDS.filter((s) => s.base === "dark").map(build),
  HEDY_THEMES.light,
  CLASSIC.light,
  ...SEEDS.filter((s) => s.base === "light").map(build),
];

function build({ seed, ...rest }: (typeof SEEDS)[number]): ThemeDef {
  return { ...rest, tokens: deriveTokens(seed, rest.base) };
}

export function presetById(id: string): ThemeDef | undefined {
  return PRESETS.find((p) => p.id === id);
}
