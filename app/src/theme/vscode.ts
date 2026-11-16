// "Importar de VS Code": a VS Code color theme (the JSON a theme extension
// ships, comments and trailing commas allowed) read into a seed. Only what
// the file itself says: `include` of other files is not followed.

import { flatten, isHex, toHex, parseHex } from "./color";
import { luminance } from "./contrast";
import type { SeedKey, ThemeSeed } from "./derive";
import type { ThemeBase } from "./tokens";

export type ImportErrorCode = "invalidJson" | "notTheme" | "include" | "missingColors";

/** Why a file could not be read as a theme (the UI says it in the user's language). */
export class ThemeImportError extends Error {
  constructor(readonly code: ImportErrorCode) {
    super(code);
    this.name = "ThemeImportError";
  }
}

export interface ImportedTheme {
  /** The theme's own `name`, when it has one. */
  name: string | null;
  base: ThemeBase;
  seed: ThemeSeed;
}

/** JSONC → JSON: drops `//` and `/* *\/` comments (never inside strings) and trailing commas. */
export function stripJsonc(text: string): string {
  let out = "";
  let i = 0;
  while (i < text.length) {
    const c = text.charAt(i);
    if (c === '"') {
      let j = i + 1;
      while (j < text.length && text[j] !== '"') j += text[j] === "\\" ? 2 : 1;
      out += text.slice(i, j + 1);
      i = j + 1;
    } else if (c === "/" && text[i + 1] === "/") {
      while (i < text.length && text[i] !== "\n") i++;
    } else if (c === "/" && text[i + 1] === "*") {
      const end = text.indexOf("*/", i + 2);
      i = end === -1 ? text.length : end + 2;
    } else if (c === ",") {
      // A comma followed only by blanks (or comments) and a closing bracket goes.
      let j = i + 1;
      for (;;) {
        while (j < text.length && /\s/.test(text.charAt(j))) j++;
        if (text[j] === "/" && text[j + 1] === "/") {
          while (j < text.length && text[j] !== "\n") j++;
        } else if (text[j] === "/" && text[j + 1] === "*") {
          const end = text.indexOf("*/", j + 2);
          j = end === -1 ? text.length : end + 2;
        } else break;
      }
      if (text[j] !== "}" && text[j] !== "]") out += c;
      i++;
    } else {
      out += c;
      i++;
    }
  }
  return out;
}

type Colors = Record<string, unknown>;

interface TokenRule {
  scope?: string | string[];
  settings?: { foreground?: string };
}

/** A color as `#RRGGBB`, or undefined when it is not one. */
function hexOf(value: unknown): string | undefined {
  const color = typeof value === "string" ? parseHex(value) : null;
  return color ? toHex(color) : undefined;
}

/** The first of `keys` that holds a color, flattened over `bg` when it has alpha. */
function pick(colors: Colors, keys: string[], bg?: string): string | undefined {
  for (const key of keys) {
    const value = colors[key];
    if (!isHex(value)) continue;
    return bg ? flatten(value, bg) : hexOf(value);
  }
  return undefined;
}

function scopesOf(rule: TokenRule): string[] {
  const raw = Array.isArray(rule.scope) ? rule.scope : typeof rule.scope === "string" ? rule.scope.split(",") : [];
  return raw.map((s) => s.trim()).filter((s) => s && !s.includes(" "));
}

/**
 * The color of the first scope in `wanted` that a rule names exactly; failing
 * that, a rule for a narrower scope (`keyword.control` for `keyword`), never one in `avoid`.
 */
function tokenColor(rules: TokenRule[], wanted: string[], avoid: string[] = []): string | undefined {
  const colored = rules.filter((r) => hexOf(r.settings?.foreground));
  const color = (r: TokenRule) => hexOf(r.settings?.foreground);
  for (const scope of wanted) {
    const exact = colored.find((r) => scopesOf(r).includes(scope));
    if (exact) return color(exact);
  }
  for (const scope of wanted) {
    const narrower = colored.find((r) =>
      scopesOf(r).some((s) => s.startsWith(`${scope}.`) && !avoid.some((a) => s.startsWith(a))),
    );
    if (narrower) return color(narrower);
  }
  return undefined;
}

