// One state per task, the same everywhere it shows (the task's header, Inicio,
// the project's list, the task switcher): the board's when it lists the task,
// else derived from the task and, when they are at hand, its events.

import type { ActivityItem, AgentEvent, BoardState, Task } from "../api/types";

/** Whether the latest turn ended on a reached limit (nothing restarted it since). */
export function stoppedByLimit(events: AgentEvent[]): boolean {
  let limited = false;
  for (const e of events) {
    if (e.kind === "limitWarning" && e.level === "reached") limited = true;
    if (e.kind === "userMessage" || e.kind === "phaseStarted") limited = false;
    // Like the backend's `LimitLevel::from_report`: rejected, or used up (Codex's 100 %).
    if (e.kind === "rateLimit" && e.status !== "rejected" && !((e.utilization ?? 0) >= 100)) limited = false;
  }
  return limited;
}

/** Which card a task waiting for the user shows: an open plan, else a permission. */
function waitingFor(events: AgentEvent[] | undefined): BoardState {
  if (!events) return "plan";
  const open = new Set<string>();
  for (const e of events) {
    if (e.kind === "plan" && e.requestId) open.add(e.requestId);
    if (e.kind === "permissionResolved") open.delete(e.requestId);
  }
  return open.size > 0 ? "plan" : "permission";
}

/** Like the backend's `task_state`, when the board does not list the task. */
export function taskState(task: Task, item?: ActivityItem, events?: AgentEvent[]): BoardState {
  if (item) return item.state;
  switch (task.status) {
    case "running":
      return events && stoppedByLimit(events) ? "limited" : "working";
    case "waitingApproval":
      return waitingFor(events);
    case "review":
      return task.mode === "project" ? "review" : "answered";
    case "accepted":
      return "accepted";
    case "discarded":
      return "cancelled";
    case "failed":
      return events && stoppedByLimit(events) ? "limited" : "failed";
  }
}
