// The rows of each code mode: files of the copy, a line or a symbol of the
// active file, and text across the copy. Each reads through the shared
// `codeKeys`, so the explorer and the palette reuse one cache.

import { keepPreviousData, skipToken, useQuery } from "@tanstack/react-query";
import { useEffect, useMemo, useState, type ReactNode } from "react";

import type { ChangeKind, CodeSymbol, SymbolKind, TreeEntry } from "../../api/codeTypes";
import { errorMessage, useApi } from "../../api/context";
import { codeKeys, type NavigatorTarget, useFileNavigator } from "../../files/navigator";
import type { MessageKey, Translate } from "../../i18n";
import type { ActiveCopy } from "./copy";
import { fuzzyMatch, fuzzyPath, prepare, type Prepared } from "./fuzzy";
import { FileIcon, LineIcon, SymbolIcon } from "./icons";
import { parseLine, splitFileLine } from "./modes";
import { modKey } from "./keys";
import { recentFiles } from "./recentFiles";
import type { ListSection } from "./VirtualList";

export interface Item {
  key: string;
  label: string;
  /** Matched characters of `label`, to highlight. */
  labelHits?: number[];
  detail?: string;
  detailHits?: number[];
  /** Code (a matching line): monospace. */
  mono?: boolean;
  icon?: ReactNode;
  /** A dot of this color before the label (decorative). */
  color?: string;
  status?: ChangeKind | null;
  /** A shortcut shown on the right. */
  keys?: string[];
  /** Opens a page of choices. */
  more?: boolean;
  /** Commands: remembered among the recent ones. */
  commandId?: string;
  /** What Enter does, for the footer… */
  primary: MessageKey;
  /** …and Ctrl/⌘ Enter (or Ctrl/⌘ click), when it does something else. */
  alt?: MessageKey;
  /** The palette stays open (a page, another mode, a project picked to search in). */
  stay?: boolean;
  run: (alt: boolean) => void;
}

export interface Source {
  sections: ListSection<Item>[];
  loading?: boolean;
  /** Why the list is empty (no project, no active file, no match), or a failure. */
  message?: string;
  error?: boolean;
  /** A quiet line in the footer: "3 matches in 2 files". */
  note?: string;
}

const EMPTY: Source = { sections: [] };
/** Files listed with a query: the best ones (the list is virtualized, the sort is not free). */
const MAX_FILES = 500;
/** Files offered in the "everything" mode, after the commands. */
export const FILES_IN_ALL = 6;

export const STATUS_LETTER: Record<ChangeKind, string> = {
  added: "A",
  modified: "M",
  deleted: "D",
  renamed: "R",
  untracked: "U",
};

export const STATUS_LABEL: Record<ChangeKind, MessageKey> = {
  added: "palette.status.added",
  modified: "palette.status.modified",
  deleted: "palette.status.deleted",
  renamed: "palette.status.renamed",
  untracked: "palette.status.untracked",
};

function split(path: string): { dir: string; name: string; at: number } {
  const at = path.lastIndexOf("/") + 1;
  return { dir: path.slice(0, Math.max(0, at - 1)), name: path.slice(at), at };
}

/** A file row; `hits` index the whole path. */
function fileItem(
  entry: TreeEntry,
  hits: number[],
  open: (target: NavigatorTarget) => void,
  copy: ActiveCopy["copy"],
  line: number | null,
): Item {
  const { dir, name, at } = split(entry.path);
  const gone = entry.status === "deleted";
  const changed = entry.status !== null;
  return {
    key: `file:${entry.path}`,
    label: name,
    labelHits: hits.filter((i) => i >= at).map((i) => i - at),
    detail: dir || undefined,
    detailHits: hits.filter((i) => i < at - 1),
    icon: <FileIcon path={entry.path} locked={entry.locked} />,
    status: entry.status,
    // A deleted file only has a diff left.
    primary: gone ? "palette.action.diff" : "palette.action.open",
    alt: changed && !gone ? "palette.action.diff" : undefined,
    run: (alt) =>
      open({
        copy,
        path: entry.path,
        ...(line ? { line } : {}),
        view: gone || (alt && changed) ? "diff" : "file",
      }),
  };
}

