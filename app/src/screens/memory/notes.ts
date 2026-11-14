// Labels and helpers shared by the memory screen and its notes.

import { useAction } from "../../api/context";
import type { HedyApi } from "../../api/HedyApi";
import type { Note, NoteCategory } from "../../api/types";
import type { MessageKey, Translate } from "../../i18n";

export const CATEGORY_LABEL: Record<NoteCategory, MessageKey> = {
  decision: "memory.category.decision",
  convention: "memory.category.convention",
  rule: "memory.category.rule",
  preference: "memory.category.preference",
};

export function categoryOptions(t: Translate): { value: NoteCategory; label: string }[] {
  return (Object.keys(CATEGORY_LABEL) as NoteCategory[]).map((value) => ({ value, label: t(CATEGORY_LABEL[value]) }));
}

/** The project's memory and, below it, its searches. */
export const memoryKey = (project: string) => ["memory", project] as const;

/** Runs a memory action and refreshes the project's memory and status (which counts notes). */
export function useMemoryAction<TVars, TResult>(
  project: string,
  action: (api: HedyApi, vars: TVars) => Promise<TResult>,
) {
  return useAction(action, { invalidate: [memoryKey(project), ["status", project]] });
}

export function authorLabel(note: Note, t: Translate): string {
  return note.author === "user" ? t("memory.note.you") : note.author;
}

const words = (text: string) => text.toLowerCase().match(/[\p{L}\p{N}]+/gu) ?? [];

/**
 * Button text to keep one of two contradicting notes: what makes it different
 * ("Quedarme con 24 h") when that is short, otherwise its position.
 */
export function keepLabel(note: Note, other: Note, position: "first" | "second", t: Translate): string {
  const others = new Set(words(other.text));
  const own = words(note.text);
  const start = own.findIndex((w) => !others.has(w));
  if (start >= 0) {
    // The differing word and, after a number, its unit ("24 h").
    const next = own[start + 1];
    const unit = /\d/.test(own[start] ?? "") && next && next.length <= 3 ? ` ${next}` : "";
    const phrase = `${own[start]}${unit}`;
    if (own.filter((w) => !others.has(w)).length <= 2 && phrase.length <= 14) return t("memory.keepPhrase", { phrase });
  }
  return t(position === "first" ? "memory.keepFirst" : "memory.keepSecond");
}
