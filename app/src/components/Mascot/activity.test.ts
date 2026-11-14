import type { AgentEvent } from "../../api/types";
import { messages } from "../../i18n/messages";
import {
  ACTIVITY_LABEL,
  ACTIVITY_WORDS,
  activityOfEvent,
  activityOfEvents,
  activityOfTool,
  looksLikeTests,
  withClock,
  WORKING_ACTIVITIES,
  type WorkingActivity,
} from "./activity";

describe("activityOfTool", () => {
  const cases: Record<WorkingActivity, string[]> = {
    reading: [
      // Claude Code and Hedy's API agent.
      "Read", "Glob", "LS", "NotebookRead",
      // Gemini CLI.
      "read_file", "read_many_files", "list_directory", "glob",
      // Kimi Code.
      "ReadFile", "ReadMediaFile",
      // Codex.
      "view_image",
      // ACP tool call kind.
      "read",
    ],
    search: ["Grep", "search_file_content", "search", "mcp__hedy-memory__search_notes"],
    edit: [
      "Edit", "Write", "MultiEdit", "NotebookEdit",
      "apply_patch", "fileChange",
      "replace", "write_file",
      "WriteFile", "StrReplaceFile",
      "edit", "delete", "move",
    ],
    command: ["Bash", "BashOutput", "KillShell", "shell", "local_shell", "commandExecution", "run_shell_command", "Shell", "execute"],
    test: [],
    web: ["WebFetch", "WebSearch", "web_search", "web_fetch", "google_web_search", "SearchWeb", "FetchURL", "fetch"],
    think: ["TodoWrite", "ExitPlanMode", "update_plan", "SetTodoList", "Think", "think", "switch_mode", "reasoning"],
    subagents: ["Task", "Agent", "subagent", "mcp__team__delegate_work"],
    waiting: [],
    long: [],
  };
  for (const [activity, tools] of Object.entries(cases)) {
    for (const tool of tools) {
      it(`maps ${tool} to ${activity}`, () => {
        expect(activityOfTool(tool)).toBe(activity);
      });
    }
  }

  it("tells test runs from other commands", () => {
    expect(activityOfTool("Bash", "cargo test -p core")).toBe("test");
    expect(activityOfTool("execute", "npx vitest run")).toBe("test");
    expect(activityOfTool("run_shell_command", "ls -la")).toBe("command");
  });

  it("knows nothing of tools that say nothing", () => {
    expect(activityOfTool("Tool")).toBeNull();
    expect(activityOfTool("mcp__x__ping")).toBeNull();
  });
});

describe("looksLikeTests", () => {
  it.each([
    "cargo test",
    "cd api && cargo test -p core",
    "cargo nextest run",
    "pytest tests/test_x.py",
    "python -m pytest -q",
    "npm test",
    "npm run test -- carrito",
    "pnpm test",
    "yarn test",
    "bun test",
    "npx vitest run",
    "vitest",
    "jest --watch=false",
    "go test ./...",
    "mix test",
    "bundle exec rspec",
    "./gradlew test",
    "mvn -q test",
    "dotnet test",
    "make test",
  ])("%s is a test run", (command) => {
    expect(looksLikeTests(command)).toBe(true);
  });

  it.each(["ls -la", "npm run build", "cargo build", "git log --oneline", "echo latest", "cat attest.txt", "detox build"])(
    "%s is not",
    (command) => {
      expect(looksLikeTests(command)).toBe(false);
    },
  );
});

