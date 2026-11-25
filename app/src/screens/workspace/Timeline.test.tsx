import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { ApiProvider } from "../../api/context";
import { MockApi } from "../../api/MockApi";

import type { AgentEvent, Task } from "../../api/types";
import { translator } from "../../i18n";
import { buildTimeline } from "../../tasks/timeline";
import type { TaskMascot } from "../../tasks/useMascot";
import { groupSteps, summarize, summaryFacts } from "./stepGroups";
import { Timeline } from "./Timeline";

const t = translator("es");

const task: Task = {
  id: "t1",
  project: "/code/agenda",
  mode: "project",
  title: "IVA",
  prompt: "Corrige el IVA",
  account: "claude-personal",
  model: null,
  workspace: null,
  sessionId: null,
  status: "running",
  createdAt: 0,
  stages: [],
  stage: 0,
  live: true,
};

const resting: TaskMascot = { state: "idle", activity: undefined };
const working: TaskMascot = { state: "working", activity: "search" };

const queryClient = new QueryClient();
const api = new MockApi();

function timeline(events: AgentEvent[], mascot: TaskMascot) {
  return (
    <QueryClientProvider client={queryClient}>
      <ApiProvider api={api}>
        <Timeline
          task={task}
          items={buildTimeline(events, t)}
          agent="Claude"
          account={undefined}
          accounts={[]}
          mascot={mascot}
        />
      </ApiProvider>
    </QueryClientProvider>
  );
}

function show(events: AgentEvent[], mascot: TaskMascot = resting) {
  const { rerender } = render(timeline(events, mascot));
  return {
    user: userEvent.setup(),
    update: (next: AgentEvent[], nextMascot: TaskMascot = mascot) => rerender(timeline(next, nextMascot)),
  };
}

const phase: AgentEvent = { kind: "phaseStarted", phase: "implement", agent: "Claude Opus", account: "Claude personal" };

