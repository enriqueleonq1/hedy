import { flatten, mix, parseHex, withAlpha } from "./color";
import { contrast } from "./contrast";
import { asSeed, buildTheme, deriveTokens } from "./derive";
import { contrastFailures, repairContrast } from "./repair";
import { themes } from "./tokens";

describe("colors", () => {
  it("parses short, long and alpha hex", () => {
    expect(parseHex("#abc")).toEqual({ r: 170, g: 187, b: 204, a: 1 });
    expect(parseHex("#FFFFFF80")?.a).toBeCloseTo(0.5, 2);
    expect(parseHex("blue")).toBeNull();
  });

  it("mixes and flattens alpha over a background", () => {
    expect(mix("#000000", "#FFFFFF", 0.5)).toBe("#808080");
    expect(flatten("#FFFFFF80", "#000000")).toBe("#808080");
    expect(withAlpha("#2B2340", 0.17)).toBe("#2B23402B");
  });
});

describe("deriveTokens", () => {
  it("builds every token from bg, ink and accent alone", () => {
    const tokens = deriveTokens({ bg: "#1B2226", ink: "#D5E1E6", accent: "#7CCFC4" }, "dark");
    expect(Object.keys(tokens).sort()).toEqual(Object.keys(themes.dark).sort());
    // Mascot colors stay Hedy's.
    expect(tokens.cheek).toBe(themes.dark.cheek);
    expect(tokens.synKeyword).toBe("#7CCFC4");
  });

  it("puts white on a deep action and dark ink on a pale one", () => {
    expect(deriveTokens({ bg: "#FFFFFF", ink: "#1F2328", accent: "#0866D6" }, "light").actionInk).toBe("#FFFFFF");
    const pale = deriveTokens({ bg: "#282A36", ink: "#F8F8F2", accent: "#BD93F9" }, "dark");
    expect(pale.actionInk).toBe("#282A36");
    expect(contrast(pale.actionInk!, pale.action!)).toBeGreaterThanOrEqual(4.5);
  });

  it("keeps the seed's own surfaces and syntax", () => {
    const tokens = deriveTokens(
      { bg: "#FDF6E3", panel: "#FFFBF0", ink: "#073642", accent: "#1D6BA3", string: "#1E7F78", comment: "#64767C" },
      "light",
    );
    expect([tokens.panel, tokens.synString, tokens.synComment]).toEqual(["#FFFBF0", "#1E7F78", "#64767C"]);
  });

  it("refuses a seed without bg, ink or accent", () => {
    expect(asSeed({ bg: "#000000", ink: "#FFFFFF" })).toBeNull();
    expect(asSeed({ bg: "#000", ink: "#fff", accent: "#f0a", extra: "#123456" })).toEqual({
      bg: "#000000",
      ink: "#FFFFFF",
      accent: "#FF00AA",
    });
  });
});

describe("repairContrast", () => {
  it("moves failing text away from its background and says what changed", () => {
    const tokens = deriveTokens({ bg: "#1E1E1E", ink: "#D4D4D4", muted: "#5A5A5A", accent: "#3A6EA5" }, "dark");
    expect(contrastFailures(tokens).length).toBeGreaterThan(0);
    const { tokens: fixed, fixes } = repairContrast(tokens, "dark");
    expect(contrastFailures(fixed)).toEqual([]);
    const muted = fixes.find((f) => f.token === "muted");
    expect(muted).toMatchObject({ from: "#5A5A5A" });
    expect(muted!.before).toBeLessThan(4.5);
    expect(muted!.after).toBeGreaterThanOrEqual(4.5);
    // Lighter, as it sits on a dark background; backgrounds never move.
    expect(contrast(muted!.to, "#000000")).toBeGreaterThan(contrast(muted!.from, "#000000"));
    expect(fixed.bg).toBe(tokens.bg);
    expect(fixed.panel).toBe(tokens.panel);
  });

  it("darkens text on a light theme and sets nested surfaces apart", () => {
    const { tokens, fixes } = buildTheme(
      { bg: "#FFFFFF", panel: "#FFFFFF", panel2: "#FFFFFF", ink: "#777777", accent: "#66AAFF" },
      "light",
    );
    expect(contrastFailures(tokens)).toEqual([]);
    expect(fixes.map((f) => f.token)).toEqual(expect.arrayContaining(["panel2", "ink", "accent"]));
    expect(contrast(tokens.panel2!, tokens.panel!)).toBeGreaterThanOrEqual(1.05);
  });

  it("leaves a theme that already reads well alone", () => {
    expect(repairContrast(themes.light, "light")).toEqual({ tokens: themes.light, fixes: [] });
  });
});
