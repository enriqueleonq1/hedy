import type { AgentEvent, Task, WorkingStyle } from "../api/types";
import { stopsOf } from "./stops";

const AFTER_OK: WorkingStyle = { plan: true, tests: true, e2e: true, review: false, e2eCommand: "npm run e2e" };

const task = (extra: Partial<Task>): Task => ({
  id: "t1",
  project: "/code/agenda",
  mode: "project",
  title: "Vencer reservas",
  prompt: "Las reservas deben vencer",
  account: "claude",
  model: null,
  workspace: null,
  sessionId: null,
  status: "running",
  createdAt: 0,
  stages: [],
  stage: 0,
  live: true,
  working: AFTER_OK,
  ...extra,
});

const states = (t: Task, events: AgentEvent[]) => stopsOf(t, events)?.map((s) => `${s.key}:${s.state}`).join(" ");

const planned: AgentEvent[] = [
  { kind: "userMessage", text: "Las reservas deben vencer" },
  { kind: "plan", text: "## Lo que supongo\n- 15 min", requestId: "p1" },
];
const approved: AgentEvent[] = [
  ...planned,
  { kind: "permissionResolved", requestId: "p1", decision: "allow", automatic: false, reason: null },
  { kind: "fileEdited", path: "shop/inventory.py" },
];
const finished: AgentEvent = { kind: "finished", ok: true, summary: "Listo", durationMs: 1000 };

describe("stopsOf", () => {
  it("follows a task from its plan to the user's review", () => {
    expect(states(task({}), [{ kind: "userMessage", text: "x" }])).toBe(
      "plan:current ok:pending implement:pending tests:pending e2e:pending you:pending",
    );
    expect(states(task({ status: "waitingApproval" }), planned)).toBe(
      "plan:done ok:current implement:pending tests:pending e2e:pending you:pending",
    );
    expect(states(task({}), approved)).toBe("plan:done ok:done implement:current tests:pending e2e:pending you:pending");
    // The turn is over: Hedy runs the tests, then the e2e.
    expect(states(task({}), [...approved, finished])).toBe(
      "plan:done ok:done implement:done tests:current e2e:pending you:pending",
    );
    const checked: AgentEvent[] = [
      ...approved,
      finished,
      { kind: "checkRan", check: "tests", command: "pytest -q", passed: true },
      { kind: "checkRan", check: "e2e", command: "npm run e2e", passed: false },
    ];
    expect(states(task({ status: "review" }), checked)).toBe(
      "plan:done ok:done implement:done tests:done e2e:failed you:current",
    );
    expect(states(task({ status: "accepted", live: false }), checked)).toBe(
      "plan:done ok:done implement:done tests:done e2e:failed you:done",
    );
  });

  it("goes back to implementing when a failure was sent back", () => {
    const events: AgentEvent[] = [
      ...approved,
      finished,
      { kind: "checkRan", check: "tests", command: "pytest -q", passed: false },
      { kind: "fileEdited", path: "shop/inventory.py" },
    ];
    expect(states(task({}), events)).toBe("plan:done ok:done implement:current tests:pending e2e:pending you:pending");
  });

  it("leaves out what does not apply", () => {
    // No e2e command: no e2e stop; no plan: it starts implementing.
    const quick = task({ working: { ...AFTER_OK, plan: false, e2eCommand: null } });
    expect(states(quick, [{ kind: "userMessage", text: "x" }])).toBe("implement:current tests:pending you:pending");
    // Right away with nothing to check: no bar.
    expect(stopsOf(task({ working: { plan: false, tests: false, e2e: false, review: false } }), [])).toBeNull();
    expect(stopsOf(task({ working: null }), [])).toBeNull();
    expect(stopsOf(task({ mode: "ask" }), [])).toBeNull();
  });
});
