// The syntax theme of a diff (shiki). Hedy's own themes keep the editor themes
// they always had; every other theme gets one built from its `syn*` tokens, so
// a diff looks like the editor in that theme.

import type { ThemeRegistration } from "shiki/core";

import type { ThemeDef } from "./presets";

const BUNDLED: Record<string, string> = { "tokyo-night": "tokyo-night", burbuja: "catppuccin-latte" };

const built = new Map<string, ThemeRegistration>();

/** A short stable hash, so the same colors reuse the same loaded theme. */
function hash(text: string): string {
  let h = 5381;
  for (let i = 0; i < text.length; i++) h = (h * 33) ^ text.charCodeAt(i);
  return (h >>> 0).toString(36);
}

/** The shiki theme name for a Hedy theme (built and remembered when it is not a bundled one). */
export function codeThemeName(theme: Pick<ThemeDef, "id" | "base" | "tokens">): string {
  const bundled = BUNDLED[theme.id];
  if (bundled) return bundled;
  const t = theme.tokens;
  const colors = {
    ink: t.ink ?? "#888888",
    panel: t.panel ?? "#000000",
    muted: t.muted ?? t.ink ?? "#888888",
    keyword: t.synKeyword ?? t.accent ?? "#888888",
    string: t.synString ?? t.success ?? "#888888",
    number: t.synNumber ?? t.warning ?? "#888888",
    func: t.synFunction ?? t.activity ?? "#888888",
    type: t.synType ?? t.activity ?? "#888888",
    comment: t.synComment ?? t.muted ?? "#888888",
    tag: t.synTag ?? t.danger ?? "#888888",
    added: t.diffAddInk ?? "#888888",
    deleted: t.diffDelInk ?? "#888888",
  };
  const name = `hedy-${hash(JSON.stringify(colors))}`;
  if (!built.has(name)) {
    built.set(name, {
      name,
      type: theme.base,
      colors: { "editor.foreground": colors.ink, "editor.background": colors.panel },
      tokenColors: [
        { settings: { foreground: colors.ink, background: colors.panel } },
        { scope: ["comment", "punctuation.definition.comment"], settings: { foreground: colors.comment, fontStyle: "italic" } },
        { scope: ["keyword", "storage", "storage.type", "storage.modifier"], settings: { foreground: colors.keyword } },
        { scope: ["keyword.operator", "punctuation"], settings: { foreground: colors.muted } },
        { scope: ["string", "string.quoted", "string.template", "string.regexp"], settings: { foreground: colors.string } },
        { scope: ["constant.numeric", "constant.language", "constant.character", "support.constant"], settings: { foreground: colors.number } },
        { scope: ["entity.name.function", "support.function", "meta.function-call"], settings: { foreground: colors.func } },
        {
          scope: ["entity.name.type", "entity.name.class", "support.type", "support.class", "entity.other.inherited-class"],
          settings: { foreground: colors.type, fontStyle: "italic" },
        },
        { scope: ["entity.name.tag"], settings: { foreground: colors.tag } },
        { scope: ["entity.other.attribute-name", "variable.other.property"], settings: { foreground: colors.ink } },
        { scope: ["markup.inserted"], settings: { foreground: colors.added } },
        { scope: ["markup.deleted", "invalid"], settings: { foreground: colors.deleted } },
      ],
    });
  }
  return name;
}

/** The theme built for `name`, if it is one of Hedy's (not bundled with shiki). */
export function builtCodeTheme(name: string): ThemeRegistration | undefined {
  return built.get(name);
}
