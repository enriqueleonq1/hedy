import { contrast } from "./contrast";
import { accountPalette, applyTheme, cssVar, themes, type ThemeName } from "./tokens";

// Text/background pairs used by the UI. All must pass WCAG AA (4.5:1).
const textPairs: [string, string][] = [
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
];

describe.each(Object.keys(themes) as ThemeName[])("%s theme", (name) => {
  const t = themes[name];

  it.each(textPairs)("%s on %s passes AA", (fg, bg) => {
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

it("account colors stand out on the dark background (3:1, non-text)", () => {
  for (const { hex } of accountPalette) {
    expect(contrast(hex, themes.dark.bg!)).toBeGreaterThanOrEqual(3);
  }
});

it("converts token names to CSS variables", () => {
  expect(cssVar("panel2")).toBe("--panel-2");
  expect(cssVar("warnInk")).toBe("--warn-ink");
});
