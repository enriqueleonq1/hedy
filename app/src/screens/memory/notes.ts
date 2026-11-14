// Labels and helpers shared by the memory screen and its notes.

import { useAction } from "../../api/context";
import type { HedyApi } from "../../api/HedyApi";
import type { Note, NoteCategory } from "../../api/types";

export const CATEGORY_LABEL: Record<NoteCategory, string> = {
  decision: "Decisión",
  convention: "Convención",
  rule: "Regla",
  preference: "Preferencia",
};

export const CATEGORY_OPTIONS = (Object.keys(CATEGORY_LABEL) as NoteCategory[]).map((value) => ({
  value,
  label: CATEGORY_LABEL[value],
}));

/** The project's memory and, below it, its searches. */
export const memoryKey = (project: string) => ["memory", project] as const;

/** Runs a memory action and refreshes the project's memory and status (which counts notes). */
export function useMemoryAction<TVars, TResult>(
  project: string,
  action: (api: HedyApi, vars: TVars) => Promise<TResult>,
) {
  return useAction(action, { invalidate: [memoryKey(project), ["status", project]] });
}

export function authorLabel(note: Note): string {
  return note.author === "user" ? "tú" : note.author;
}

const words = (text: string) => text.toLowerCase().match(/[\p{L}\p{N}]+/gu) ?? [];

/**
 * Button text to keep one of two contradicting notes: what makes it different
 * ("Quedarme con 24 h") when that is short, otherwise its position.
 */
export function keepLabel(note: Note, other: Note, position: "primera" | "segunda"): string {
  const others = new Set(words(other.text));
  const own = words(note.text);
  const start = own.findIndex((w) => !others.has(w));
  if (start >= 0) {
    // The differing word and, after a number, its unit ("24 h").
    const next = own[start + 1];
    const unit = /\d/.test(own[start] ?? "") && next && next.length <= 3 ? ` ${next}` : "";
    const phrase = `${own[start]}${unit}`;
    if (own.filter((w) => !others.has(w)).length <= 2 && phrase.length <= 14) return `Quedarme con ${phrase}`;
  }
  return `Quedarme con la ${position}`;
}