describe("step groups", () => {
  it("groups consecutive steps and lets anything else end the run", () => {
    const items = buildTimeline(
      [
        { kind: "userMessage", text: "Corrige el IVA" },
        phase,
        { kind: "toolUsed", tool: "Grep", summary: "iva" },
        { kind: "fileEdited", path: "a.py" },
        { kind: "message", text: "Ahora los tests." },
        { kind: "testsRun", command: "pytest", passed: true },
        { kind: "permissionRequest", requestId: "r1", tool: "Bash", summary: "rm", input: {} },
        { kind: "commandRun", command: "ls", tests: false },
        { kind: "plan", text: "1. Algo", requestId: null },
        { kind: "fileEdited", path: "b.py" },
        { kind: "limitWarning", account: "claude-personal", level: "warning", percent: 91, resetsAt: null },
        { kind: "fileEdited", path: "c.py" },
        { kind: "finished", ok: true, summary: "", durationMs: 1000 },
      ],
      t,
    );
    const entries = groupSteps(items);
    expect(entries.map((e) => (e.type === "group" ? `group:${e.steps.length}` : e.item.type))).toEqual([
      "user",
      "phase",
      "group:2",
      "agent",
      "group:1",
      "permission",
      "group:1",
      "plan",
      "group:1",
      "limit",
      "group:1",
      "finished",
    ]);
  });

  it("sums up files, commands, tests and blocks", () => {
    const items = buildTimeline(
      [
        { kind: "fileEdited", path: "a.py" },
        { kind: "fileEdited", path: "a.py" },
        { kind: "fileEdited", path: "b.py" },
        { kind: "commandRun", command: "ls", tests: false },
        { kind: "testsRun", command: "pytest", passed: false },
        { kind: "testsRun", command: "pytest", passed: true },
        { kind: "permissionResolved", requestId: "h1", decision: "deny", automatic: true, reason: ".env" },
      ],
      t,
    );
    const [group] = groupSteps(items);
    if (group?.type !== "group") throw new Error("expected a group");
    const summary = summarize(group.steps);
    expect(summary).toEqual({ count: 7, files: 2, commands: 1, tests: "recovered", blocked: 1 });
    expect(summaryFacts(summary, t)).toEqual([
      { text: "7 pasos", failed: false },
      { text: "editó 2 archivos", failed: false },
      { text: "corrió 1 comando", failed: false },
      { text: "tests pasaron tras fallar", failed: false },
      { text: "Hedy bloqueó 1", failed: true },
    ]);
    expect(summarize(group.steps.slice(0, 5)).tests).toBe("failed");
  });

  it("shows a run as one line that opens with the keyboard", async () => {
    const { user } = show([
      { kind: "userMessage", text: "Corrige el IVA" },
      phase,
      { kind: "toolUsed", tool: "Grep", summary: "iva" },
      { kind: "fileEdited", path: "facturas/impuestos.py" },
      { kind: "commandRun", command: "npm run build", tests: false },
      { kind: "message", text: "Listo." },
    ]);
    const group = screen.getByRole("button", { name: "3 pasos · editó 1 archivo · corrió 1 comando" });
    expect(group).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText("Claude editó facturas/impuestos.py")).toBeNull();

    await user.tab();
    expect(group).toHaveFocus();
    await user.keyboard("{Enter}");
    expect(group).toHaveAttribute("aria-expanded", "true");
    const list = document.getElementById(group.getAttribute("aria-controls") ?? "");
    expect(list).toHaveTextContent("Claude buscó · iva");
    expect(list).toHaveTextContent("Claude editó facturas/impuestos.py");
    expect(list).toHaveTextContent("Claude corrió npm run build");

    await user.keyboard(" ");
    expect(group).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText("Claude editó facturas/impuestos.py")).toBeNull();
  });

  it("keeps a run whose tests failed open, failures in sight", async () => {
    show([
      { kind: "userMessage", text: "Corrige el IVA" },
      phase,
      { kind: "fileEdited", path: "facturas/impuestos.py" },
      { kind: "testsRun", command: "pytest", passed: false },
      { kind: "permissionResolved", requestId: "h1", decision: "deny", automatic: true, reason: ".env" },
      { kind: "finished", ok: true, summary: "", durationMs: 1_200_000 },
    ]);
    const group = screen.getByRole("button", { name: "3 pasos · editó 1 archivo · tests fallaron · Hedy bloqueó 1" });
    expect(group).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText("Claude: fallan los tests · pytest").closest("li")).toHaveClass("step-group__item--failed");
    const failed = screen.getByText("tests fallaron");
    expect(failed).toHaveClass("step-group__fact--failed");
    expect(failed.querySelector(".step-group__mark")).toHaveTextContent("✗");
    expect(screen.getByText("Hedy bloqueó 1")).toHaveClass("step-group__fact--failed");
    // The end of the turn is a quiet footer, not a step.
    expect(screen.getByText(/^Terminó · 20 min/)).toHaveClass("turn-end");

    // Folded by hand, it stays folded with the failures in its line.
    await userEvent.click(group);
    expect(group).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText("Claude: fallan los tests · pytest")).toBeNull();
    expect(screen.getByText("tests fallaron")).toHaveClass("step-group__fact--failed");
  });

  it("keeps the run going on open, and folds it once text follows", () => {
    const events: AgentEvent[] = [
      { kind: "userMessage", text: "Corrige el IVA" },
      phase,
      { kind: "toolUsed", tool: "Read", summary: "facturas/impuestos.py" },
      { kind: "toolUsed", tool: "Grep", summary: "redondeo" },
    ];
    const { update } = show(events, working);
    expect(screen.getByRole("button", { name: "2 pasos" })).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText("Claude buscó · redondeo").closest(".step-row")).toHaveClass("step-row--live");
    expect(screen.getByText("Claude leyó · facturas/impuestos.py").closest(".step-row")).not.toHaveClass("step-row--live");

    // Text after it: the run is over and folds into its line.
    update([...events, { kind: "message", text: "El redondeo va por línea." }]);
    expect(screen.getByRole("button", { name: "2 pasos" })).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText("Claude buscó · redondeo")).toBeNull();
  });

  it("shows a single step as a line of its own", () => {
    show([{ kind: "userMessage", text: "Corre los tests" }, phase, { kind: "testsRun", command: "pytest", passed: false }]);
    expect(screen.queryByRole("button", { name: /pasos?/ })).toBeNull();
    expect(screen.getByText("Claude: fallan los tests · pytest").closest(".step-row")).toHaveClass("step-row--failed");
  });
});
