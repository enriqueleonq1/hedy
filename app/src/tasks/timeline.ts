// Pure helpers that turn a task's events into what the chat shows.

import type { AgentEvent, Attachment, Decision, Phase, StepStatus, Task, TaskMode } from "../api/types";
import type { MascotState } from "../components/Mascot/Mascot";
import type { MessageKey, Translate } from "../i18n";
import { stoppedByLimit } from "./state";

/** A tool step of the agent: a tool, an edit, a command, a test run or a block by Hedy. */
export type StepKind = "tool" | "edit" | "command" | "tests" | "blocked";

export type Resolution = { decision: Decision; automatic: boolean; reason: string | null };

export type TimelineItem =
  | { type: "user"; key: string; text: string; images: Attachment[] }
  | { type: "agent"; key: string; text: string }
  | {
      type: "plan";
      key: string;
      agent: string;
      text: string;
      requestId: string | null;
      resolution: Resolution | null;
    }
  | { type: "phase"; key: string; phase: Phase; agent: string; account: string }
  | {
      type: "limit";
      key: string;
      account: string;
      level: "warning" | "reached";
      percent: number | null;
      resetsAt: number | null;
      latest: boolean;
    }
  | { type: "steps"; key: string; steps: { text: string; status: StepStatus }[] }
  | {
      type: "permission";
      key: string;
      agent: string;
      requestId: string;
      tool: string;
      summary: string;
      resolution: Resolution | null;
    }
  | {
      type: "activity";
      key: string;
      tone: "done" | "running" | "failed";
      /** What the step was, to sum a run of steps up. */
      step: StepKind;
      /** The file, command or tool summary it was about. */
      detail: string;
      text: string;
    }
  | { type: "finished"; key: string; ok: boolean; durationMs: number | null }
  | { type: "flowCheck"; key: string; node: string; what: string; passed: boolean }
  | { type: "flowOutOfRounds"; key: string; rounds: number; asked: string }
  /** From here on the task changes code, in its own branch. */
  | { type: "changesCode"; key: string; branch: string }
  /**
   * A flow's approval step. `answer`: the user's, once given; `latest`: the
   * last time the flow asked (only that one can still be waiting).
   */
  | { type: "flowApproval"; key: string; node: string; answer: "approved" | "changes" | null; latest: boolean }
  | { type: "error"; key: string; message: string };

/** "claude-opus-5-5" → "Claude Opus". */
export function modelLabel(model: string | null | undefined): string {
  if (!model) return "Claude";
  const family = ["fable", "opus", "sonnet", "haiku"].find((f) => model.toLowerCase().includes(f));
  return family ? `Claude ${family.charAt(0).toUpperCase()}${family.slice(1)}` : "Claude";
}

/** Who is working now: the latest phase's agent, or the model of the session. */
export function agentName(events: AgentEvent[]): string {
  const phase = [...events].reverse().find((e) => e.kind === "phaseStarted");
  if (phase?.kind === "phaseStarted") return phase.agent;
  const started = events.find((e) => e.kind === "sessionStarted");
  return modelLabel(started?.kind === "sessionStarted" ? started.model : null);
}

export const PHASE_VERB: Record<Phase, MessageKey> = {
  plan: "workspace.phase.plan",
  implement: "workspace.phase.implement",
  tests: "workspace.phase.tests",
  review: "workspace.phase.review",
};

const TOOL_VERB: Record<string, MessageKey> = {
  Read: "workspace.tool.Read",
  Grep: "workspace.tool.Grep",
  Glob: "workspace.tool.Glob",
  WebSearch: "workspace.tool.WebSearch",
  WebFetch: "workspace.tool.WebFetch",
  Task: "workspace.tool.Task",
};

