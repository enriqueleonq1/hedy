// What a working mascot shows, from what the agent is doing. Pure: the hook
// that adds the clock lives in `tasks/useMascot.ts`.

import type { AgentEvent } from "../../api/types";
import type { MessageKey } from "../../i18n";

/** The variants of the `working` state; `edit` is the default laptop pose. */
export type WorkingActivity =
  | "reading"
  | "search"
  | "edit"
  | "command"
  | "test"
  | "web"
  | "think"
  | "subagents"
  | "waiting"
  | "long";

export const WORKING_ACTIVITIES: WorkingActivity[] = [
  "reading",
  "search",
  "edit",
  "command",
  "test",
  "web",
  "think",
  "subagents",
  "waiting",
  "long",
];

export const DEFAULT_ACTIVITY: WorkingActivity = "edit";

/** Every timing of the working mascot, in one place. */
export const ACTIVITY_TIMING = {
  /** No event for this long while the agent runs: it is waiting on something slow. */
  waitingAfterMs: 20_000,
  /** A turn longer than this turns commands, tests and waits into the "long" pose. */
  longAfterMs: 10 * 60_000,
  /** Each activity stays at least this long, so the mascot does not flicker. */
  minHoldMs: 2_500,
  /** The words of an activity rotate this often. */
  wordEveryMs: 3_000,
  /** The cross-fade between two activities. */
  fadeMs: 400,
} as const;

/** Only these become "long": reading or editing for a while is not a struggle. */
const LONG_FROM: WorkingActivity[] = ["command", "test", "waiting"];

/** Accessible name and gallery caption of each activity. */
export const ACTIVITY_LABEL: Record<WorkingActivity, MessageKey> = {
  reading: "mascot.activity.reading",
  search: "mascot.activity.search",
  edit: "mascot.activity.edit",
  command: "mascot.activity.command",
  test: "mascot.activity.test",
  web: "mascot.activity.web",
  think: "mascot.activity.think",
  subagents: "mascot.activity.subagents",
  waiting: "mascot.activity.waiting",
  long: "mascot.activity.long",
};

/** The words that rotate next to a working mascot. */
export const ACTIVITY_WORDS: Record<WorkingActivity, MessageKey[]> = {
  reading: ["mascot.word.reading.1", "mascot.word.reading.2", "mascot.word.reading.3"],
  search: ["mascot.word.search.1", "mascot.word.search.2", "mascot.word.search.3"],
  edit: ["mascot.word.edit.1", "mascot.word.edit.2", "mascot.word.edit.3"],
  command: ["mascot.word.command.1", "mascot.word.command.2", "mascot.word.command.3"],
  test: ["mascot.word.test.1", "mascot.word.test.2", "mascot.word.test.3"],
  web: ["mascot.word.web.1", "mascot.word.web.2", "mascot.word.web.3"],
  think: ["mascot.word.think.1", "mascot.word.think.2", "mascot.word.think.3"],
  subagents: ["mascot.word.subagents.1", "mascot.word.subagents.2", "mascot.word.subagents.3"],
  waiting: ["mascot.word.waiting.1", "mascot.word.waiting.2", "mascot.word.waiting.3"],
  long: ["mascot.word.long.1", "mascot.word.long.2", "mascot.word.long.3"],
};

/**
 * Test runners, as whole words: a superset of the backend's `looks_like_tests`
 * (adapters already flag most test runs as `testsRun`).
 */
const TEST_COMMAND =
  /(^|[\s;&|(/])(cargo (test|nextest)|(python3? -m )?pytest|(npm|pnpm|yarn|bun|deno)( run)? test|npx (vitest|jest|playwright test)|vitest|jest|go test|mix test|rspec|phpunit|dotnet test|mvn( \S+)* test|gradlew? test|make (test|check)|tox|ctest|swift test|flutter test|rake test|bundle exec rspec)($|[\s;&|):])/;

/** Whether a shell command looks like a test run. */
export function looksLikeTests(command: string): boolean {
  return TEST_COMMAND.test(command.trim());
}

/**
 * Tool names of every provider, normalized (lower case, letters and digits
 * only): Claude Code, Codex, Gemini CLI, Kimi Code, Hedy's API agent, and the
 * kinds of ACP tool calls (`read`, `edit`, `search`, `execute`, `fetch`,
 * `think`...), which Hedy also reports as Claude names.
 */
