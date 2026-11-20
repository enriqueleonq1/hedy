import type { ActivityItem, AgentEvent, Task } from "../api/types";
import { stoppedByLimit, taskState } from "./state";
import { mascotState } from "./timeline";

const task = (extra: Partial<Task> = {}): Task => ({
  id: "t1",
  project: "/code/agenda",
  mode: "project",
  title: "Recordatorios",
  prompt: "Agrega recordatorios",
  account: "claude-personal",
  model: null,
  workspace: null,
  sessionId: null,
  status: "running",
  createdAt: 0,
  stages: [],
  stage: 0,
  live: true,
  ...extra,
});

const reached: AgentEvent = { kind: "limitWarning", account: "claude-personal", level: "reached", percent: 100, resetsAt: null };
const rateLimit = (status: string, utilization: number | null): AgentEvent => ({ kind: "rateLimit", status, utilization, resetsAt: null });

describe("stoppedByLimit", () => {
  it("is the latest turn ending on a reached limit", () => {
    expect(stoppedByLimit([])).toBe(false);
    expect(stoppedByLimit([reached])).toBe(true);
    // A warning is not a stop.
    expect(stoppedByLimit([{ ...reached, level: "warning", percent: 90 }])).toBe(false);
  });

  it("is cleared by whatever starts the agent again", () => {
    expect(stoppedByLimit([reached, { kind: "userMessage", text: "sigue" }])).toBe(false);
    expect(stoppedByLimit([reached, { kind: "phaseStarted", phase: "implement", agent: "Claude Opus", account: "Claude personal" }])).toBe(false);
    expect(stoppedByLimit([reached, rateLimit("allowed", 40)])).toBe(false);
    expect(stoppedByLimit([reached, rateLimit("allowed_warning", null)])).toBe(false);
    // And it can be reached again afterwards.
    expect(stoppedByLimit([reached, { kind: "userMessage", text: "sigue" }, reached])).toBe(true);
  });

  it("is not cleared by a report that the limit is still used up", () => {
    expect(stoppedByLimit([reached, rateLimit("rejected", null)])).toBe(true);
    // Codex says "allowed" at 100 %: still out of it.
    expect(stoppedByLimit([reached, rateLimit("allowed", 100)])).toBe(true);
    expect(stoppedByLimit([reached, rateLimit("allowed", 100), rateLimit("allowed", 3)])).toBe(false);
  });
});

describe("taskState", () => {
  it("takes the board's state when the board lists the task", () => {
    const item = { state: "permission" } as ActivityItem;
    expect(taskState(task({ status: "review" }), item, [])).toBe("permission");
  });

  it("tells a plan from a permission by the cards still open", () => {
    const waiting = task({ status: "waitingApproval" });
    // Without the events, a plan (as the board's default).
    expect(taskState(waiting)).toBe("plan");
    const plan: AgentEvent = { kind: "plan", text: "1. Buscar", requestId: "p1" };
    expect(taskState(waiting, undefined, [plan])).toBe("plan");
    const approved: AgentEvent = { kind: "permissionResolved", requestId: "p1", decision: "allow", automatic: false, reason: null };
    const permission: AgentEvent = { kind: "permissionRequest", requestId: "r1", tool: "Bash", summary: "npm test", input: {} };
    expect(taskState(waiting, undefined, [plan, approved, permission])).toBe("permission");
    // A plan without an id never waits for an answer.
    expect(taskState(waiting, undefined, [{ ...plan, requestId: null }, permission])).toBe("permission");
  });

  it("says a running or failed task is paused while its limit holds", () => {
    expect(taskState(task())).toBe("working");
    expect(taskState(task(), undefined, [reached])).toBe("limited");
    expect(taskState(task(), undefined, [reached, { kind: "userMessage", text: "sigue" }])).toBe("working");
    expect(taskState(task(), undefined, [reached, rateLimit("allowed", 100)])).toBe("limited");
    expect(taskState(task({ status: "failed" }), undefined, [reached])).toBe("limited");
    expect(taskState(task({ status: "failed" }), undefined, [{ kind: "error", message: "npm install falló" }])).toBe("failed");
    expect(taskState(task({ status: "failed" }))).toBe("failed");
  });

  it("names the rest from the task's status", () => {
    expect(taskState(task({ status: "review" }))).toBe("review");
    expect(taskState(task({ status: "review", mode: "ask" }))).toBe("answered");
    expect(taskState(task({ status: "review", mode: "research" }))).toBe("answered");
    expect(taskState(task({ status: "accepted" }))).toBe("accepted");
    expect(taskState(task({ status: "discarded" }))).toBe("cancelled");
  });
});

describe("the mascot of a task paused by a limit", () => {
  it("sleeps until the agent starts again", () => {
    expect(mascotState(task(), [])).toBe("working");
    expect(mascotState(task(), [reached])).toBe("sleeping");
    expect(mascotState(task(), [reached, { kind: "userMessage", text: "sigue" }])).toBe("working");
  });
});
