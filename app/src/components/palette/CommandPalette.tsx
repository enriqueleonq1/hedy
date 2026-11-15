// Ctrl/⌘ K: a keyboard-first way to go anywhere and run a project's actions.
// An accessible combobox: the focus stays in the field, the active option is
// announced through aria-activedescendant.

import { useEffect, useId, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";

import type { Overview } from "../../api/types";
import { useT, type MessageKey } from "../../i18n";
import { useCreate } from "../../screens/create/CreateProvider";
import type { View } from "../Layout";
import { GROUP_ORDER, usePaletteCommands, type Command, type GroupKey, type Page } from "./commands";
import { fuzzyScore } from "./fuzzy";

const RECENT_KEY = "hedy.palette.recent";
const MAX_RECENT = 8;
const SHOWN_RECENT = 5;
const MAX_RESULTS = 60;
/** "Preguntar «…»": not a stable command, never a recent one. */
const ASK_ID = "ask:query";

const GROUP_TITLE: Record<GroupKey | "recent", MessageKey> = {
  recent: "palette.group.recent",
  pending: "palette.group.pending",
  nav: "palette.group.nav",
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

export function CommandPalette({
  open,
  onOpenChange,
  overview,
  onNavigate,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  overview: Overview;
  onNavigate: (view: View) => void;
}) {
  // Ctrl/⌘ K anywhere opens (or closes) it, except in a terminal, where it belongs to the shell.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.target instanceof Element && e.target.closest(".xterm")) return;
      if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && e.key.toLowerCase() === "k") {
        e.preventDefault();
        onOpenChange(!open);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onOpenChange]);

  if (!open) return null;
  return <PalettePanel overview={overview} onNavigate={onNavigate} onClose={() => onOpenChange(false)} />;
}

function PalettePanel({
  overview,
  onNavigate,
  onClose,
}: {
  overview: Overview;
  onNavigate: (view: View) => void;
  onClose: () => void;
}) {
  const t = useT();
  const id = useId();
  const input = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState("");
  const [page, setPage] = useState<Page | null>(null);
  const [active, setActive] = useState(0);
  const [recent, setRecent] = useState(readRecent);
  const all = usePaletteCommands({ overview, open: true, onNavigate, t });
  const create = useCreate();

  const sections = page
    ? arrange(page.commands, query, [], () => "")
    : arrange(all, query, recent, (k) => t(GROUP_TITLE[k]));
  // Whatever was typed can also be asked, outside any project (like Spotlight).
  const asked = query.trim();
  if (!page && asked && overview.accounts.length > 0) {
    sections.push({
      key: "ask",
      title: "",
      commands: [
        {
          id: ASK_ID,
          group: "actions",
          label: t("palette.ask", { query: asked }),
          detail: t("create.quickQuestion"),
          run: () => create.quickQuestion(asked),
        },
      ],
    });
  }
  const flat = sections.flatMap((s) => s.commands);
  const current = Math.min(active, Math.max(flat.length - 1, 0));
  const optionId = (index: number) => `${id}-option-${index}`;

  // Focus in on open, back where it was on close.
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    input.current?.focus();
    return () => previous?.focus?.();
  }, []);

  useEffect(() => {
    document.getElementById(optionId(current))?.scrollIntoView?.({ block: "nearest" });
  });

  const run = (command: Command | undefined) => {
    if (!command) return;
    if (!page && command.id !== ASK_ID) {
      const next = [command.id, ...recent.filter((r) => r !== command.id)];
      setRecent(next);
      writeRecent(next);
    }
    if (command.page) {
      setPage(command.page());
      setQuery("");
      setActive(0);
      return;
    }
    onClose();
    command.run?.();
  };

  const back = () => {
    setPage(null);
    setQuery("");
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
        setActive(Math.min(current + 8, flat.length - 1));
        break;
      case "PageUp":
        e.preventDefault();
        setActive(Math.max(current - 8, 0));
        break;
      case "Enter":
        e.preventDefault();
        run(flat[current]);
        break;
      case "Escape":
        e.preventDefault();
        e.stopPropagation();
        if (page) back();
        else onClose();
        break;
      case "Backspace":
        if (page && query === "") {
          e.preventDefault();
          back();
        }
        break;
      case "Tab":
        // The field keeps the focus; options are reached with the arrows.
        e.preventDefault();
        break;
    }
  };

  const count =
    flat.length === 0
      ? t("palette.count.none")
      : flat.length === 1
        ? t("palette.count.one")
        : t("palette.count.other", { n: flat.length });
  // Where each section starts in the flat list of options.
  const offsets = sections.map((_, s) => sections.slice(0, s).reduce((n, x) => n + x.commands.length, 0));

  return (
    <div className="palette-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="palette" role="dialog" aria-modal="true" aria-label={page?.title ?? t("palette.title")}>
        {page && (
          <div className="palette__page">
            <button type="button" className="palette__back" tabIndex={-1} onClick={back}>
              ← {t("palette.back")}
            </button>
            <p className="palette__page-title">{page.title}</p>
            {page.hint && <p className="hint">{page.hint}</p>}
          </div>
        )}
        <input
          ref={input}
          className="palette__input"
          role="combobox"
          aria-expanded="true"
          aria-controls={`${id}-list`}
          aria-autocomplete="list"
          aria-activedescendant={flat.length ? optionId(current) : undefined}
          aria-label={page?.title ?? t("palette.title")}
          placeholder={t("palette.placeholder")}
          spellCheck={false}
          autoComplete="off"
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setActive(0);
          }}
          onKeyDown={onKeyDown}
        />
        <div id={`${id}-list`} className="palette__list" role="listbox" aria-label={t("palette.results")}>
          {sections.map((section, s) => (
            <div key={section.key} role="group" aria-labelledby={section.title ? `${id}-${section.key}` : undefined}>
              {section.title && (
                <div id={`${id}-${section.key}`} className="palette__group" role="presentation">
                  {section.title}
                </div>
              )}
              {section.commands.map((command, c) => {
                const i = (offsets[s] ?? 0) + c;
                return (
                  <div
                    key={`${section.key}-${command.id}`}
                    id={optionId(i)}
                    role="option"
                    aria-selected={i === current}
                    className="palette__option"
                    onMouseMove={() => i !== current && setActive(i)}
                    onMouseDown={(e) => e.preventDefault()}
                    onClick={() => run(command)}
                  >
                    {command.color && <span className="dot" style={{ background: command.color }} aria-hidden />}
                    <span className="palette__label">{command.label}</span>
                    {command.detail && <span className="palette__detail">{command.detail}</span>}
                    {command.page && (
                      <span className="palette__more" aria-hidden>
                        ›
                      </span>
                    )}
                  </div>
                );
              })}
            </div>
          ))}
          {flat.every((c) => c.id === ASK_ID) && (
            <p className="palette__empty">{t("palette.empty", { query: query.trim() })}</p>
          )}
        </div>
        <p className="palette__foot" aria-hidden>
          {t("palette.keys")}
        </p>
        <p className="sr-only" role="status">
          {count}
        </p>
      </div>
    </div>
  );
}
