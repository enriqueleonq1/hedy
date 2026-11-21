import { QueryClient } from "@tanstack/react-query";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";
import type { AgentEvent } from "../../api/types";
import { buildTimeline } from "../../tasks/timeline";

vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

const TASK = `#p=${encodeURIComponent("/code/tienda")}&t=f1`;

async function setup() {
  const api = MockApi.withSampleData().withSampleFlowRun();
  api.delayMs = 0;
  history.replaceState(null, "", TASK);
  render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
  return { api, user: userEvent.setup() };
}

const progress = () => screen.findByRole("region", { name: "Flujo Pago con revisión" }, { timeout: 5000 });
const card = () => screen.findByRole("region", { name: "Tu aprobación" }, { timeout: 5000 });

afterEach(() => {
  history.replaceState(null, "", "#");
  sessionStorage.clear();
  localStorage.clear();
});

describe("A flow task, live", () => {
  it("shows where the flow is, its round, and each check in the timeline", async () => {
    await setup();
    const strip = await progress();
    expect(within(strip).getByText("Espera tu aprobación")).toBeInTheDocument();
    expect(within(strip).getByText("ronda 2")).toBeInTheDocument();
    // Every step, with its state for screen readers.
    expect(within(strip).getByText("Arquitecto: hecho")).toBeInTheDocument();
    expect(within(strip).getByText("Tu aprobación: ahora")).toBeInTheDocument();
    expect(within(strip).getByText("Final: pendiente")).toBeInTheDocument();
    expect(screen.getByText("npm test: no pasa")).toBeInTheDocument();
    expect(screen.getByText("npm test: pasa")).toBeInTheDocument();
    expect(screen.getByText("el veredicto de la revisión: pasa")).toBeInTheDocument();
  });

  it("opens the board: the latest of each section, as plain text", async () => {
    const { user } = await setup();
    await user.click(within(await progress()).getByRole("button", { name: "Pizarra" }));
    const board = await screen.findByRole("list", { name: "Pizarra" });
    expect(within(board).getByText("architect")).toBeInTheDocument();
    expect(within(board).getByText(/VEREDICTO: APRUEBA/)).toBeInTheDocument();
  });

  it("approving moves the flow on", async () => {
    const { api, user } = await setup();
    const answer = vi.spyOn(api, "answerFlowApproval");
    const approval = await card();
    expect(within(approval).getByText(/queda para tu revisión final/)).toBeInTheDocument();
    await user.click(within(approval).getByRole("button", { name: /Aprobar/ }));
    await waitFor(() => expect(answer).toHaveBeenCalledWith("f1", true, null));
    await waitFor(() => expect(within(approval).queryByRole("button", { name: /Aprobar/ })).not.toBeInTheDocument());
  });

  it("asking for changes sends what to change", async () => {
    const { api, user } = await setup();
    const answer = vi.spyOn(api, "answerFlowApproval");
    const approval = await card();
    await user.click(within(approval).getByRole("button", { name: /Pedir cambios/ }));
    await user.type(within(approval).getByRole("textbox", { name: "¿Qué hay que cambiar?" }), "El cobro va en el servidor");
    await user.click(within(approval).getByRole("button", { name: /Enviar/ }));
    await waitFor(() => expect(answer).toHaveBeenCalledWith("f1", false, "El cobro va en el servidor"));
  });

  it("knows how each approval was answered", () => {
    const t = ((key: string) => key) as Parameters<typeof buildTimeline>[1];
    const ask: AgentEvent = { kind: "flowApproval", node: "n3" };
    const phase: AgentEvent = { kind: "phaseStarted", phase: "implement", agent: "A", account: "B" };
    const answers = (events: AgentEvent[]) =>
      buildTimeline(events, t).flatMap((i) => (i.type === "flowApproval" ? [i.answer] : []));
    expect(answers([ask])).toEqual([null]);
    expect(answers([ask, phase])).toEqual(["approved"]);
    expect(answers([ask, { kind: "userMessage", text: "otra cosa" }, phase])).toEqual(["changes"]);
  });
});
