// Text search over a copy: a query with case, whole-word and regex toggles and
// an optional "files to include" glob (debounced), results grouped by file in a
// virtualized tree with highlighted matches. A result opens the viewer there.

import { Fragment, useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { errorMessage, useApi } from "../../api/context";
import type { CodeCopy, CopyInfo, MatchRange, SearchOptions } from "../../api/codeTypes";
import { useContextMenu } from "../../components/ui/ContextMenu";
import { flattenTree, type TreeRow } from "../../components/ui/treeModel";
import { VirtualTree, type VirtualTreeHandle } from "../../components/ui/VirtualTree";
import { codeKeys } from "../../files/navigator";
import { useT } from "../../i18n";
import { dirName } from "../copies";
import { TreeSkeleton } from "../explorer/Explorer";
import { useFileActions } from "../fileActions";
import { FileIcon } from "../fileIcons";
import { CollapseAllIcon, CopyIcon, ExternalIcon, FilesIcon, FolderOpenIcon, MoreIcon, RefreshIcon, SparkIcon } from "../icons";
import { CodeState } from "../viewer/states";
import { previewLine, resultsTree, type MatchItem } from "./searchModel";

const DEBOUNCE_MS = 250;

function useDebounced<T>(value: T, ms: number): [T, (now: T) => void] {
  const [settled, setSettled] = useState(value);
  useEffect(() => {
    const id = window.setTimeout(() => setSettled(value), ms);
    return () => window.clearTimeout(id);
  }, [value, ms]);
  return [settled, setSettled];
}

/** Text with its matches marked. */
export function Highlighted({ text, ranges }: { text: string; ranges: MatchRange[] }) {
  const parts = [];
  let at = 0;
  for (const range of [...ranges].sort((a, b) => a.start - b.start)) {
    if (range.start < at || range.end <= range.start) continue;
    if (range.start > at) parts.push(<Fragment key={`t${at}`}>{text.slice(at, range.start)}</Fragment>);
    parts.push(<mark key={`m${range.start}`}>{text.slice(range.start, range.end)}</mark>);
    at = range.end;
  }
  if (at < text.length) parts.push(<Fragment key={`t${at}`}>{text.slice(at)}</Fragment>);
  return <>{parts}</>;
}

export interface SearchViewProps {
  copy: CodeCopy;
  info: CopyInfo | undefined;
  /** Opens a match in the viewer. */
  onOpen: (path: string, line: number, ranges: MatchRange[], key: string) => void;
  /** The match open in the viewer (its row is selected). */
  openKey: string | null;
  /** Changes to focus the query field, filled with `prefill` when given. */
  focusSeq?: number;
  prefill?: string;
}

export function SearchView({ copy, info, onOpen, openKey, focusSeq = 0, prefill }: SearchViewProps) {
  const api = useApi();
  const t = useT();
  const client = useQueryClient();
  const actions = useFileActions(copy, info);
  const menu = useContextMenu();
  const input = useRef<HTMLInputElement>(null);
  const tree = useRef<VirtualTreeHandle>(null);
  const [query, setQuery] = useState(prefill ?? "");
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [wholeWord, setWholeWord] = useState(false);
  const [regex, setRegex] = useState(false);
  const [globOpen, setGlobOpen] = useState(false);
  const [glob, setGlob] = useState("");
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());

  const [settledQuery, settleQuery] = useDebounced(query, DEBOUNCE_MS);
  const [settledGlob] = useDebounced(glob, DEBOUNCE_MS);
  const options = useMemo<SearchOptions>(() => {
    const o: SearchOptions = {};
    if (caseSensitive) o.caseSensitive = true;
    if (wholeWord) o.wholeWord = true;
    if (regex) o.regex = true;
    if (globOpen && settledGlob.trim()) o.glob = settledGlob.trim();
    return o;
  }, [caseSensitive, wholeWord, regex, globOpen, settledGlob]);
  const term = settledQuery.trim() ? settledQuery : "";

  const results = useQuery({
    queryKey: codeKeys.search(copy, term, options),
    queryFn: () => api.searchText(copy, term, options),
    enabled: term !== "",
    placeholderData: (previous) => previous,
  });

  // Ctrl/⌘ Shift F: the field takes the focus, filled with the selection when there is one.
  const [seenFocus, setSeenFocus] = useState(focusSeq);
  if (focusSeq !== seenFocus) {
    setSeenFocus(focusSeq);
    if (prefill) {
      setQuery(prefill);
      settleQuery(prefill);
    }
  }
  useEffect(() => {
    if (focusSeq === 0) return;
    input.current?.focus();
    input.current?.select();
  }, [focusSeq]);

  const files = useMemo(() => (term ? (results.data?.files ?? []) : []), [results.data, term]);
  const root = useMemo(() => resultsTree(files), [files]);
  const expanded = useMemo(() => new Set(files.map((f) => f.path).filter((p) => !collapsed.has(p))), [files, collapsed]);
  const rows = useMemo(() => flattenTree(root, expanded), [root, expanded]);

  const onInputKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      e.preventDefault();
      settleQuery(query);
    } else if (e.key === "ArrowDown" && rows.length > 0) {
      e.preventDefault();
      tree.current?.focus();
    } else if (e.altKey && !e.ctrlKey && !e.metaKey) {
      const toggle = { c: setCaseSensitive, w: setWholeWord, r: setRegex }[e.key.toLowerCase()];
      if (toggle) {
        e.preventDefault();
        toggle((on) => !on);
      }
    }
  };

  const openMenu = (row: TreeRow<MatchItem>, at: { x: number; y: number }) => {
    const path = row.node.kind === "file" ? row.node.item.file : row.node.path;
    const line = row.node.kind === "file" ? row.node.item.match.line : undefined;
    menu.open(
      at,
      [
        {
          label: t("code.action.open"),
          icon: <FilesIcon />,
          shortcut: "Enter",
          onSelect: () =>
            row.node.kind === "file" ? onOpen(path, line ?? 1, row.node.item.match.ranges, row.node.path) : onOpen(path, 1, [], path),
        },
        { label: t("code.action.copyPath"), icon: <CopyIcon />, separatorBefore: true, onSelect: () => actions.copyPath(path) },
        ...(line ? [{ label: t("code.action.copyLine"), onSelect: () => actions.copyLineReference(path, line) }] : []),
        { label: t("code.action.reveal"), icon: <FolderOpenIcon />, onSelect: () => actions.reveal(path) },
        { label: t("code.action.openHere"), icon: <ExternalIcon />, onSelect: () => actions.openInEditor(path, line) },
        {
          label: line ? t("code.action.askAboutThis") : t("code.action.askAi"),
          icon: <SparkIcon />,
          separatorBefore: true,
          onSelect: () => actions.askAi(path, line ? { start: line, end: line } : undefined),
        },
      ],
      t("code.explorer.menu", { name: row.node.kind === "file" ? `${path}:${line}` : path }),
    );
  };

  const total = results.data?.total ?? 0;
  let body;
  if (!term) {
    body = <CodeState title={t("code.search.idle")} text={t("code.search.idleText")} />;
  } else if (results.isPending) {
    body = <TreeSkeleton label={t("code.search.loading")} rows={6} />;
  } else if (results.isError) {
    body = <CodeState tone="error" role="alert" title={t("code.search.error")} text={errorMessage(results.error, t)} />;
  } else if (files.length === 0) {
    body = <CodeState role="status" title={t("code.search.none")} text={t("code.search.noneText")} />;
  } else {
    body = (
      <VirtualTree<MatchItem>
        ref={tree}
        label={t("code.search.results")}
        className="search-results"
        rows={rows}
        expanded={expanded}
        selectedPath={openKey}
        onToggle={(path, on) =>
          setCollapsed((old) => {
            const next = new Set(old);
            if (on) next.delete(path);
            else next.add(path);
            return next;
          })
        }
        onActivate={(row) =>
          row.node.kind === "file" && onOpen(row.node.item.file, row.node.item.match.line, row.node.item.match.ranges, row.node.path)
        }
        onContextMenu={openMenu}
        renderRow={(row) => {
          const node = row.node;
          if (node.kind === "folder") {
            const dir = dirName(node.path);
            return (
              <>
                <FileIcon path={node.path} />
                <span className="vtree__name">{node.name}</span>
                {dir && <span className="search-results__dir">{dir}</span>}
                <span className="count-badge" aria-label={t("code.search.fileCount", { n: node.size })}>
                  {node.size}
                </span>
              </>
            );
          }
          const { match } = node.item;
          const preview = previewLine(match.text, match.ranges);
          return (
            <>
              <span className="search-results__line" aria-hidden>
                {match.line}
              </span>
              <span className="search-results__text">
                <span className="sr-only">{t("code.search.lineLabel", { line: match.line })} </span>
                {preview.cut && "…"}
                <Highlighted text={preview.text} ranges={preview.ranges} />
              </span>
              <span className="row-actions" aria-hidden>
                <button
                  type="button"
                  className="row-action"
                  tabIndex={-1}
                  aria-label={t("code.action.more")}
                  title={t("code.action.more")}
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={(e) => {
                    e.stopPropagation();
                    const box = e.currentTarget.getBoundingClientRect();
                    openMenu(row, { x: box.left, y: box.bottom + 2 });
                  }}
                >
                  <MoreIcon />
                </button>
              </span>
            </>
          );
        }}
      />
    );
  }

  return (
    <div className="search-view">
      <div className="search-form">
        <div className="search-field">
          <input
            ref={input}
            type="search"
            className="search-field__input"
            value={query}
            aria-label={t("code.search.query")}
            placeholder={t("code.search.placeholder")}
            spellCheck={false}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={onInputKey}
          />
          <Toggle label={t("code.search.caseSensitive")} keys="Alt+C" on={caseSensitive} onClick={() => setCaseSensitive((v) => !v)}>
            Aa
          </Toggle>
          <Toggle label={t("code.search.wholeWord")} keys="Alt+W" on={wholeWord} onClick={() => setWholeWord((v) => !v)}>
            <span className="search-field__word">ab</span>
          </Toggle>
          <Toggle label={t("code.search.regex")} keys="Alt+R" on={regex} onClick={() => setRegex((v) => !v)}>
            .*
          </Toggle>
        </div>
        <button
          type="button"
          className="link-btn search-form__more"
          aria-expanded={globOpen}
          onClick={() => setGlobOpen((v) => !v)}
        >
          {t("code.search.include")}
        </button>
        {globOpen && (
          <input
            className="search-field__glob mono"
            value={glob}
            aria-label={t("code.search.include")}
            placeholder={t("code.search.includePlaceholder")}
            spellCheck={false}
            onChange={(e) => setGlob(e.target.value)}
          />
        )}
      </div>
      {term && results.data && files.length > 0 && (
        <div className="code-toolbar search-summary">
          <span className="code-toolbar__title" role="status">
            {total === 1 ? t("code.search.summaryOne") : t("code.search.summary", { n: total, files: files.length })}
          </span>
          <button
            type="button"
            className="icon-btn"
            aria-label={t("code.explorer.collapseAll")}
            title={t("code.explorer.collapseAll")}
            onClick={() => setCollapsed(new Set(files.map((f) => f.path)))}
          >
            <CollapseAllIcon />
          </button>
          <button
            type="button"
            className="icon-btn"
            aria-label={t("code.search.refresh")}
            title={t("code.search.refresh")}
            onClick={() => void client.invalidateQueries({ queryKey: codeKeys.search(copy, term) })}
          >
            <RefreshIcon />
          </button>
        </div>
      )}
      {term && results.data?.truncated && <p className="code-notice">{t("code.search.truncated")}</p>}
      <div className="search-view__results">{body}</div>
      {menu.element}
    </div>
  );
}

function Toggle({
  label,
  keys,
  on,
  onClick,
  children,
}: {
  label: string;
  keys: string;
  on: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      className="search-toggle"
      aria-pressed={on}
      aria-label={label}
      title={`${label} (${keys})`}
      onClick={onClick}
    >
      <span aria-hidden>{children}</span>
    </button>
  );
}