/** Reads a VS Code color theme into a seed; throws `ThemeImportError` when it cannot. */
export function importVscodeTheme(text: string): ImportedTheme {
  let json: unknown;
  try {
    json = JSON.parse(stripJsonc(text));
  } catch {
    throw new ThemeImportError("invalidJson");
  }
  if (!json || typeof json !== "object" || Array.isArray(json)) throw new ThemeImportError("notTheme");
  const theme = json as { name?: unknown; type?: unknown; colors?: unknown; tokenColors?: unknown; include?: unknown };
  const colors: Colors = theme.colors && typeof theme.colors === "object" ? (theme.colors as Colors) : {};
  const rules: TokenRule[] = Array.isArray(theme.tokenColors) ? (theme.tokenColors as TokenRule[]) : [];
  if (!theme.colors && !theme.tokenColors && !theme.include) throw new ThemeImportError("notTheme");

  const bg = pick(colors, ["editor.background"]);
  const ink = pick(colors, ["editor.foreground", "foreground"], bg);
  if (!bg || !ink) throw new ThemeImportError(theme.include ? "include" : "missingColors");

  const type = typeof theme.type === "string" ? theme.type.toLowerCase() : "";
  const base: ThemeBase = type.includes("light")
    ? "light"
    : type === "dark" || type.startsWith("hc")
      ? "dark"
      : luminance(bg) > 0.4
        ? "light"
        : "dark";

  const on = (keys: string[]) => pick(colors, keys, bg);
  const accent =
    on(["textLink.foreground", "focusBorder", "button.background", "activityBarBadge.background"]) ??
    tokenColor(rules, ["keyword"]) ??
    ink;

  const seed: Partial<Record<SeedKey, string>> = {
    bg,
    ink,
    accent,
    panel: on(["sideBar.background", "activityBar.background", "panel.background"]),
    panel2: on(["editorWidget.background", "dropdown.background", "input.background"]),
    muted: on(["descriptionForeground", "editorLineNumber.activeForeground", "tab.inactiveForeground"]),
    line: on(["panel.border", "editorGroup.border", "sideBar.border", "contrastBorder"]),
    action: on(["button.background"]),
    success: on(["gitDecoration.addedResourceForeground", "terminal.ansiGreen"]),
    danger: on(["errorForeground", "editorError.foreground", "terminal.ansiRed"]),
    warning: on(["editorWarning.foreground", "terminal.ansiYellow"]),
    info: on(["editorInfo.foreground", "terminal.ansiCyan", "terminal.ansiBlue"]),
    diffAddBg: on(["diffEditor.insertedLineBackground", "diffEditor.insertedTextBackground"]),
    diffDelBg: on(["diffEditor.removedLineBackground", "diffEditor.removedTextBackground"]),
    keyword: tokenColor(rules, ["keyword", "keyword.control", "storage.type", "storage"], ["keyword.operator"]),
    string: tokenColor(rules, ["string", "string.quoted"]),
    number: tokenColor(rules, ["constant.numeric", "constant.language", "constant"]),
    function: tokenColor(rules, ["entity.name.function", "support.function"]),
    type: tokenColor(rules, ["entity.name.type", "support.type", "entity.name.class", "support.class"]),
    comment: tokenColor(rules, ["comment", "punctuation.definition.comment"]),
  };
  const defined = Object.fromEntries(Object.entries(seed).filter(([, v]) => v)) as ThemeSeed;
  const name = typeof theme.name === "string" && theme.name.trim() ? theme.name.trim() : null;
  return { name, base, seed: defined };
}