export function buildTimeline(events: AgentEvent[], t: Translate): TimelineItem[] {
  const resolutions = new Map<string, Resolution>();
  const requested = new Set<string>();
  let lastSteps = -1;
  events.forEach((e, i) => {
    if (e.kind === "permissionResolved") {
      resolutions.set(e.requestId, { decision: e.decision, automatic: e.automatic, reason: e.reason });
    }
    if (e.kind === "steps") lastSteps = i;
    if (e.kind === "permissionRequest" || (e.kind === "plan" && e.requestId)) {
      requested.add(e.kind === "plan" ? (e.requestId ?? "") : e.requestId);
    }
  });
  const items: TimelineItem[] = [];
  let stepsShown = false;
  // Activity lines name whoever was working at that moment.
  const firstPhase = events.find((e) => e.kind === "phaseStarted");
  let agent = firstPhase?.kind === "phaseStarted" ? firstPhase.agent : agentName(events);
  const name = () => agent.split(" ")[0] ?? "Claude";
  const lastLimit = events.reduce((last, e, i) => (e.kind === "limitWarning" ? i : last), -1);

  events.forEach((e, i) => {
    const key = `${i}`;
    switch (e.kind) {
      case "userMessage":
        items.push({ type: "user", key, text: e.text, images: e.images ?? [] });
        break;
      case "message":
        items.push({ type: "agent", key, text: e.text });
        break;
      case "flowCheck":
        items.push({ type: "flowCheck", key, node: e.node, what: e.what, passed: e.passed });
        break;
      case "flowOutOfRounds":
        items.push({ type: "flowOutOfRounds", key, rounds: e.rounds, asked: e.asked });
        break;
      case "changesCode":
        items.push({ type: "changesCode", key, branch: e.branch });
        break;
      case "flowApproval": {
        const after = events.slice(i + 1);
        const again = after.findIndex((x) => x.kind === "flowApproval");
        const reply = (again < 0 ? after : after.slice(0, again)).find(
          (x): x is Extract<AgentEvent, { kind: "flowApprovalAnswered" }> => x.kind === "flowApprovalAnswered",
        );
        items.push({
          type: "flowApproval",
          key,
          node: e.node,
          answer: reply ? (reply.approved ? "approved" : "changes") : null,
          latest: again < 0,
        });
        break;
      }
      case "phaseStarted":
        agent = e.agent;
        items.push({ type: "phase", key, phase: e.phase, agent: e.agent, account: e.account });
        break;
      case "limitWarning":
        items.push({
          type: "limit",
          key,
          account: e.account,
          level: e.level,
          percent: e.percent,
          resetsAt: e.resetsAt,
          latest: i === lastLimit,
        });
        break;
      case "plan":
        items.push({
          type: "plan",
          key,
          agent,
          text: e.text,
          requestId: e.requestId,
          resolution: e.requestId ? (resolutions.get(e.requestId) ?? null) : null,
        });
        break;
      case "steps": {
        // One checklist, at the place it first appeared, with the latest state.
        const latest = events[lastSteps];
        if (!stepsShown && latest?.kind === "steps") {
          items.push({ type: "steps", key, steps: latest.steps });
          stepsShown = true;
        }
        break;
      }
      case "permissionRequest":
        items.push({
          type: "permission",
          key,
          agent,
          requestId: e.requestId,
          tool: e.tool,
          summary: e.summary,
          resolution: resolutions.get(e.requestId) ?? null,
        });
        break;
      case "permissionResolved":
        // Blocks without a card (Hedy's hook) still show up in the activity.
        if (!requested.has(e.requestId) && e.decision === "deny") {
          const text = t("workspace.activity.blocked", { reason: e.reason ?? "" });
          items.push({ type: "activity", key, tone: "failed", step: "blocked", detail: e.reason ?? "", text });
        }
        break;
      case "fileEdited":
        items.push({
          type: "activity",
          key,
          tone: "done",
          step: "edit",
          detail: e.path,
          text: t("workspace.activity.edited", { agent: name(), path: e.path }),
        });
        break;
      case "commandRun":
        items.push({
          type: "activity",
          key,
          tone: "done",
          step: "command",
          detail: e.command,
          text: t("workspace.activity.ran", { agent: name(), command: e.command }),
        });
        break;
      case "testsRun":
        items.push({
          type: "activity",
          key,
          tone: e.passed ? "done" : "failed",
          step: "tests",
          detail: e.command,
          text: t(e.passed ? "workspace.activity.testsPassed" : "workspace.activity.testsFailed", {
            agent: name(),
            command: e.command,
          }),
        });
        break;
      case "checkRan":
        items.push({
          type: "activity",
          key,
          tone: e.passed ? "done" : "failed",
          step: "tests",
          detail: e.command,
          text: t(
            e.check === "e2e"
              ? e.passed
                ? "workspace.activity.e2ePassed"
                : "workspace.activity.e2eFailed"
              : e.passed
                ? "workspace.activity.checkPassed"
                : "workspace.activity.checkFailed",
            { command: e.command },
          ),
        });
        break;
      case "checksSkipped":
        items.push({ type: "activity", key, tone: "failed", step: "tests", detail: "", text: e.reason });
        break;
      case "toolUsed": {
        const verb = TOOL_VERB[e.tool];
        items.push({
          type: "activity",
          key,
          tone: "running",
          step: "tool",
          detail: e.summary,
          text: t("workspace.activity.tool", {
            agent: name(),
            verb: verb ? t(verb) : t("workspace.tool.other", { tool: e.tool }),
            summary: e.summary,
          }),
        });
        break;
      }
      case "finished":
        items.push({ type: "finished", key, ok: e.ok, durationMs: e.durationMs });
        break;
      case "error":
        items.push({ type: "error", key, message: e.message });
        break;
      default:
        break;
    }
  });
  return items;
}

