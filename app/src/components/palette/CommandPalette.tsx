// The command palette: a keyboard-first way to go anywhere, open any file of the
// code on screen, and run any action. An accessible combobox: the focus stays in
// the field, the active option is announced through aria-activedescendant.
//
// One palette, one field, several modes (as VS Code's quick open):
// - Ctrl/⌘ K opens it on **everything**: what needs you, places, projects,
//   tasks, accounts and actions; with a query, the best files of the copy too.
// - Ctrl/⌘ P opens it on **files** of the copy on screen (the open task's
//   worktree, else the project's folder): fuzzy by name and path, recent and
//   changed files first. Enter opens the file, Ctrl/⌘ Enter its diff.
// - F1 or Ctrl/⌘ ⇧ P opens it on **commands** (`>`): every action, with its shortcut.
// - A prefix switches mode wherever it was opened: `>` commands, `:` a line of
//   the active file, `@` a symbol of it, `#` text across the copy, `?` the list
//   of modes. Deleting the prefix goes back to where it was opened (files or
//   everything). The chips at the top do the same with the mouse.
// The active file is `useFileNavigator().current`; every file opens through
// `useFileNavigator().open`, so whoever shows code (the side panel) shows it.

import {
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent,
} from "react";
import { skipToken, useQuery } from "@tanstack/react-query";

import { useApi } from "../../api/context";
import type { Overview } from "../../api/types";
import { codeKeys, sameCopy, useFileNavigator } from "../../files/navigator";
import { useT, type MessageKey, type Translate } from "../../i18n";
import { useCreate } from "../../screens/create/CreateProvider";
import { projectColor } from "../projects";
import type { View } from "../Layout";
import { useCodeCommands, useOpenFile } from "./codeCommands";
import { GROUP_ORDER, usePaletteCommands, type Command, type GroupKey, type Page } from "./commands";
import { useActiveCopy, type ActiveCopy } from "./copy";
import { fuzzyMatch, fuzzyScore } from "./fuzzy";
import { CommandIcon, FileIcon } from "./icons";
import { modKey } from "./keys";
import {
  CHIPS,
  HELP,
  parseInput,
  PLACEHOLDER,
  PREFIX,
  prefixOf,
  type BaseMode,
  type Mode,
} from "./modes";
import { rememberFile } from "./recentFiles";
import {
  FILES_IN_ALL,
  STATUS_LABEL,
  STATUS_LETTER,
  useFileSource,
  useLineSource,
  useSymbolSource,
  useTextSource,
  type Item,
  type Source,
} from "./sources";
import { VirtualList, type ListSection } from "./VirtualList";
import "./palette.css";

const RECENT_KEY = "hedy.palette.recent";
const MAX_RECENT = 8;
const SHOWN_RECENT = 5;
const MAX_RESULTS = 60;
const PAGE_STEP = 8;
/** "Preguntar «…»": not a stable command, never a recent one. */
const ASK_ID = "ask:query";

const GROUP_TITLE: Record<GroupKey | "recent", MessageKey> = {
  recent: "palette.group.recent",
  pending: "palette.group.pending",
  nav: "palette.group.nav",
  code: "palette.group.code",
  projects: "palette.group.projects",
  tasks: "palette.group.tasks",
  accounts: "palette.group.accounts",
  actions: "palette.group.actions",
};

function readRecent(): string[] {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]");
    return Array.isArray(value) ? value.filter((v): v is string => typeof v === "string") : [];
  } catch {
    return [];
  }
}

function writeRecent(ids: string[]) {
  try {
    localStorage.setItem(RECENT_KEY, JSON.stringify(ids.slice(0, MAX_RECENT)));
  } catch {
    // Storage blocked: recents are a convenience.
  }
}

interface Section {
  key: string;
  title: string;
  commands: Command[];
}

