// Lets any view put text in the open composer ("Preguntar a la IA sobre este
// archivo" inserts `@path `) without knowing where the composer lives: the
// composer listens while it is mounted, everyone else calls `insertInComposer`.

import { useEffect, useLayoutEffect, useRef } from "react";

type Listener = (text: string) => void;

const listeners = new Set<Listener>();

/** Inserts `text` at the composer's caret and focuses it; `false` when no composer is open. */
export function insertInComposer(text: string): boolean {
  const latest = [...listeners].at(-1);
  if (!latest) return false;
  latest(text);
  return true;
}

/** The composer's side: called with each inserted text while mounted. */
export function useComposerInsert(onInsert: Listener): void {
  const handler = useRef(onInsert);
  useLayoutEffect(() => {
    handler.current = onInsert;
  });
  useEffect(() => {
    const listener: Listener = (text) => handler.current(text);
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  }, []);
}

/** Where `text` goes in `value` at `caret`, with a space before it when needed. */
export function insertAt(value: string, caret: number, text: string): { text: string; caret: number } {
  const at = Math.max(0, Math.min(caret, value.length));
  const before = value.slice(0, at);
  const gap = before && !/\s$/.test(before) ? " " : "";
  const next = `${before}${gap}${text}${value.slice(at)}`;
  return { text: next, caret: at + gap.length + text.length };
}
