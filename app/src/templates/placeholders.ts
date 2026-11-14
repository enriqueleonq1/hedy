// Prompt templates in the composer: `{name}` placeholders (letters, digits,
// `_` or `-`, up to 40, no spaces), the same rule as the backend's
// `domain/template.rs`, and the picker's filter.

import type { PromptTemplate } from "../api/types";

const PLACEHOLDER = /\{([\p{L}\p{N}_-]{1,40})\}/gu;

/** Each placeholder of `text` once, in the order it first appears. */
export function placeholders(text: string): string[] {
  const found: string[] = [];
  for (const match of text.matchAll(PLACEHOLDER)) {
    const name = match[1] ?? "";
    if (!found.includes(name)) found.push(name);
  }
  return found;
}

/** Puts each value in its placeholders; an empty value leaves the placeholder. */
export function fillTemplate(text: string, values: Record<string, string>): string {
  return text.replace(PLACEHOLDER, (whole, name: string) => {
    const value = values[name]?.trim();
    return value ? value : whole;
  });
}

const fold = (text: string) =>
  text
    .normalize("NFD")
    .replace(/\p{M}/gu, "")
    .toLowerCase()
    .trim();

/**
 * Templates whose name (or text) matches what follows the `/`, without
 * accents or case: names that start with it first, then names containing
 * it, then texts containing it.
 */
export function filterTemplates(templates: PromptTemplate[], query: string): PromptTemplate[] {
  const q = fold(query);
  if (!q) return templates;
  const rank = (t: PromptTemplate) => {
    const name = fold(t.name);
    if (name.startsWith(q)) return 0;
    if (name.includes(q)) return 1;
    if (fold(t.text).includes(q)) return 2;
    return 3;
  };
  return templates
    .map((t) => ({ t, r: rank(t) }))
    .filter(({ r }) => r < 3)
    .sort((a, b) => a.r - b.r)
    .map(({ t }) => t);
}

/** The text after a leading `/`, while the composer holds only that (the picker is open). */
export function slashQuery(text: string): string | null {
  if (!text.startsWith("/") || text.includes("\n")) return null;
  return text.slice(1);
}
