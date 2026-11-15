import { createContext, useCallback, useContext, useMemo, useState, type ReactNode } from "react";

import type { CodeCopy, Rev } from "../api/codeTypes";

/**
 * The one way any part of the UI asks to show a file: a search result, a diff
 * line, a blame row, the palette (`@symbol`), a link in the timeline. Whoever
 * renders code (the explorer) reads `current` and shows it; everyone else only
 * calls `open`. Keeping it in one place means "open this file at this line" works
 * the same from everywhere, and the copy (folder or task) always travels with it.
 *
 * ```tsx
 * const nav = useFileNavigator();
 * nav.open({ copy: { kind: "task", task: id }, path: "facturas/impuestos.py", line: 16, view: "diff" });
 * ```
 */
export type NavigatorView = "file" | "diff";

export interface NavigatorTarget {
  /** Which copy: never implied, so the user's folder and a task's worktree are not mixed. */
  copy: CodeCopy;
  /** Relative to the copy, `/`-separated. */
  path: string;
  /** 1-based line to reveal (and highlight); the top of the file when absent. */
  line?: number;
  /** Show the file at a commit, or `"base"` (before the AI); the copy's current files when absent. */
  rev?: Rev;
  /** `"file"` (default): the contents; `"diff"`: against the copy's base (or `rev`). */
  view?: NavigatorView;
}

export interface FileNavigator {
  /** Asks to show a file. The explorer renders it; outside a provider nothing happens. */
  open(target: NavigatorTarget): void;
  /** The last request (with `view` filled in), until `close`. */
  current: (NavigatorTarget & { view: NavigatorView }) | null;
  /**
   * Counts requests, so asking again for the same target (a second click on the
   * same result after scrolling away) can be noticed: use it as an effect dependency.
   */
  seq: number;
  /** The viewer was closed. */
  close(): void;
}

const INERT: FileNavigator = { open: () => {}, current: null, seq: 0, close: () => {} };

const NavigatorContext = createContext<FileNavigator>(INERT);

/**
 * Holds the current file request for everything below it (mounted once in
 * `App`). `onOpen` is told about every request, e.g. to switch to the screen that
 * shows code.
 */
export function FileNavigatorProvider({
  children,
  onOpen,
}: {
  children: ReactNode;
  onOpen?: (target: NavigatorTarget & { view: NavigatorView }) => void;
}) {
  const [state, setState] = useState<{ current: FileNavigator["current"]; seq: number }>({ current: null, seq: 0 });
  const open = useCallback(
    (target: NavigatorTarget) => {
      const current = { ...target, view: target.view ?? "file" };
      setState((s) => ({ current, seq: s.seq + 1 }));
      onOpen?.(current);
    },
    [onOpen],
  );
  const close = useCallback(() => setState((s) => ({ ...s, current: null })), []);
  const value = useMemo<FileNavigator>(() => ({ open, close, ...state }), [open, close, state]);
  return <NavigatorContext.Provider value={value}>{children}</NavigatorContext.Provider>;
}

/** The file navigator; outside `FileNavigatorProvider`, an inert one (tests of single components). */
export function useFileNavigator(): FileNavigator {
  return useContext(NavigatorContext);
}

/** A stable string for a copy: `folder:/code/hedy`, `task:s5` (query keys, maps). */
export function copyKey(copy: CodeCopy): string {
  return copy.kind === "folder" ? `folder:${copy.project}` : `task:${copy.task}`;
}

export function sameCopy(a: CodeCopy, b: CodeCopy): boolean {
  return copyKey(a) === copyKey(b);
}

/**
 * TanStack Query keys for the code API, shared so views reuse each other's
 * cache. Invalidate `codeKeys.copy(copy)` when a copy's files change (an agent
 * edited them, an edit was saved), or `codeKeys.all` for everything.
 */
export const codeKeys = {
  all: ["code"] as const,
  copy: (copy: CodeCopy) => ["code", copyKey(copy)] as const,
  info: (copy: CodeCopy) => ["code", copyKey(copy), "info"] as const,
  tree: (copy: CodeCopy, ignored = false) => ["code", copyKey(copy), "tree", ignored] as const,
  file: (copy: CodeCopy, path: string, rev?: Rev) => ["code", copyKey(copy), "file", path, rev ?? null] as const,
  diff: (copy: CodeCopy, path: string, against?: Rev, full = false) =>
    ["code", copyKey(copy), "diff", path, against ?? "base", full] as const,
  search: (copy: CodeCopy, query: string, options: object = {}) =>
    ["code", copyKey(copy), "search", query, options] as const,
  history: (copy: CodeCopy, path: string, lines?: { start: number; end: number }) =>
    ["code", copyKey(copy), "history", path, lines ?? null] as const,
  blame: (copy: CodeCopy, path: string, rev?: Rev) => ["code", copyKey(copy), "blame", path, rev ?? null] as const,
  symbols: (copy: CodeCopy, path: string) => ["code", copyKey(copy), "symbols", path] as const,
};
