// What the palette's field means, read from its first character (as in VS
// Code's quick open): `>` commands, `:` a line of the active file, `@` a symbol
// of it, `#` text across the copy, `?` the list of modes. Without a prefix it is
// the mode the palette was opened in: files (Ctrl/⌘ P) or everything (Ctrl/⌘ K).

import type { MessageKey } from "../../i18n";

export type Mode = "all" | "files" | "commands" | "line" | "symbols" | "text" | "help";

/** What no prefix means. */
export type BaseMode = "all" | "files";

export const PREFIX: Record<Exclude<Mode, BaseMode>, string> = {
  commands: ">",
  line: ":",
  symbols: "@",
  text: "#",
  help: "?",
};

const BY_PREFIX = new Map(Object.entries(PREFIX).map(([mode, prefix]) => [prefix, mode as Mode]));

export interface Parsed {
  mode: Mode;
  /** The field without its prefix. */
  query: string;
}

export function parseInput(value: string, base: BaseMode): Parsed {
  const mode = BY_PREFIX.get(value.charAt(0));
  return mode ? { mode, query: value.slice(1) } : { mode: base, query: value };
}

/** What a mode's field starts with (`""` for the base ones). */
export function prefixOf(mode: Mode): string {
  return mode === "all" || mode === "files" ? "" : PREFIX[mode];
}

/** `120`, `120:5` or `120,5` (column ignored by the viewer, accepted as VS Code does). */
export function parseLine(query: string): { line: number; column: number | null } | null {
  const m = /^\s*(\d+)(?:\s*[:,]\s*(\d+))?\s*$/.exec(query);
  if (!m) return null;
  const line = Number(m[1]);
  return line >= 1 ? { line, column: m[2] ? Number(m[2]) : null } : null;
}

/** `impuestos:16` in files mode: the file query and the line to open it at. */
export function splitFileLine(query: string): { query: string; line: number | null } {
  const m = /^(.*?):(\d+)(?::\d+)?\s*$/.exec(query);
  if (!m || !m[1]?.trim()) return { query, line: null };
  const line = Number(m[2]);
  return { query: m[1], line: line >= 1 ? line : null };
}

/** The clickable chips at the top, in order. */
export const CHIPS: { mode: Mode; label: MessageKey }[] = [
  { mode: "files", label: "palette.mode.files" },
  { mode: "commands", label: "palette.mode.commands" },
  { mode: "symbols", label: "palette.mode.symbols" },
  { mode: "text", label: "palette.mode.text" },
];

/** The rows of `?`, in order: each mode, how to reach it and what it does. */
export const HELP: { mode: Mode; label: MessageKey; detail: MessageKey }[] = [
  { mode: "files", label: "palette.help.files", detail: "palette.help.files.detail" },
  { mode: "commands", label: "palette.help.commands", detail: "palette.help.commands.detail" },
  { mode: "line", label: "palette.help.line", detail: "palette.help.line.detail" },
  { mode: "symbols", label: "palette.help.symbols", detail: "palette.help.symbols.detail" },
  { mode: "text", label: "palette.help.text", detail: "palette.help.text.detail" },
];

export const PLACEHOLDER: Record<Mode, MessageKey> = {
  all: "palette.placeholder",
  files: "palette.placeholder.files",
  commands: "palette.placeholder.commands",
  line: "palette.placeholder.line",
  symbols: "palette.placeholder.symbols",
  text: "palette.placeholder.text",
  help: "palette.placeholder.help",
};