/** Recent first and then by group with no query; by score (recent ones a little up) with one. */
export function arrange(commands: Command[], query: string, recent: string[], title: (key: GroupKey | "recent") => string): Section[] {
  if (query.trim()) {
    const scored = commands
      .map((command) => {
        const label = fuzzyScore(query, command.label);
        const all = label ?? fuzzyScore(query, `${command.label} ${command.detail ?? ""}`);
        if (all === null) return null;
        const score = (label ?? all * 0.7) + (recent.includes(command.id) ? 25 : 0);
        return { command, score };
      })
      .filter((x): x is { command: Command; score: number } => x !== null)
      .sort((a, b) => b.score - a.score)
      .slice(0, MAX_RESULTS);
    return scored.length ? [{ key: "results", title: "", commands: scored.map((x) => x.command) }] : [];
  }
  const byId = new Map(commands.map((c) => [c.id, c]));
  const recentCommands = recent
    .map((id) => byId.get(id))
    .filter((c): c is Command => !!c)
    .slice(0, SHOWN_RECENT);
  const sections: Section[] = [];
  if (recentCommands.length) sections.push({ key: "recent", title: title("recent"), commands: recentCommands });
  const shown = new Set(recentCommands.map((c) => c.id));
  for (const group of GROUP_ORDER) {
    const list = commands.filter((c) => c.group === group && !shown.has(c.id));
    if (list.length) sections.push({ key: group, title: title(group), commands: list });
  }
  return sections;
}

interface Request {
  base: BaseMode;
  text: string;
  /** Counts requests, so asking for the same mode again resets the field. */
  seq: number;
}

/** Whether a key press opens the palette, and on what. */
export function paletteShortcut(e: KeyboardEvent): { base: BaseMode; text: string } | "toggle" | null {
  const mod = (e.ctrlKey || e.metaKey) && !e.altKey;
  const key = e.key.toLowerCase();
  if (mod && !e.shiftKey && key === "k") return "toggle";
  if (mod && !e.shiftKey && key === "p") return { base: "files", text: "" };
  if (mod && e.shiftKey && key === "p") return { base: "files", text: PREFIX.commands };
  if (e.key === "F1" && !e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey) return { base: "files", text: PREFIX.commands };
  return null;
}