/**
 * The mascot reacts to what the agent is doing (spec §6). While the agent
 * runs it is `working`; what it does then (reading, testing, thinking...)
 * is the activity, from `useMascot`.
 */
export function mascotState(task: Task | null | undefined, events: AgentEvent[]): MascotState {
  if (!task) return "idle";
  switch (task.status) {
    case "accepted":
      return "done";
    case "discarded":
    case "failed":
      return "sleeping";
    case "waitingApproval":
      return "thinking";
    case "review": {
      const finished = [...events].reverse().find((e) => e.kind === "finished");
      return finished?.kind === "finished" && finished.ok ? "done" : "idle";
    }
    case "running":
      // Paused by a limit: asleep, like the pill and the board say.
      if (stoppedByLimit(events)) return "sleeping";
      return task.live || runningElsewhere(task) ? "working" : "sleeping";
  }
}

export const MODE_LABEL: Record<TaskMode, MessageKey> = {
  project: "workspace.mode.project",
  ask: "workspace.mode.ask",
  research: "workspace.mode.research",
};

export const STATUS_LABEL: Record<Task["status"], MessageKey> = {
  running: "workspace.status.running",
  waitingApproval: "workspace.status.waitingApproval",
  review: "workspace.status.review",
  accepted: "workspace.status.accepted",
  discarded: "workspace.status.discarded",
  failed: "workspace.status.failed",
};

/**
 * A conversation that is answering in another Hedy process (`hedy ask` in a
 * terminal): running by its stored status, with no agent in this app.
 */
export function runningElsewhere(task: Task | null | undefined): task is Task {
  return !!task && !!task.conversation && !task.live && (task.status === "running" || task.status === "waitingApproval");
}

/** Not accepted nor discarded: it can still take messages and changes. */
export function isOpen(task: Task | null | undefined): boolean {
  return !!task && task.status !== "accepted" && task.status !== "discarded";
}

/** A plan or permission card that still waits for the user's answer. */
export type PendingCard = "plan" | "permission";

/** The latest card still waiting for an answer, if any. */
export function pendingCard(events: AgentEvent[]): PendingCard | null {
  const open = new Map<string, PendingCard>();
  for (const e of events) {
    if (e.kind === "plan" && e.requestId) open.set(e.requestId, "plan");
    if (e.kind === "permissionRequest") open.set(e.requestId, "permission");
    if (e.kind === "permissionResolved") open.delete(e.requestId);
  }
  return [...open.values()].at(-1) ?? null;
}

/** What a screen reader hears about a task: only when it needs the user or ends. */
export type Milestone = "plan" | "permission" | "approval" | "finished" | "failed" | "limit";

/**
 * The latest milestone of a timeline and its position, to announce each one
 * once. Cards count while they wait; limits only once reached.
 */
export function latestMilestone(items: TimelineItem[]): { index: number; milestone: Milestone } | null {
  for (let index = items.length - 1; index >= 0; index--) {
    const item = items[index];
    if (!item) continue;
    if (item.type === "plan" && item.requestId !== null && item.resolution === null) return { index, milestone: "plan" };
    if (item.type === "permission" && item.resolution === null) return { index, milestone: "permission" };
    if (item.type === "flowApproval" && item.answer === null && item.latest) return { index, milestone: "approval" };
    if (item.type === "finished") return { index, milestone: item.ok ? "finished" : "failed" };
    if (item.type === "limit" && item.level === "reached") return { index, milestone: "limit" };
  }
  return null;
}
