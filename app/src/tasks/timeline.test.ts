import type { AgentEvent, Task } from "../api/types";
import { translator } from "../i18n";
import { agentName, buildTimeline, latestMilestone, mascotState, modelLabel, pendingCard } from "./timeline";

const t = translator("es");

const task = (extra: Partial<Task>): Task => ({
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

describe("buildTimeline", () => {
  const events: AgentEvent[] = [
    { kind: "userMessage", text: "Agrega recordatorios" },
    { kind: "phaseStarted", phase: "plan", agent: "Claude Opus", account: "Claude personal" },
    { kind: "sessionStarted", sessionId: "s", model: "claude-opus-5-5" },
    { kind: "plan", text: "1. Buscar", requestId: "p1" },
    { kind: "permissionResolved", requestId: "p1", decision: "allow", automatic: false, reason: null },
    { kind: "steps", steps: [{ text: "Buscar", status: "inProgress" }] },
    { kind: "fileEdited", path: "citas/servicio.py" },
    { kind: "permissionRequest", requestId: "r1", tool: "Bash", summary: "pytest", input: {} },
    { kind: "steps", steps: [{ text: "Buscar", status: "done" }] },
    { kind: "testsRun", command: "pytest", passed: true },
    { kind: "usage", inputTokens: 1, outputTokens: 1, cacheReadTokens: 0, cacheWriteTokens: 0, costUsd: null },
    { kind: "finished", ok: true, summary: "", durationMs: 4000 },
  ];

  it("links resolutions, keeps one checklist and names the model", () => {
    const items = buildTimeline(events, t);
    expect(items.map((i) => i.type)).toEqual([
      "user",
      "phase",
      "plan",
      "steps",
      "activity",
      "permission",
      "activity",
      "finished",
    ]);
    const plan = items[2];
    expect(plan?.type === "plan" && plan.resolution?.decision).toBe("allow");
    const steps = items[3];
    expect(steps?.type === "steps" && steps.steps[0]?.status).toBe("done");
    const edit = items[4];
    expect(edit?.type === "activity" && edit.text).toBe("Claude editó citas/servicio.py");
    expect(edit?.type === "activity" && [edit.step, edit.detail]).toEqual(["edit", "citas/servicio.py"]);
    const permission = items[5];
    expect(permission?.type === "permission" && permission.resolution).toBeNull();
  });

  it("names whoever did each step and keeps only the last limit actionable", () => {
    const items = buildTimeline([
      { kind: "phaseStarted", phase: "implement", agent: "Codex", account: "ChatGPT" },
      { kind: "fileEdited", path: "a.py" },
      { kind: "limitWarning", account: "chatgpt", level: "warning", percent: 91, resetsAt: null },
      { kind: "phaseStarted", phase: "tests", agent: "Kimi", account: "Kimi" },
      { kind: "testsRun", command: "pytest", passed: true },
      { kind: "limitWarning", account: "kimi", level: "reached", percent: null, resetsAt: null },
    ], t);
    const texts = items.filter((i) => i.type === "activity").map((i) => i.type === "activity" && i.text);
    expect(texts).toEqual(["Codex editó a.py", "Kimi: pasan los tests · pytest"]);
    const limits = items.filter((i) => i.type === "limit").map((i) => i.type === "limit" && i.latest);
    expect(limits).toEqual([false, true]);
    expect(agentName([{ kind: "phaseStarted", phase: "tests", agent: "Kimi", account: "Kimi" }])).toBe("Kimi");
  });

  it("shows hook blocks that had no card", () => {
    const items = buildTimeline([
      { kind: "permissionResolved", requestId: "hook-1", decision: "deny", automatic: true, reason: "Hedy no permite «.env»" },
    ], t);
    expect(items).toEqual([
      {
        type: "activity",
        key: "0",
        tone: "failed",
        step: "blocked",
        detail: "Hedy no permite «.env»",
        text: "Bloqueado por Hedy · Hedy no permite «.env»",
      },
    ]);
  });

  it("labels models", () => {
    expect(modelLabel("claude-sonnet-5-5")).toBe("Claude Sonnet");
    expect(modelLabel(null)).toBe("Claude");
  });
});

describe("mascotState", () => {
  it("follows the agent", () => {
    expect(mascotState(null, [])).toBe("idle");
    expect(mascotState(task({ status: "waitingApproval" }), [])).toBe("thinking");
    expect(mascotState(task({}), [{ kind: "fileEdited", path: "a" }])).toBe("working");
    // Thinking before any tool is an activity of the working mascot.
    expect(mascotState(task({}), [{ kind: "message", text: "pienso" }])).toBe("working");
    expect(mascotState(task({ live: false }), [])).toBe("sleeping");
    expect(
      mascotState(task({ status: "review" }), [{ kind: "finished", ok: true, summary: "", durationMs: 1 }]),
    ).toBe("done");
    expect(mascotState(task({ status: "accepted" }), [])).toBe("done");
    expect(mascotState(task({ status: "failed" }), [])).toBe("sleeping");
  });
});

describe("pendingCard", () => {
  it("is the latest card without an answer", () => {
    const plan: AgentEvent = { kind: "plan", text: "1. Hacer", requestId: "p1" };
    const ask: AgentEvent = { kind: "permissionRequest", requestId: "r1", tool: "Bash", summary: "ls", input: {} };
    const answered = (requestId: string): AgentEvent => ({
      kind: "permissionResolved",
      requestId,
      decision: "allow",
      automatic: false,
      reason: null,
    });
    expect(pendingCard([])).toBeNull();
    expect(pendingCard([plan])).toBe("plan");
    expect(pendingCard([plan, answered("p1"), ask])).toBe("permission");
    expect(pendingCard([plan, answered("p1"), ask, answered("r1")])).toBeNull();
    // A plan without a request (research) waits for nobody.
    expect(pendingCard([{ kind: "plan", text: "Notas", requestId: null }])).toBeNull();
  });
});

describe("latestMilestone", () => {
  it("names the latest card to answer, end or reached limit", () => {
    const events: AgentEvent[] = [
      { kind: "userMessage", text: "Hola" },
      { kind: "plan", text: "1. Hacer", requestId: "p1" },
    ];
    expect(latestMilestone(buildTimeline(events, t))).toEqual({ index: 1, milestone: "plan" });
    events.push(
      { kind: "permissionResolved", requestId: "p1", decision: "allow", automatic: false, reason: null },
      { kind: "limitWarning", account: "a", level: "warning", percent: 80, resetsAt: null },
    );
    expect(latestMilestone(buildTimeline(events, t))).toBeNull();
    events.push({ kind: "finished", ok: false, summary: "", durationMs: null });
    expect(latestMilestone(buildTimeline(events, t))).toEqual({ index: 3, milestone: "failed" });
    events.push({ kind: "limitWarning", account: "a", level: "reached", percent: 100, resetsAt: null });
    expect(latestMilestone(buildTimeline(events, t))).toEqual({ index: 4, milestone: "limit" });
  });
});
