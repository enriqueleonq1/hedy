import { contrast } from "./contrast";
import { PRESETS, presetById } from "./presets";
import { contrastFailures, FOCUS_RATIO, repairContrast, TEXT_PAIRS } from "./repair";
import { accountPalette, applyTheme, applyTokens, classicThemes, cssVar, terminalTheme, themes, type ThemeName, type ThemeTokens } from "./tokens";

describe.each(Object.keys(themes) as ThemeName[])("%s theme", (name) => {
  const t: ThemeTokens = themes[name];

  it.each(TEXT_PAIRS.map(([fg, bg]) => [fg, bg]))("%s on %s passes AA", (fg, bg) => {
    expect(t[fg], fg).toBeDefined();
    expect(t[bg], bg).toBeDefined();
    expect(contrast(t[fg]!, t[bg]!)).toBeGreaterThanOrEqual(4.5);
  });

  it("focus ring is visible against the background (3:1)", () => {
    expect(contrast(t.focus!, t.bg!)).toBeGreaterThanOrEqual(3);
  });

  // Cards inside cards have no border: their tone alone must tell them apart.
  it("nested surfaces stand apart from the card and the page", () => {
    expect(contrast(t.panel!, t.panel2!)).toBeGreaterThanOrEqual(1.05);
    expect(contrast(t.bg!, t.panel2!)).toBeGreaterThanOrEqual(1.04);
  });

  it("applies every token as a CSS variable", () => {
    const root = document.createElement("div");
    applyTheme(name, root);
    expect(root.dataset.theme).toBe(name);
    expect(root.style.getPropertyValue("--panel-2")).toBe(t.panel2);
    expect(root.style.getPropertyValue("--diff-add-bg")).toBe(t.diffAddBg);
    expect(root.style.getPropertyValue("--syn-keyword")).toBe(t.synKeyword);
  });

  it("removes variables the theme does not have", () => {
    const root = document.createElement("div");
    root.style.setProperty("--old-token", "#000000");
    applyTheme(name, root);
    expect(root.style.getPropertyValue("--old-token")).toBe("");
  });
});

it("both themes define the same tokens", () => {
  expect(Object.keys(themes.light).sort()).toEqual(Object.keys(themes.dark).sort());
});

// Hedy's own themes: near-black or white neutrals and one light; syntax colors
// are the semantic ones.
it("uses Hedy's own themes by default", () => {
  expect(presetById("hedy")?.tokens).toBe(themes.dark);
  expect(presetById("hedy-light")?.tokens).toBe(themes.light);
  expect([themes.dark.bg, themes.dark.ink, themes.dark.action]).toEqual(["#0A0A0C", "#EDEDF0", "#6847F5"]);
  expect([themes.light.bg, themes.light.ink, themes.light.action]).toEqual(["#FFFFFF", "#18181B", "#6847F5"]);
  for (const t of [themes.dark, themes.light]) {
    expect([t.synKeyword, t.synString, t.synNumber, t.synFunction, t.synType, t.synComment, t.synTag]).toEqual([
      t.accent,
      t.success,
      t.warning,
      t.activity,
      t.activity,
      t.muted,
      t.danger,
    ]);
  }
});

// The themes Hedy shipped first stay as presets, exactly as they were.
it("keeps Tokyo Night and Burbuja as they were", () => {
  expect(presetById("tokyo-night")?.tokens).toBe(classicThemes.dark);
  expect(presetById("burbuja")?.tokens).toBe(classicThemes.light);
  const d = classicThemes.dark;
  expect([d.bg, d.panel, d.panel2, d.ink, d.muted, d.accent, d.action, d.selection]).toEqual([
    "#1A1B26",
    "#16161E",
    "#1F2335",
    "#C0CAF5",
    "#9AA5CE",
    "#B4A0FF",
    "#6847F5",
    "#2E2A4F",
  ]);
  const l = classicThemes.light;
  expect([l.bg, l.panel, l.panel2, l.ink, l.muted, l.accent, l.action, l.tipBg]).toEqual([
    "#FFF8F1",
    "#FFFFFF",
    "#F9F3EB",
    "#2B2340",
    "#625A76",
    "#4B32C8",
    "#6847F5",
    "#FFE3EC",
  ]);
  for (const t of [d, l]) {
    expect([t.synKeyword, t.synString, t.synNumber, t.synFunction, t.synType, t.synComment, t.synTag]).toEqual([
      t.accent,
      t.success,
      t.warning,
      t.activity,
      t.activity,
      t.muted,
      t.danger,
    ]);
  }
  expect(terminalTheme(d)).toMatchObject({ cursor: "#7DCFFF", black: "#15161E", blue: "#7AA2F7", white: "#A9B1D6", brightWhite: "#C0CAF5" });
  expect(terminalTheme(l)).toMatchObject({ cursor: "#7C5CFF", yellow: "#7A5300", magenta: "#7C3AED", brightBlack: "#625A76" });
});

describe.each(PRESETS.map((p) => [p.id, p] as const))("preset %s", (_id, preset) => {
  it("has every token of Hedy's themes", () => {
    expect(Object.keys(preset.tokens).sort()).toEqual(Object.keys(themes.dark).sort());
    for (const value of Object.values(preset.tokens)) expect(value).toMatch(/^#[0-9A-F]{6}([0-9A-F]{2})?$/);
  });

  it("passes every contrast check on its own", () => {
    expect(contrastFailures(preset.tokens)).toEqual([]);
    expect(repairContrast(preset.tokens, preset.base).fixes).toEqual([]);
    expect(contrast(preset.tokens.focus!, preset.tokens.bg!)).toBeGreaterThanOrEqual(FOCUS_RATIO);
  });
});

it("lists Hedy's themes, its classic ones and eight more, dark and light", () => {
  expect(PRESETS.map((p) => p.id)).toEqual([
    "hedy",
    "tokyo-night",
    "one-dark-pro",
    "dracula",
    "catppuccin-mocha",
    "nord",
    "github-dark",
    "hedy-light",
    "burbuja",
    "github-light",
    "catppuccin-latte",
    "solarized-light",
  ]);
  expect(PRESETS.filter((p) => p.base === "light")).toHaveLength(5);
});

it("sets the color scheme of the theme's base", () => {
  const root = document.createElement("div");
  applyTokens(presetById("github-light")!.tokens, "light", root);
  expect(root.style.colorScheme).toBe("light");
  expect(root.style.getPropertyValue("--bg")).toBe("#FFFFFF");
});

it("account colors stand out on the dark background (3:1, non-text)", () => {
  for (const { hex } of accountPalette) {
    expect(contrast(hex, themes.dark.bg!)).toBeGreaterThanOrEqual(3);
  }
});

it("converts token names to CSS variables", () => {
  expect(cssVar("panel2")).toBe("--panel-2");
  expect(cssVar("warnInk")).toBe("--warn-ink");
  expect(cssVar("synKeyword")).toBe("--syn-keyword");
});
