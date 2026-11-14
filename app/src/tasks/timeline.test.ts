import type { AgentEvent, Task } from "../api/types";
import { buildTimeline, formatTokens, mascotState, modelLabel } from "./timeline";

const task = (extra: Partial<Task>): Task => ({
  id: "t1",
  project: "/code/moira",
  mode: "project",
  title: "Recordatorios",
  prompt: "Agrega recordatorios",
  account: "claude-personal",
  model: null,
  workspace: null,
  sessionId: null,
  status: "running",
  createdAt: 0,
  live: true,
  ...extra,
});

describe("buildTimeline", () => {
  const events: AgentEvent[] = [
    { kind: "userMessage", text: "Agrega recordatorios" },
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
    const items = buildTimeline(events);
    expect(items.map((i) => i.type)).toEqual([
      "user",
      "plan",
      "steps",
      "activity",
      "permission",
      "activity",
      "finished",
    ]);
    const plan = items[1];
    expect(plan?.type === "plan" && plan.resolution?.decision).toBe("allow");
    const steps = items[2];
    expect(steps?.type === "steps" && steps.steps[0]?.status).toBe("done");
    const edit = items[3];
    expect(edit?.type === "activity" && edit.text).toBe("Claude editó citas/servicio.py");
    const permission = items[4];
    expect(permission?.type === "permission" && permission.resolution).toBeNull();
  });

  it("shows hook blocks that had no card", () => {
    const items = buildTimeline([
      { kind: "permissionResolved", requestId: "hook-1", decision: "deny", automatic: true, reason: "Hedy no permite «.env»" },
    ]);
    expect(items).toEqual([{ type: "activity", key: "0", tone: "failed", text: "Bloqueado por Hedy · Hedy no permite «.env»" }]);
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
    expect(mascotState(task({}), [{ kind: "message", text: "pienso" }])).toBe("thinking");
    expect(mascotState(task({ live: false }), [])).toBe("sleeping");
    expect(
      mascotState(task({ status: "review" }), [{ kind: "finished", ok: true, summary: "", durationMs: 1 }]),
    ).toBe("done");
    expect(mascotState(task({ status: "accepted" }), [])).toBe("done");
    expect(mascotState(task({ status: "failed" }), [])).toBe("sleeping");
  });
});

it("formats token counts", () => {
  expect(formatTokens(950)).toBe("950");
  expect(formatTokens(182_000)).toBe("182k");
  expect(formatTokens(2_400_000)).toBe("2.4M");
});
