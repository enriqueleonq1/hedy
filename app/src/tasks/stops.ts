// The stops of a task that works with one AI (docs/decisions.md §60), for the bar
// over its conversation: derived from its way of working, its status and its
// history, with no state of its own.

import type { AgentEvent, Task } from "../api/types";

export type StopKey = "plan" | "ok" | "implement" | "tests" | "e2e" | "review" | "you";
export type StopState = "done" | "current" | "pending" | "failed";
export interface Stop {
  key: StopKey;
  state: StopState;
}

/** `null`: a flow, a question, an older task, or one with no stop but implementing. */
export function stopsOf(task: Task, events: AgentEvent[]): Stop[] | null {
  const working = task.working;
  if (!working || task.mode !== "project" || task.flow) return null;
  const checks: StopKey[] = [
    ...(working.tests ? (["tests"] as const) : []),
    ...(working.e2e && working.e2eCommand ? (["e2e"] as const) : []),
    ...(working.review ? (["review"] as const) : []),
  ];
  const keys: StopKey[] = [...(working.plan ? (["plan", "ok"] as const) : []), "implement", ...checks, "you"];
  // «Implementa · Revisas» says nothing the task does not say already.
  if (keys.length < 3) return null;

  const plans = new Set(events.flatMap((e) => (e.kind === "plan" && e.requestId ? [e.requestId] : [])));
  const planned = plans.size > 0;
  const approved = events.some((e) => e.kind === "permissionResolved" && e.decision === "allow" && plans.has(e.requestId));
  const lastFinished = events.findLastIndex((e) => e.kind === "finished");
  const lastWork = events.findLastIndex((e) => e.kind !== "finished" && !AFTER_TURN.has(e.kind));
  const turnOver = lastFinished > lastWork;
  // The last turn's results, while no new work makes them stale.
  const results = new Map<StopKey, boolean>();
  for (const e of turnOver ? events.slice(lastFinished + 1) : []) {
    if (e.kind === "checkRan") results.set(e.check, e.passed);
    if (e.kind === "reviewRan") results.set("review", e.warnings.length === 0);
  }
  const ready = task.status === "review" || task.status === "accepted" || task.status === "discarded" || task.status === "failed";
  const pastPlan = !working.plan || approved;

  // The first check without a result is the one running, while the turn is over.
  const running = !ready && turnOver && pastPlan ? checks.find((k) => !results.has(k)) : undefined;
  const state = (key: StopKey): StopState => {
    switch (key) {
      case "plan":
        return planned || approved ? "done" : "current";
      case "ok":
        return approved ? "done" : planned ? "current" : "pending";
      case "implement":
        if (!pastPlan) return "pending";
        return ready || turnOver ? "done" : "current";
      case "you":
        return task.status === "accepted" ? "done" : task.status === "review" ? "current" : "pending";
      default: {
        const result = results.get(key);
        if (result !== undefined) return result ? "done" : "failed";
        return key === running ? "current" : "pending";
      }
    }
  };
  return keys.map((key) => ({ key, state: state(key) }));
}

/** What comes after a turn's end without being more work. */
const AFTER_TURN = new Set<AgentEvent["kind"]>([
  "usage",
  "rateLimit",
  "limitWarning",
  "checkRan",
  "checksSkipped",
  "reviewRan",
]);