export function CommandPalette({
  open,
  onOpenChange,
  overview,
  view,
  onNavigate,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  overview: Overview;
  /** What is on screen: its project or task is the copy files are searched in. */
  view: View;
  onNavigate: (view: View) => void;
}) {
  const [request, setRequest] = useState<Request>({ base: "all", text: "", seq: 0 });
  const active = useActiveCopy(view);
  const nav = useFileNavigator();

  const show = useCallback(
    (base: BaseMode, text: string) => {
      setRequest((r) => ({ base, text, seq: r.seq + 1 }));
      onOpenChange(true);
    },
    [onOpenChange],
  );
  // Opened from elsewhere (the top bar's button), it starts on everything.
  const close = useCallback(() => {
    onOpenChange(false);
    setRequest((r) => ({ base: "all", text: "", seq: r.seq + 1 }));
  }, [onOpenChange]);

  // Ctrl/⌘ K, Ctrl/⌘ P, F1 and Ctrl/⌘ ⇧ P anywhere, except in a terminal, where they belong to the shell.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.target instanceof Element && e.target.closest(".xterm")) return;
      const shortcut = paletteShortcut(e);
      if (!shortcut) return;
      e.preventDefault();
      if (shortcut !== "toggle") show(shortcut.base, shortcut.text);
      else if (open) close();
      else show("all", "");
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, show, close]);

  // Every file opened anywhere becomes a recent one of its project.
  const target = nav.current;
  const project =
    target?.copy.kind === "folder"
      ? target.copy.project
      : target && active && sameCopy(active.copy, target.copy)
        ? active.project
        : null;
  useEffect(() => {
    if (target && project) rememberFile(project, target.path);
    // Once per request, not on every render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [nav.seq, project]);

  if (!open) return null;
  return (
    <PalettePanel
      request={request}
      overview={overview}
      active={active}
      onNavigate={onNavigate}
      onClose={close}
      onReopen={(text) => show("files", text)}
    />
  );
}

function shownKeys(keys: string[]) {
  return (
    <kbd className="palette__keys" aria-label={keys.join(" ")}>
      {keys.map((k) => (
        <kbd key={k}>{k}</kbd>
      ))}
    </kbd>
  );
}

/** `text` with the characters at `hits` marked. */
function Highlight({ text, hits }: { text: string; hits?: number[] }) {
  if (!hits?.length) return <>{text}</>;
  const marked = new Set(hits);
  const parts: { text: string; hit: boolean }[] = [];
  for (let i = 0; i < text.length; i++) {
    const hit = marked.has(i);
    const last = parts[parts.length - 1];
    if (last && last.hit === hit) last.text += text[i];
    else parts.push({ text: text[i] ?? "", hit });
  }
  return (
    <>
      {parts.map((p, i) =>
        p.hit ? (
          <mark key={i} className="palette__hit">
            {p.text}
          </mark>
        ) : (
          <span key={i}>{p.text}</span>
        ),
      )}
    </>
  );
}

function PalettePanel({
  request,
  overview,
  active,
  onNavigate,
  onClose,
  onReopen,
}: {
  request: Request;
  overview: Overview;
  active: ActiveCopy | null;
  onNavigate: (view: View) => void;
  onClose: () => void;
  onReopen: (text: string) => void;
}) {
  const t = useT();
  const id = useId();
  const input = useRef<HTMLInputElement>(null);
  const [seen, setSeen] = useState(request.seq);
  const [base, setBase] = useState<BaseMode>(request.base);
  const [text, setText] = useState(request.text);
  const [page, setPage] = useState<Page | null>(null);
  const [activeIndex, setActive] = useState(0);
  const [recent, setRecent] = useState(readRecent);
  const create = useCreate();
  const openFile = useOpenFile(active, onNavigate);

  // Ctrl/⌘ P, F1… while open: start over in that mode.
  if (seen !== request.seq) {
    setSeen(request.seq);
    setBase(request.base);
    setText(request.text);
    setPage(null);
    setActive(0);
  }

  const parsed = page ? { mode: "commands" as Mode, query: text } : parseInput(text, base);
  const { mode, query } = parsed;
  const trimmed = query.trim();

  const appCommands = usePaletteCommands({ overview, open: true, onNavigate, t });
  const codeCommands = useCodeCommands({ active, t, onNavigate, reopen: onReopen });
  const commands = [...appCommands, ...codeCommands];

  const files = useFileSource({
    active,
    query,
    enabled: !page && (mode === "files" || (mode === "all" && trimmed.length > 0)),
    open: openFile,
    t,
    limit: mode === "all" ? FILES_IN_ALL : undefined,
  });
  const line = useLineSource({ query, enabled: mode === "line", t });
  const symbols = useSymbolSource({ query, enabled: mode === "symbols", t });
  const textSearch = useTextSource({ active, query, enabled: mode === "text", open: openFile, t });

  const switchTo = (next: Mode, keepQuery = true) => {
    const rest = keepQuery ? query : "";
    if (next === "files" || next === "all") {
      setBase(next);
      setText(rest);
    } else {
      setText(prefixOf(next) + rest);
    }
    setPage(null);
    setActive(0);
    input.current?.focus();
  };

  const commandItem = (c: Command, sectionKey: string, q: string): Item => ({
    key: `${sectionKey}-${c.id}`,
    commandId: page ? undefined : c.id,
    label: c.label,
    labelHits: q ? fuzzyMatch(q, c.label)?.positions : undefined,
    detail: c.detail,
    color: c.color,
    keys: c.keys,
    more: !!c.page,
    primary: c.page ? "palette.action.choose" : "palette.action.run",
    stay: !!c.page || !!c.switchTo,
    run: () => {
      if (c.page) {
        setPage(c.page());
        setText("");
        setActive(0);
      } else if (c.switchTo) {
        switchTo(c.switchTo, false);
      } else {
        c.run?.();
      }
    },
  });
  const commandSections = (list: Command[], q: string, withRecent: boolean): ListSection<Item>[] =>
    arrange(list, q, withRecent ? recent : [], (k) => t(GROUP_TITLE[k])).map((s) => ({
      key: s.key,
      title: s.title || undefined,
      items: s.commands.map((c) => commandItem(c, s.key, q)),
    }));

  let source: Source;
  let empty: string | null = null;
  if (page) {
    source = { sections: commandSections(page.commands, query, false) };
  } else if (mode === "all" || mode === "commands") {
    const sections = commandSections(commands, query, true);
    if (mode === "all" && trimmed) {
      sections.push(...files.sections);
      // Whatever was typed can also be asked, outside any project (like Spotlight).
      const nothing = sections.length === 0;
      if (overview.accounts.length > 0) {
        sections.push({
          key: "ask",
          items: [
            {
              key: ASK_ID,
              label: t("palette.ask", { query: trimmed }),
              detail: t("create.quickQuestion"),
              primary: "palette.action.ask",
              run: () => create.quickQuestion(trimmed),
            },
          ],
        });
      }
      if (nothing) empty = t("palette.empty", { query: trimmed });
    } else if (!sections.length && trimmed) {
      empty = t("palette.empty", { query: trimmed });
    }
    source = { sections, loading: mode === "all" && files.loading };
  } else if ((mode === "files" || mode === "text") && !active) {
    // No code on screen: say so, and offer where to look.
    source = {
      sections: overview.projects.length
        ? [
            {
              key: "projects",
              title: t("palette.group.projects"),
              items: overview.projects.map((p) => ({
                key: `project:${p.path}`,
                label: p.name,
                detail: p.path,
                color: projectColor(p, overview.accounts),
                primary: "palette.action.choose" as const,
                stay: true,
                run: () => {
                  onNavigate({ screen: "workspace", project: p.path });
                  setActive(0);
                },
              })),
            },
          ]
        : [],
    };
    empty = t("palette.noProject");
  } else if (mode === "files") {
    source = files;
  } else if (mode === "line") {
    source = line;
  } else if (mode === "symbols") {
    source = symbols;
  } else if (mode === "text") {
    source = textSearch;
  } else {
    source = {
      sections: [
        {
          key: "help",
          items: HELP.map((h) => ({
            key: `help:${h.mode}`,
            label: t(h.label),
            detail: t(h.detail),
            keys: h.mode === "files" ? [modKey(), "P"] : [prefixOf(h.mode)],
            primary: "palette.action.choose" as const,
            stay: true,
            run: () => switchTo(h.mode, false),
          })),
        },
      ],
    };
  }

  const sections = source.sections;
  const flat = sections.flatMap((s) => s.items);
  const current = Math.min(activeIndex, Math.max(flat.length - 1, 0));
  const activeItem = flat[current];
  const optionId = (index: number) => `${id}-option-${index}`;

  // Focus in on open, back where it was on close.
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    input.current?.focus();
    return () => previous?.focus?.();
  }, []);

  const run = (item: Item | undefined, alt = false) => {
    if (!item) return;
    if (item.commandId && item.key !== ASK_ID) {
      const next = [item.commandId, ...recent.filter((r) => r !== item.commandId)];
      setRecent(next);
      writeRecent(next);
    }
    if (item.stay) {
      item.run(alt);
      return;
    }
    onClose();
    item.run(alt);
  };

  const back = () => {
    setPage(null);
    setText("");
    setActive(0);
  };

  const onKeyDown = (e: ReactKeyboardEvent<HTMLInputElement>) => {
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        setActive(flat.length ? (current + 1) % flat.length : 0);
        break;
      case "ArrowUp":
        e.preventDefault();
        setActive(flat.length ? (current - 1 + flat.length) % flat.length : 0);
        break;
      case "PageDown":
        e.preventDefault();
        setActive(Math.max(Math.min(current + PAGE_STEP, flat.length - 1), 0));
        break;
      case "PageUp":
        e.preventDefault();
        setActive(Math.max(current - PAGE_STEP, 0));
        break;
      case "Home":
      case "End":
        // In the field they move the caret; with Ctrl/⌘, through the list.
        if (e.ctrlKey || e.metaKey) {
          e.preventDefault();
          setActive(e.key === "Home" ? 0 : Math.max(flat.length - 1, 0));
        }
        break;
      case "Enter":
        e.preventDefault();
        run(activeItem, e.ctrlKey || e.metaKey);
        break;
      case "Escape":
        e.preventDefault();
        e.stopPropagation();
        if (page) back();
        else onClose();
        break;
      case "Backspace":
        if (page && text === "") {
          e.preventDefault();
          back();
        }
        break;
      case "Tab":
        // The field keeps the focus; in `?`, Tab completes the mode's prefix.
        e.preventDefault();
        if (mode === "help" && activeItem) activeItem.run(false);
        break;
    }
  };

  const onPick = (index: number, e: MouseEvent) => {
    setActive(index);
    run(flat[index], e.ctrlKey || e.metaKey);
  };

  const count =
    flat.length === 0
      ? t("palette.count.none")
      : flat.length === 1
        ? t("palette.count.one")
        : t("palette.count.other", { n: flat.length });
  const title = page?.title ?? t("palette.title");
  const codeMode = mode === "files" || mode === "text" || mode === "line" || mode === "symbols";

  return (
    <div className="palette-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="palette" role="dialog" aria-modal="true" aria-label={title}>
        {page ? (
          <div className="palette__page">
            <button type="button" className="palette__back" tabIndex={-1} onClick={back}>
              ← {t("palette.back")}
            </button>
            <p className="palette__page-title">{page.title}</p>
            {page.hint && <p className="hint">{page.hint}</p>}
          </div>
        ) : (
          <div className="palette__bar">
            <div className="palette__chips" role="group" aria-label={t("palette.modes")}>
              {CHIPS.map((chip) => (
                <button
                  key={chip.mode}
                  type="button"
                  className="palette__chip"
                  tabIndex={-1}
                  aria-pressed={mode === chip.mode}
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={() => switchTo(chip.mode)}
                >
                  {chip.mode !== "files" && (
                    <span className="palette__chip-prefix" aria-hidden>
                      {prefixOf(chip.mode)}
                    </span>
                  )}
                  {t(chip.label)}
                </button>
              ))}
            </div>
            {codeMode && <Context mode={mode} active={active} overview={overview} />}
          </div>
        )}
        <div className="palette__field">
          <input
            ref={input}
            className="palette__input"
            role="combobox"
            aria-expanded="true"
            aria-controls={`${id}-list`}
            aria-autocomplete="list"
            aria-activedescendant={flat.length ? optionId(current) : undefined}
            aria-label={title}
            placeholder={t(page ? "palette.placeholder" : PLACEHOLDER[mode])}
            spellCheck={false}
            autoComplete="off"
            value={text}
            onChange={(e) => {
              setText(e.target.value);
              setActive(0);
            }}
            onKeyDown={onKeyDown}
          />
          {source.loading && <span className="palette__spinner" aria-hidden />}
        </div>
        {empty && flat.some((i) => i.key !== ASK_ID) && <p className="palette__lead">{empty}</p>}
        <VirtualList
          id={`${id}-list`}
          label={t("palette.results")}
          sections={sections}
          active={current}
          optionId={optionId}
          itemKey={(item) => item.key}
          onHover={setActive}
          onPick={onPick}
          footer={
            flat.length === 0 ? (
              source.loading ? (
                <div className="palette__skeleton" aria-hidden>
                  {[0, 1, 2, 3, 4].map((i) => (
                    <span key={i} style={{ width: `${70 - i * 9}%` }} />
                  ))}
                </div>
              ) : (
                (empty ?? source.message) && (
                  <p className={source.error ? "palette__empty palette__empty--error" : "palette__empty"}>
                    {empty ?? source.message}
                  </p>
                )
              )
            ) : (
              empty && flat.every((i) => i.key === ASK_ID) && <p className="palette__empty">{empty}</p>
            )
          }
        >
          {(item) => <Row item={item} t={t} />}
        </VirtualList>
        <div className="palette__foot">
          <div className="palette__foot-left">
            {text === "" && !page ? (
              <span className="palette__legend">
                {(["commands", "line", "symbols", "text", "help"] as const).map((m) => (
                  <button
                    key={m}
                    type="button"
                    tabIndex={-1}
                    className="palette__legend-item"
                    onMouseDown={(e) => e.preventDefault()}
                    onClick={() => switchTo(m, false)}
                  >
                    <kbd>{PREFIX[m]}</kbd>
                    {t(`palette.legend.${m}`)}
                  </button>
                ))}
              </span>
            ) : (
              <span className="palette__note">{source.note ?? ""}</span>
            )}
          </div>
          {activeItem && (
            <div className="palette__actions" aria-hidden>
              <span>
                <kbd>↵</kbd>
                {t(activeItem.primary)}
              </span>
              {activeItem.alt && (
                <span>
                  <kbd>{modKey()}</kbd>
                  <kbd>↵</kbd>
                  {t(activeItem.alt)}
                </span>
              )}
            </div>
          )}
        </div>
        <p className="sr-only" role="status">
          {count}
        </p>
      </div>
    </div>
  );
}