/** Files of the copy: fuzzy by name and path, recent and changed ones first with no query. */
export function useFileSource({
  active,
  query,
  enabled,
  open,
  t,
  limit,
}: {
  active: ActiveCopy | null;
  query: string;
  enabled: boolean;
  open: (target: NavigatorTarget) => void;
  t: Translate;
  /** Only the best few, in one section (the "everything" mode). */
  limit?: number;
}): Source {
  const api = useApi();
  const copy = active?.copy;
  const tree = useQuery({
    queryKey: copy ? codeKeys.tree(copy) : ["code", "none"],
    queryFn: copy && enabled ? () => api.codeTree(copy) : skipToken,
  });
  const entries = tree.data?.entries;
  const prepared = useMemo<{ entry: TreeEntry; p: Prepared }[]>(
    () =>
      (entries ?? [])
        // Ignored folders are listed as `dir/`: nothing to open there.
        .filter((e) => !e.path.endsWith("/"))
        .map((entry) => ({ entry, p: prepare(entry.path) })),
    [entries],
  );
  const { query: fileQuery, line } = splitFileLine(query);
  const trimmed = fileQuery.trim();

  const ranked = useMemo(() => {
    const wanted = splitFileLine(query).query.trim();
    if (!wanted) return null;
    const scored: { entry: TreeEntry; score: number; positions: number[] }[] = [];
    for (const { entry, p } of prepared) {
      const m = fuzzyPath(wanted, p);
      if (m) scored.push({ entry, score: m.score, positions: m.positions });
    }
    scored.sort((a, b) => b.score - a.score || a.entry.path.length - b.entry.path.length);
    return scored.slice(0, MAX_FILES);
  }, [prepared, query]);

  if (!enabled || !active || !copy) return EMPTY;
  if (tree.isPending) return { sections: [], loading: true };
  if (tree.isError) return { sections: [], error: true, message: errorMessage(tree.error, t) };

  if (ranked) {
    const shown = limit ? ranked.slice(0, limit) : ranked;
    if (!shown.length) return limit ? EMPTY : { sections: [], message: t("palette.files.none", { query: trimmed }) };
    return {
      sections: [
        {
          key: "files",
          title: limit ? t("palette.group.files") : undefined,
          items: shown.map((s) => fileItem(s.entry, s.positions, open, copy, line)),
        },
      ],
      note: limit ? undefined : count(t, ranked.length, "palette.files.count"),
    };
  }
  if (limit) return EMPTY;

  // No query: what was opened lately, then what changed, then everything else.
  const byPath = new Map(prepared.map(({ entry }) => [entry.path, entry]));
  const recent = recentFiles(active.project)
    .map((path) => byPath.get(path))
    .filter((e): e is TreeEntry => !!e && e.status !== "deleted");
  const seen = new Set(recent.map((e) => e.path));
  const changed = prepared.map((x) => x.entry).filter((e) => e.status !== null && !seen.has(e.path));
  for (const e of changed) seen.add(e.path);
  const rest = prepared.map((x) => x.entry).filter((e) => !seen.has(e.path));
  const sections: ListSection<Item>[] = [];
  const add = (key: string, title: string, list: TreeEntry[]) =>
    list.length && sections.push({ key, title, items: list.map((e) => fileItem(e, [], open, copy, null)) });
  add("recent", t("palette.group.recent"), recent);
  add("changed", t("palette.group.changed"), changed);
  add("all", t("palette.group.allFiles"), rest);
  if (!sections.length) return { sections, message: t("palette.files.empty") };
  return { sections, note: count(t, prepared.length, "palette.files.count") };
}

