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
    expect(within(strip).getByText("Arquitecto: hecho · 50k tokens · 45 s")).toBeInTheDocument();
    expect(within(strip).getByText("Tu aprobación: ahora")).toBeInTheDocument();
    expect(within(strip).getByText("Final: pendiente")).toBeInTheDocument();
    expect(screen.getByText("npm test: no pasa")).toBeInTheDocument();
    expect(screen.getByText("npm test: pasa")).toBeInTheDocument();
    expect(screen.getByText("el veredicto de la revisión: pasa")).toBeInTheDocument();
  });

  it("says what the run and each of its steps spent", async () => {
    await setup();
    const strip = await progress();
    const costs = within(strip).getByRole("list", { name: "Lo que gastó cada paso" });
    expect(within(costs).getAllByRole("listitem").map((li) => li.textContent)).toEqual([
      "En total 227k tokens · 4 min · caché 89 %",
      "Arquitecto 50k tokens · 45 s",
      "Programador 148k tokens · 2 min",
      "Revisor 30k tokens · 50 s",
    ]);
    // A check spends nothing and says nothing.
    expect(within(strip).getByText("¿Pasan los tests?: hecho")).toBeInTheDocument();
  });

  it("a loop out of rounds pauses for you, with the last ask, and writes the next round in the composer", async () => {
    const api = MockApi.withSampleData().withSampleFlowOutOfRounds();
    api.delayMs = 0;
    history.replaceState(null, "", TASK);
    render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
    const user = userEvent.setup();
    const notice = await screen.findByRole("region", { name: "Se acabaron las vueltas" }, { timeout: 5000 });
    expect(within(notice).getByText(/las 2 veces permitidas/)).toBeInTheDocument();
    expect(within(notice).getByText(/Falta validar la firma del webhook/)).toBeInTheDocument();
    await user.click(within(notice).getByRole("button", { name: "Una vuelta más" }));
    expect(screen.getByLabelText("Responde o pide algo más")).toHaveValue("Una vuelta más: corrige lo último que se pidió.");
  });

  it("opens the board: the latest of each section, as plain text", async () => {
    const { user } = await setup();
    await user.click(within(await progress()).getByRole("button", { name: "Pizarra" }));
    const board = await screen.findByRole("list", { name: "Pizarra" });
    // Sections by the names people know, not their ids.
    expect(within(board).getByText("Contexto")).toBeInTheDocument();
    expect(within(board).queryByText("architect")).not.toBeInTheDocument();
    expect(within(board).getAllByText("Arquitecto").length).toBeGreaterThan(0);
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

  it("a message written while it waits leaves the approval waiting", async () => {
    const { api, user } = await setup();
    await card();
    await api.sendMessage("f1", "¿Por qué el cliente va en src/pagos?");
    const approval = await card();
    await waitFor(() => expect(screen.getByText("¿Por qué el cliente va en src/pagos?")).toBeInTheDocument());
    expect(within(approval).getByRole("button", { name: /Aprobar/ })).toBeEnabled();
    await user.click(within(approval).getByRole("button", { name: /Aprobar/ }));
    await waitFor(() => expect(within(approval).getByText(/aprobado/)).toBeInTheDocument());
  });

  it("knows how each approval was answered, and which ask is the latest", () => {
    const t = ((key: string) => key) as Parameters<typeof buildTimeline>[1];
    const ask: AgentEvent = { kind: "flowApproval", node: "n3" };
    const said = (approved: boolean): AgentEvent => ({ kind: "flowApprovalAnswered", node: "n3", approved });
    const answers = (events: AgentEvent[]) =>
      buildTimeline(events, t).flatMap((i) => (i.type === "flowApproval" ? [[i.answer, i.latest]] : []));
    expect(answers([ask])).toEqual([[null, true]]);
    // A message is not an answer.
    expect(answers([ask, { kind: "userMessage", text: "¿por qué?" }])).toEqual([[null, true]]);
    expect(answers([ask, said(false), ask, said(true)])).toEqual([
      ["changes", false],
      ["approved", true],
    ]);
  });
});