/** Which copy (files, text) or which file (line, symbols) the mode reads: never mixed silently. */
function Context({ mode, active, overview }: { mode: Mode; active: ActiveCopy | null; overview: Overview }) {
  const t = useT();
  const api = useApi();
  const nav = useFileNavigator();
  const copy = active?.copy;
  const info = useQuery({
    queryKey: copy ? codeKeys.info(copy) : ["code", "none"],
    queryFn: copy && (mode === "files" || mode === "text") ? () => api.codeCopy(copy) : skipToken,
  });
  if (mode === "line" || mode === "symbols") {
    const file = nav.current;
    if (!file) return null;
    return (
      <span className="palette__context" title={file.path}>
        <FileIcon path={file.path} />
        {file.path.slice(file.path.lastIndexOf("/") + 1)}
      </span>
    );
  }
  if (!active) return null;
  const project = overview.projects.find((p) => p.path === active.project)?.name ?? active.project;
  const label =
    active.copy.kind === "folder"
      ? t("palette.copy.folder")
      : t("palette.copy.task", { title: info.data?.title ?? "…" });
  return (
    <span className="palette__context" title={`${project} · ${label}`}>
      <span className="palette__context-project">{project}</span>
      <span aria-hidden>·</span>
      <span className="palette__context-copy">{label}</span>
    </span>
  );
}

function Row({ item, t }: { item: Item; t: Translate }) {
  return (
    <>
      {item.color ? (
        <span className="dot" style={{ background: item.color }} aria-hidden />
      ) : (
        <span className="palette__icon" aria-hidden>
          {item.icon ?? (item.commandId || item.key === ASK_ID ? <CommandIcon /> : null)}
        </span>
      )}
      <span className={item.mono ? "palette__label palette__label--mono" : "palette__label"}>
        <Highlight text={item.label} hits={item.labelHits} />
      </span>
      {item.detail && (
        <span className="palette__detail">
          <Highlight text={item.detail} hits={item.detailHits} />
        </span>
      )}
      {item.status && (
        <span
          className={`palette__status palette__status--${item.status}`}
          title={t(STATUS_LABEL[item.status])}
          aria-label={t(STATUS_LABEL[item.status])}
        >
          {STATUS_LETTER[item.status]}
        </span>
      )}
      {item.keys && shownKeys(item.keys)}
      {item.more && (
        <span className="palette__more" aria-hidden>
          ›
        </span>
      )}
    </>
  );
}