function count(t: Translate, n: number, key: "palette.files.count" | "palette.text.count"): string {
  return t(key, { n: n.toLocaleString() });
}

// ---------------------------------------------------------------- the active file

function basename(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1);
}

/** `:120` — a line of the active file. */
export function useLineSource({ query, enabled, t }: { query: string; enabled: boolean; t: Translate }): Source {
  const api = useApi();
  const nav = useFileNavigator();
  const current = nav.current;
  const file = useQuery({
    queryKey: current ? codeKeys.file(current.copy, current.path, current.rev) : ["code", "none"],
    queryFn: current && enabled ? () => api.readFile(current.copy, current.path, current.rev) : skipToken,
  });
  if (!enabled) return EMPTY;
  if (!current) return { sections: [], message: t("palette.noFile", { keys: `${modKey()} P` }) };
  const lines = file.data?.kind === "text" ? file.data.text.split("\n").length - (file.data.text.endsWith("\n") ? 1 : 0) : null;
  const name = basename(current.path);
  const target = parseLine(query);
  if (!target) {
    return {
      sections: [],
      message: query.trim()
        ? t("palette.line.invalid")
        : lines
          ? t("palette.line.prompt", { name, n: lines })
          : t("palette.line.promptNoCount", { name }),
    };
  }
  const line = lines ? Math.min(target.line, Math.max(lines, 1)) : target.line;
  return {
    sections: [
      {
        key: "line",
        items: [
          {
            key: `line:${line}`,
            label: t("palette.line.go", { line }),
            detail: name,
            icon: <LineIcon />,
            primary: "palette.action.go",
            run: () => nav.open({ copy: current.copy, path: current.path, rev: current.rev, line, view: "file" }),
          },
        ],
      },
    ],
    note: lines ? t("palette.line.count", { n: lines }) : undefined,
  };
}

const KIND_ORDER: SymbolKind[] = [
  "class",
  "struct",
  "interface",
  "trait",
  "enum",
  "type",
  "module",
  "function",
  "method",
  "constant",
  "heading",
];

const KIND_TITLE: Record<SymbolKind, MessageKey> = {
  class: "palette.kind.class",
  struct: "palette.kind.struct",
  interface: "palette.kind.interface",
  trait: "palette.kind.trait",
  enum: "palette.kind.enum",
  type: "palette.kind.type",
  module: "palette.kind.module",
  function: "palette.kind.function",
  method: "palette.kind.method",
  constant: "palette.kind.constant",
  heading: "palette.kind.heading",
};

/** `@name` — a symbol of the active file, grouped by kind with no query. */
export function useSymbolSource({ query, enabled, t }: { query: string; enabled: boolean; t: Translate }): Source {
  const api = useApi();
  const nav = useFileNavigator();
  const current = nav.current;
  const symbols = useQuery({
    queryKey: current ? codeKeys.symbols(current.copy, current.path) : ["code", "none"],
    queryFn: current && enabled ? () => api.fileSymbols(current.copy, current.path) : skipToken,
  });
  if (!enabled) return EMPTY;
  if (!current) return { sections: [], message: t("palette.noFile", { keys: `${modKey()} P` }) };
  if (symbols.isPending) return { sections: [], loading: true };
  if (symbols.isError) return { sections: [], error: true, message: errorMessage(symbols.error, t) };
  const name = basename(current.path);
  const item = (s: CodeSymbol, hits: number[] = []): Item => ({
    key: `symbol:${s.line}:${s.name}`,
    label: s.name,
    labelHits: hits,
    detail: t("palette.symbol.line", { line: s.line }),
    icon: <SymbolIcon kind={s.kind} />,
    primary: "palette.action.go",
    run: () => nav.open({ copy: current.copy, path: current.path, rev: current.rev, line: s.line, view: "file" }),
  });
  const list = symbols.data;
  if (!list.length) return { sections: [], message: t("palette.symbol.empty", { name }) };
  const trimmed = query.trim();
  if (trimmed) {
    const ranked = list
      .map((s) => ({ s, m: fuzzyMatch(trimmed, s.name) }))
      .filter((x): x is { s: CodeSymbol; m: NonNullable<typeof x.m> } => x.m !== null)
      .sort((a, b) => b.m.score - a.m.score || a.s.line - b.s.line);
    if (!ranked.length) return { sections: [], message: t("palette.symbol.none", { query: trimmed }) };
    return { sections: [{ key: "symbols", items: ranked.map((x) => item(x.s, x.m.positions)) }] };
  }
  const sections = KIND_ORDER.map((kind) => ({
    key: kind,
    title: t(KIND_TITLE[kind]),
    items: list.filter((s) => s.kind === kind).map((s) => item(s)),
  })).filter((s) => s.items.length > 0);
  return { sections, note: t("palette.symbol.count", { n: list.length, name }) };
}

