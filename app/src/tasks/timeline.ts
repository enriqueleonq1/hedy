// Pure helpers that turn a task's events into what the chat shows.

import type { AgentEvent, Decision, StepStatus, Task } from "../api/types";
import type { MascotState } from "../components/Mascot/Mascot";

export type Resolution = { decision: Decision; automatic: boolean; reason: string | null };

export type TimelineItem =
  | { type: "user"; key: string; text: string }
  | { type: "agent"; key: string; text: string }
  | { type: "plan"; key: string; text: string; requestId: string | null; resolution: Resolution | null }
  | { type: "steps"; key: string; steps: { text: string; status: StepStatus }[] }
  | {
      type: "permission";
      key: string;
      requestId: string;
      tool: string;
      summary: string;
      resolution: Resolution | null;
    }
  | { type: "activity"; key: string; tone: "done" | "running" | "failed"; text: string }
  | { type: "finished"; key: string; ok: boolean; durationMs: number | null }
  | { type: "error"; key: string; message: string };

/** "claude-opus-5-5" → "Claude Opus". */
export function modelLabel(model: string | null | undefined): string {
  if (!model) return "Claude";
  const family = ["fable", "opus", "sonnet", "haiku"].find((f) => model.toLowerCase().includes(f));
  return family ? `Claude ${family.charAt(0).toUpperCase()}${family.slice(1)}` : "Claude";
}

export function agentName(events: AgentEvent[]): string {
  const started = events.find((e) => e.kind === "sessionStarted");
  return modelLabel(started?.kind === "sessionStarted" ? started.model : null);
}

const TOOL_VERB: Record<string, string> = {
  Read: "leyó",
  Grep: "buscó",
  Glob: "buscó archivos",
  WebSearch: "buscó en la web",
  WebFetch: "leyó la página",
  Task: "delegó una subtarea",
};

export function buildTimeline(events: AgentEvent[]): TimelineItem[] {
  const resolutions = new Map<string, Resolution>();
  let lastSteps = -1;
  events.forEach((e, i) => {
    if (e.kind === "permissionResolved") {
      resolutions.set(e.requestId, { decision: e.decision, automatic: e.automatic, reason: e.reason });
    }
    if (e.kind === "steps") lastSteps = i;
  });
  const name = agentName(events).split(" ")[0] ?? "Claude";
  const items: TimelineItem[] = [];
  let stepsShown = false;

  events.forEach((e, i) => {
    const key = `${i}`;
    switch (e.kind) {
      case "userMessage":
        items.push({ type: "user", key, text: e.text });
        break;
      case "message":
        items.push({ type: "agent", key, text: e.text });
        break;
      case "plan":
        items.push({
          type: "plan",
          key,
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
          requestId: e.requestId,
          tool: e.tool,
          summary: e.summary,
          resolution: resolutions.get(e.requestId) ?? null,
        });
        break;
      case "fileEdited":
        items.push({ type: "activity", key, tone: "done", text: `${name} editó ${e.path}` });
        break;
      case "commandRun":
        items.push({ type: "activity", key, tone: "done", text: `${name} corrió ${e.command}` });
        break;
      case "testsRun":
        items.push({
          type: "activity",
          key,
          tone: e.passed ? "done" : "failed",
          text: `${e.passed ? "Pasan los tests" : "Fallan los tests"} · ${e.command}`,
        });
        break;
      case "toolUsed":
        items.push({
          type: "activity",
          key,
          tone: "running",
          text: `${name} ${TOOL_VERB[e.tool] ?? `usó ${e.tool}`} · ${e.summary}`,
        });
        break;
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

const WORK: AgentEvent["kind"][] = ["fileEdited", "commandRun", "testsRun", "toolUsed", "permissionResolved"];

/** The mascot reacts to what the agent is doing (spec §6). */
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
    case "running": {
      if (!task.live) return "sleeping";
      const last = [...events].reverse().find((e) => e.kind !== "usage" && e.kind !== "rateLimit");
      return last && WORK.includes(last.kind) ? "working" : "thinking";
    }
  }
}

export function formatTokens(tokens: number): string {
  if (tokens >= 1_000_000) return `${(tokens / 1_000_000).toFixed(1)}M`;
  if (tokens >= 1000) return `${Math.round(tokens / 1000)}k`;
  return `${tokens}`;
}

export function formatDuration(ms: number | null): string {
  if (ms === null) return "";
  const s = Math.round(ms / 1000);
  return s < 60 ? `${s} s` : `${Math.floor(s / 60)} min ${s % 60} s`;
}

export const STATUS_LABEL: Record<Task["status"], string> = {
  running: "Trabajando",
  waitingApproval: "Espera tu respuesta",
  review: "Lista para revisar",
  accepted: "Aceptada",
  discarded: "Descartada",
  failed: "Con error",
};

/** Not accepted nor discarded: it can still take messages and changes. */
export function isOpen(task: Task | null | undefined): boolean {
  return !!task && task.status !== "accepted" && task.status !== "discarded";
}
