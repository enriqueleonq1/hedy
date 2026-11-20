import { render, screen } from "@testing-library/react";

import type { BoardState, Task } from "../../api/types";
import { I18nProvider, translator, type Language } from "../../i18n";
import { pillLabel, StatusPill } from "./StatusPill";

const task = (extra: Partial<Task> = {}): Task => ({
  id: "t1",
  project: "/code/agenda",
  mode: "project",
  title: "Recordatorios",
  prompt: "Agrega recordatorios",
  account: "claude-personal",
  model: null,
  workspace: { worktree: "/code/agenda/.hedy/worktrees/t1", branch: "hedy/recordatorios-t1", baseBranch: "main", baseCommit: "a3f9c1e" },
  sessionId: null,
  status: "running",
  createdAt: 0,
  stages: [],
  stage: 0,
  live: true,
  ...extra,
});

const label = (state: BoardState, of: Task = task(), language: Language = "es") =>
  pillLabel(state, of, translator(language), language);

const ship = (branch: string, kept: boolean) => ({ branch, kept, message: "", files: [], pushedTo: null, pullRequest: null });
const merge = { before: "a", after: "b", tip: "c", at: 0 };

describe("pillLabel", () => {
  it("names each state in one or two words", () => {
    expect(label("working")).toBe("Trabajando");
    expect(label("plan")).toBe("Te espera: plan");
    expect(label("permission")).toBe("Te espera: permiso");
    expect(label("review")).toBe("Lista");
    expect(label("answered")).toBe("Respondida");
    expect(label("failed")).toBe("Falló");
    expect(label("cancelled")).toBe("Descartada");
    expect(label("plan", task(), "en")).toBe("Waiting for you: plan");
  });

  it("says when a paused task resumes, or that it can go on", () => {
    expect(label("limited")).toBe("En pausa");
    // 13:21 today, local time.
    const until = new Date(2026, 10, 20, 13, 21).getTime() / 1000;
    const waiting = task({ wait: { account: "claude-personal", until, since: until - 600, ready: false } });
    expect(label("limited", waiting)).toBe("En pausa · retoma 13:21");
    expect(label("limited", waiting, "en")).toMatch(/^Paused · resumes (13:21|01:21\sPM)$/);
    expect(label("limited", task({ wait: { account: "claude-personal", until: null, since: 0, ready: false } }))).toBe("En pausa");
    expect(label("limited", task({ wait: { account: "claude-personal", until, since: 0, ready: true } }))).toBe("En pausa · puede seguir");
  });

  it("says where an accepted task's work went", () => {
    expect(label("accepted", task({ status: "accepted", ship: ship("main", false), merge }))).toBe("Unida a main");
    // Merged before shipments were recorded: its base branch.
    expect(label("accepted", task({ status: "accepted", merge }))).toBe("Unida a main");
    expect(label("accepted", task({ status: "accepted", ship: ship("hedy/recordatorios-t1", true) }))).toBe("En su rama");
    expect(label("accepted", task({ status: "accepted" }))).toBe("Aceptada");
    expect(label("accepted", task({ status: "accepted", ship: ship("develop", false), merge }), "en")).toBe("Merged into develop");
  });
});

describe("StatusPill", () => {
  it("is a dot in the state's tone and its word", () => {
    const cases: [BoardState, string][] = [
      ["plan", "needs"],
      ["permission", "needs"],
      ["limited", "blocked"],
      ["working", "working"],
      ["review", "done"],
      ["answered", "done"],
      ["failed", "failed"],
      ["accepted", "closed"],
      ["cancelled", "closed"],
    ];
    for (const [state, tone] of cases) {
      const { container, unmount } = render(
        <I18nProvider setting="es">
          <StatusPill state={state} task={task()} />
        </I18nProvider>,
      );
      const pill = container.querySelector(".status-pill");
      expect(pill).toHaveClass(`status-pill--${tone}`);
      expect(pill?.querySelector(".status-pill__dot")).toHaveAttribute("aria-hidden");
      expect(pill).toHaveTextContent(label(state));
      unmount();
    }
    render(
      <I18nProvider setting="es">
        <StatusPill state="review" task={task()} />
      </I18nProvider>,
    );
    expect(screen.getByText("Lista")).toHaveClass("status-pill");
  });
});