const TOOLS: Record<string, WorkingActivity> = {
  // Reading.
  read: "reading",
  readfile: "reading",
  readmanyfiles: "reading",
  notebookread: "reading",
  glob: "reading",
  ls: "reading",
  listdirectory: "reading",
  listfiles: "reading",
  viewimage: "reading",
  readmediafile: "reading",
  // Searching.
  grep: "search",
  search: "search",
  searchfilecontent: "search",
  codebasesearch: "search",
  toolsearch: "search",
  // Editing.
  edit: "edit",
  multiedit: "edit",
  write: "edit",
  writefile: "edit",
  notebookedit: "edit",
  applypatch: "edit",
  replace: "edit",
  strreplacefile: "edit",
  filechange: "edit",
  delete: "edit",
  move: "edit",
  // Commands.
  bash: "command",
  bashoutput: "command",
  killshell: "command",
  killbash: "command",
  shell: "command",
  localshell: "command",
  execute: "command",
  runshellcommand: "command",
  commandexecution: "command",
  // The web.
  webfetch: "web",
  websearch: "web",
  fetch: "web",
  fetchurl: "web",
  searchweb: "web",
  googlewebsearch: "web",
  // Subagents.
  task: "subagents",
  agent: "subagents",
  subagent: "subagents",
  // Thinking and planning.
  think: "think",
  todowrite: "think",
  settodolist: "think",
  updateplan: "think",
  exitplanmode: "think",
  enterplanmode: "think",
  switchmode: "think",
  reasoning: "think",
  savememory: "think",
};

/** Fallbacks for tools no table knows (MCP servers, new CLIs), by the words in their name. */
const KEYWORDS: [RegExp, WorkingActivity][] = [
  [/web|fetch|browse|url|http/, "web"],
  [/agent|delegate|task/, "subagents"],
  [/test/, "test"],
  [/shell|bash|exec|command|terminal|run/, "command"],
  [/search|grep|find|query/, "search"],
  [/edit|write|patch|replace|create|update|delete|rename|move/, "edit"],
  [/read|view|list|glob|open|get|show|cat/, "reading"],
  [/think|plan|todo|reason/, "think"],
];

/**
 * What a tool means for the mascot, or `null` when nothing says. `command`
 * is the shell command, when known, to tell tests apart.
 */
export function activityOfTool(tool: string, command = ""): WorkingActivity | null {
  // `mcp__server__tool`: the tool's own name says more than the server's.
  const name = tool.split("__").pop() ?? tool;
  const key = name.toLowerCase().replace(/[^a-z0-9]/g, "");
  const activity = TOOLS[key] ?? KEYWORDS.find(([pattern]) => pattern.test(key))?.[1] ?? null;
  if (activity === "command" && looksLikeTests(command)) return "test";
  return activity;
}

/** The shell command in a tool's input (`command` as a string or argv). */
function commandOf(input: unknown): string {
  const command = (input as { command?: unknown } | null)?.command;
  if (typeof command === "string") return command;
  if (Array.isArray(command)) return command.filter((part) => typeof part === "string").join(" ");
  return "";
}

/** What an event says the agent is doing; `null` when it says nothing new. */
export function activityOfEvent(event: AgentEvent): WorkingActivity | null {
  switch (event.kind) {
    case "fileEdited":
      return "edit";
    case "commandRun":
      return event.tests || looksLikeTests(event.command) ? "test" : "command";
    case "testsRun":
      return "test";
    case "toolUsed":
      return activityOfTool(event.tool, event.summary);
    case "toolStarted":
      return activityOfTool(event.tool, event.detail);
    case "permissionRequest":
      return activityOfTool(event.tool, commandOf(event.input));
    case "phaseStarted":
      return event.phase === "tests" ? "test" : "think";
    case "userMessage":
    case "sessionStarted":
    case "message":
    case "plan":
    case "steps":
      return "think";
    default:
      return null;
  }
}

/** The activity of the latest event that says one; thinking before any tool. */
export function activityOfEvents(events: AgentEvent[]): WorkingActivity {
  for (let i = events.length - 1; i >= 0; i--) {
    const event = events[i];
    const activity = event ? activityOfEvent(event) : null;
    if (activity) return activity;
  }
  return "think";
}

/**
 * The clock's say: silence (no event for `waitingAfterMs`) means waiting,
 * and a turn longer than `longAfterMs` makes commands, tests and waits sweat.
 */
export function withClock(
  activity: WorkingActivity,
  { silent, longTurn }: { silent: boolean; longTurn: boolean },
): WorkingActivity {
  const current = silent ? "waiting" : activity;
  return longTurn && LONG_FROM.includes(current) ? "long" : current;
}
