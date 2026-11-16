// The account's slash commands in the composer's "/" picker: the same query
// as the templates, a fuzzy match on the name, and the command a message
// starts with (the backend's `domain::command::slash_command`).

import type { AgentCommand } from "../api/types";

const fold = (text: string) =>
  text
    .normalize("NFD")
    .replace(/\p{M}/gu, "")
    .toLowerCase()
    .trim();

/** Every letter of `query`, in order, somewhere in `name`. */
function subsequence(name: string, query: string): boolean {
  let at = 0;
  for (const ch of name) {
    if (ch === query[at]) at += 1;
    if (at === query.length) return true;
  }
  return at === query.length;
}

/**
 * Commands matching what follows the `/`: names that start with it first,
 * then names containing it, then names with its letters in order, then
 * descriptions containing it. Once the query has a space, the user writes
 * arguments: no command matches.
 */
export function filterCommands(commands: AgentCommand[], query: string): AgentCommand[] {
  if (/\s/.test(query.trimStart())) return [];
  const q = fold(query);
  if (!q) return commands;
  const rank = (c: AgentCommand) => {
    const name = fold(c.name);
    if (name.startsWith(q)) return 0;
    if (name.includes(q)) return 1;
    if (subsequence(name, q)) return 2;
    if (c.description && fold(c.description).includes(q)) return 3;
    return 4;
  };
  return commands
    .map((c) => ({ c, r: rank(c) }))
    .filter(({ r }) => r < 4)
    .sort((a, b) => a.r - b.r)
    .map(({ c }) => c);
}

/** The name of the `/command` a message starts with (no `/` in it, so a path is not one). */
export function commandName(text: string): string | null {
  const match = /^\s*\/([\p{L}\p{N}_:.-]+)(?:\s|$)/u.exec(text);
  return match?.[1] ?? null;
}

/** The known command a message starts with, followed by a space: its arguments are being written. */
export function commandWithArguments(text: string, commands: AgentCommand[]): AgentCommand | null {
  const name = commandName(text);
  if (!name || !/^\s*\/\S+\s/.test(text)) return null;
  return commands.find((c) => c.name === name) ?? null;
}

/** What goes in the composer when a command is chosen: ready for its arguments. */
export function commandText(command: AgentCommand): string {
  return `/${command.name} `;
}