describe("activityOfEvent", () => {
  it("maps every kind of event", () => {
    const cases: [AgentEvent, WorkingActivity | null][] = [
      [{ kind: "fileEdited", path: "a.ts" }, "edit"],
      [{ kind: "commandRun", command: "npm run build", tests: false }, "command"],
      [{ kind: "commandRun", command: "npm run build", tests: true }, "test"],
      [{ kind: "commandRun", command: "go test ./...", tests: false }, "test"],
      [{ kind: "testsRun", command: "pytest", passed: false }, "test"],
      [{ kind: "toolUsed", tool: "Read", summary: "a.ts" }, "reading"],
      [{ kind: "toolUsed", tool: "Grep", summary: "crear_cita" }, "search"],
      [{ kind: "toolUsed", tool: "WebFetch", summary: "docs" }, "web"],
      [{ kind: "toolUsed", tool: "Task", summary: "revisar" }, "subagents"],
      [{ kind: "toolUsed", tool: "Tool", summary: "?" }, null],
      [{ kind: "permissionRequest", requestId: "r", tool: "Bash", summary: "", input: { command: "npm test" } }, "test"],
      [{ kind: "permissionRequest", requestId: "r", tool: "Bash", summary: "", input: { command: ["git", "status"] } }, "command"],
      [{ kind: "permissionRequest", requestId: "r", tool: "Edit", summary: "", input: null }, "edit"],
      [{ kind: "phaseStarted", phase: "plan", agent: "Claude", account: "a" }, "think"],
      [{ kind: "phaseStarted", phase: "tests", agent: "Codex", account: "a" }, "test"],
      [{ kind: "message", text: "Voy a revisar" }, "think"],
      [{ kind: "plan", text: "1. Hacer", requestId: null }, "think"],
      [{ kind: "steps", steps: [] }, "think"],
      [{ kind: "userMessage", text: "hola" }, "think"],
      [{ kind: "sessionStarted", sessionId: "s", model: "opus" }, "think"],
      [{ kind: "usage", inputTokens: 1, outputTokens: 1, cacheReadTokens: 0, cacheWriteTokens: 0, costUsd: null }, null],
      [{ kind: "rateLimit", status: "allowed", utilization: null, resetsAt: null }, null],
      [{ kind: "permissionResolved", requestId: "r", decision: "allow", automatic: true, reason: null }, null],
      [{ kind: "finished", ok: true, summary: "", durationMs: 1 }, null],
      [{ kind: "error", message: "x" }, null],
    ];
    for (const [event, activity] of cases) {
      expect(activityOfEvent(event), JSON.stringify(event)).toBe(activity);
    }
  });

  it("takes the latest event that says something, thinking before any tool", () => {
    expect(activityOfEvents([])).toBe("think");
    expect(activityOfEvents([{ kind: "userMessage", text: "hola" }])).toBe("think");
    expect(
      activityOfEvents([
        { kind: "toolUsed", tool: "Read", summary: "a" },
        { kind: "usage", inputTokens: 1, outputTokens: 1, cacheReadTokens: 0, cacheWriteTokens: 0, costUsd: null },
        { kind: "toolUsed", tool: "Tool", summary: "?" },
      ]),
    ).toBe("reading");
    expect(
      activityOfEvents([
        { kind: "toolUsed", tool: "Read", summary: "a" },
        { kind: "message", text: "Ahora edito" },
      ]),
    ).toBe("think");
  });
});

describe("withClock", () => {
  it("waits after a silence and sweats on a long turn, but only while running things", () => {
    expect(withClock("reading", { silent: false, longTurn: false })).toBe("reading");
    expect(withClock("reading", { silent: true, longTurn: false })).toBe("waiting");
    expect(withClock("reading", { silent: true, longTurn: true })).toBe("long");
    expect(withClock("command", { silent: false, longTurn: true })).toBe("long");
    expect(withClock("test", { silent: false, longTurn: true })).toBe("long");
    expect(withClock("reading", { silent: false, longTurn: true })).toBe("reading");
    expect(withClock("edit", { silent: false, longTurn: true })).toBe("edit");
  });
});

describe("activity texts", () => {
  it("has a label and three words per activity, in both languages", () => {
    for (const activity of WORKING_ACTIVITIES) {
      expect(messages.en[ACTIVITY_LABEL[activity]]).not.toBe(messages.es[ACTIVITY_LABEL[activity]]);
      expect(ACTIVITY_WORDS[activity]).toHaveLength(3);
      for (const key of ACTIVITY_WORDS[activity]) {
        expect(messages.es[key].endsWith("…"), key).toBe(false);
        expect(messages.en[key], key).not.toBe("");
      }
    }
  });
});
