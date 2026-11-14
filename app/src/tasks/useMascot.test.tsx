import { act, renderHook } from "@testing-library/react";

import type { AgentEvent, Task } from "../api/types";
import { ACTIVITY_TIMING } from "../components/Mascot/activity";
import { useHeld, useMascot } from "./useMascot";

const NOW = new Date("2026-11-14T10:00:00Z").getTime();

function task(extra: Partial<Task> = {}): Task {
  return {
    id: "t1",
    project: "/p",
    mode: "project",
    title: "Tarea",
    prompt: "Tarea",
    account: "a",
    model: null,
    workspace: null,
    sessionId: null,
    status: "running",
    createdAt: NOW / 1000,
    stages: [],
    stage: 0,
    live: true,
    ...extra,
  };
}

const ask: AgentEvent = { kind: "userMessage", text: "hola" };
const read: AgentEvent = { kind: "toolUsed", tool: "Read", summary: "a.ts" };
const grep: AgentEvent = { kind: "toolUsed", tool: "Grep", summary: "x" };
const build: AgentEvent = { kind: "commandRun", command: "npm run build", tests: false };
const edit: AgentEvent = { kind: "fileEdited", path: "a.ts" };

function setup(initial: { task: Task | null; events: AgentEvent[] }) {
  return renderHook(({ task, events }) => useMascot(task, events), { initialProps: initial });
}

/** Lets the clock run, then the timers that React's effects set on the way. */
function advance(ms: number) {
  act(() => {
    vi.advanceTimersByTime(ms);
  });
  for (let i = 0; i < 3; i++) {
    act(() => {
      vi.advanceTimersByTime(0);
    });
  }
}

describe("useMascot", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW);
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("has an activity only while working", () => {
    const { result, rerender } = setup({ task: task(), events: [ask, read] });
    expect(result.current).toEqual({ state: "working", activity: "reading" });
    rerender({ task: task({ status: "waitingApproval" }), events: [ask, read] });
    expect(result.current).toEqual({ state: "thinking", activity: undefined });
    rerender({ task: null, events: [] });
    expect(result.current).toEqual({ state: "idle", activity: undefined });
  });

  it("waits after 20 s without events, and a new event ends the wait", () => {
    const { result, rerender } = setup({ task: task(), events: [ask, read] });
    advance(ACTIVITY_TIMING.waitingAfterMs - 1000);
    expect(result.current.activity).toBe("reading");
    advance(1000);
    expect(result.current.activity).toBe("waiting");

    rerender({ task: task(), events: [ask, read, edit] });
    advance(ACTIVITY_TIMING.minHoldMs);
    expect(result.current.activity).toBe("edit");
  });

  it("shows a running tool at once and never calls a long build a wait", () => {
    const started: AgentEvent = { kind: "toolStarted", tool: "Bash", detail: "cargo test" };
    const { result } = setup({ task: task(), events: [ask, started] });
    expect(result.current.activity).toBe("test");
    advance(ACTIVITY_TIMING.waitingAfterMs * 3);
    expect(result.current.activity).toBe("test");
  });

  it("sweats on a long turn while running commands, not while reading", () => {
    const { result, rerender } = setup({ task: task(), events: [ask, read] });
    // Events keep coming, so it never waits.
    const events = [ask, read];
    for (let i = 0; i < 66; i++) {
      events.push(i % 2 ? read : grep);
      rerender({ task: task(), events: [...events] });
      advance(10_000);
    }
    expect(result.current.activity).toBe("reading");

    rerender({ task: task(), events: [...events, build] });
    advance(ACTIVITY_TIMING.minHoldMs);
    expect(result.current.activity).toBe("long");
  });

  it("counts a follow-up turn from when it starts", () => {
    const old = task({ createdAt: NOW / 1000 - 3600 });
    const { result, rerender } = setup({ task: old, events: [ask, edit, ask, build] });
    advance(ACTIVITY_TIMING.minHoldMs);
    expect(result.current.activity).toBe("command");
    // A second turn of an hour-old task is not long yet; only silence would change it.
    rerender({ task: old, events: [ask, edit, ask, build, build] });
    advance(ACTIVITY_TIMING.minHoldMs);
    expect(result.current.activity).toBe("command");
  });

  it("starts long when the first turn began more than 10 min ago", () => {
    const { result } = setup({ task: task({ createdAt: NOW / 1000 - 11 * 60 }), events: [ask, build] });
    advance(0);
    advance(ACTIVITY_TIMING.minHoldMs);
    expect(result.current.activity).toBe("long");
  });
});

describe("useHeld", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("keeps each value at least the hold, then shows the latest", () => {
    const { result, rerender } = renderHook(({ value }) => useHeld(value, 2500), { initialProps: { value: "a" } });
    advance(1000);
    rerender({ value: "b" });
    expect(result.current).toBe("a");
    advance(1000);
    rerender({ value: "c" });
    advance(400);
    expect(result.current).toBe("a");
    advance(100);
    expect(result.current).toBe("c");

    // Held from its own start: an early change waits again.
    rerender({ value: "d" });
    advance(2000);
    expect(result.current).toBe("c");
    advance(500);
    expect(result.current).toBe("d");
  });

  it("changes at once after a calm spell, and drops a change that is undone", () => {
    const { result, rerender } = renderHook(({ value }) => useHeld(value, 2500), { initialProps: { value: "a" } });
    advance(5000);
    rerender({ value: "b" });
    advance(0);
    expect(result.current).toBe("b");
    advance(500);
    rerender({ value: "c" });
    advance(500);
    rerender({ value: "b" });
    advance(5000);
    expect(result.current).toBe("b");
  });
});