// ---------------------------------------------------------------- text across the copy

function useDebounced<T>(value: T, ms: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const id = setTimeout(() => setDebounced(value), ms);
    return () => clearTimeout(id);
  }, [value, ms]);
  return debounced;
}

/** Search waits this long after the last key. */
export const TEXT_DEBOUNCE_MS = 200;

/** `#text` — matching lines across the copy, grouped by file; Enter opens at the line. */
export function useTextSource({
  active,
  query,
  enabled,
  open,
  t,
}: {
  active: ActiveCopy | null;
  query: string;
  enabled: boolean;
  open: (target: NavigatorTarget) => void;
  t: Translate;
}): Source {
  const api = useApi();
  const copy = active?.copy;
  const typed = query.trim();
  const debounced = useDebounced(typed, TEXT_DEBOUNCE_MS);
  const search = useQuery({
    queryKey: copy ? codeKeys.search(copy, debounced) : ["code", "none"],
    queryFn: copy && enabled && debounced ? () => api.searchText(copy, debounced) : skipToken,
    placeholderData: keepPreviousData,
  });
  if (!enabled || !active || !copy) return EMPTY;
  if (!typed) return { sections: [], message: t("palette.text.prompt") };
  const waiting = debounced !== typed || search.isFetching;
  if (search.isError) return { sections: [], error: true, message: errorMessage(search.error, t) };
  const data = search.data;
  if (!data) return { sections: [], loading: true };
  if (!data.files.length) {
    return waiting ? { sections: [], loading: true } : { sections: [], message: t("palette.text.none", { query: debounced }) };
  }
  const sections: ListSection<Item>[] = data.files.map((file) => ({
    key: `text:${file.path}`,
    title: file.path,
    header: (
      <>
        <FileIcon path={file.path} />
        <span className="palette__group-path">{file.path}</span>
        <span className="palette__group-count" aria-hidden>
          {file.matches.length}
        </span>
      </>
    ),
    items: file.matches.map((m) => {
      // Leading indentation is noise in a one-line preview.
      const indent = m.text.length - m.text.trimStart().length;
      const hits: number[] = [];
      for (const r of m.ranges) for (let i = Math.max(r.start, indent); i < r.end; i++) hits.push(i - indent);
      return {
        key: `match:${file.path}:${m.line}`,
        label: m.text.slice(indent),
        labelHits: hits,
        mono: true,
        detail: String(m.line),
        primary: "palette.action.open" as const,
        alt: "palette.action.diff" as const,
        run: (alt: boolean) => open({ copy, path: file.path, line: m.line, view: alt ? "diff" : "file" }),
      };
    }),
  }));
  const note = data.truncated
    ? t("palette.text.truncated", { n: data.total.toLocaleString() })
    : t("palette.text.count", { n: data.total.toLocaleString(), files: data.files.length });
  return { sections, note, loading: waiting };
}
