import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";
import type { Account, Task } from "../../api/types";
import { translator } from "../../i18n";
import { headline, usageByDay } from "./StatusScreen";

vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

function setup() {
  const api = MockApi.withSampleData();
  api.delayMs = 0;
  render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
  return { api, user: userEvent.setup() };
}

const task = (extra: Partial<Task>): Task => ({
  id: "t1",
  project: "/code/hedy",
  mode: "project",
  title: "Reporte de comisiones",
  prompt: "",
  account: "chatgpt",
  model: null,
  workspace: null,
  sessionId: null,
  status: "running",
  createdAt: 0,
  stages: [],
  stage: 0,
  live: true,
  ...extra,
});

describe("Status", () => {
  afterEach(() => history.replaceState(null, "", "#"));

  it("groups usage by local day", () => {
    const today = new Date(2025, 2, 7, 18);
    const at = (d: number, h: number) => new Date(2025, 2, d, h).getTime() / 1000;
    const days = usageByDay(
      [
        { at: at(7, 9), tokens: 10, costUsd: null },
        { at: at(7, 23), tokens: 5, costUsd: null },
        { at: at(3, 1), tokens: 7, costUsd: null },
        { at: new Date(2025, 1, 28, 12).getTime() / 1000, tokens: 99, costUsd: null },
      ],
      today,
    );
    expect(days).toHaveLength(7);
    expect(days[6]?.tokens).toBe(15);
    expect(days[2]?.tokens).toBe(7);
    expect(days.reduce((s, d) => s + d.tokens, 0)).toBe(22);
  });

  it("says who works and who comes next", () => {
    const accounts = [{ id: "claude", name: "Claude personal" }] as Account[];
    const running = task({
      stages: [
        { phase: "implement", account: "chatgpt", model: null },
        { phase: "review", account: "claude", model: "opus" },
      ],
      stage: 0,
    });
    expect(headline(running, "Codex", accounts, translator("es"))).toBe(
      "Codex implementa «Reporte de comisiones»; después Claude personal lo revisará.",
    );
    expect(headline(task({ status: "waitingApproval" }), "Codex", accounts, translator("es"))).toBe(
      "«Reporte de comisiones» espera tu respuesta.",
    );
    expect(headline(undefined, "", accounts, translator("es"))).toMatch(/No hay tareas/);
  });

  it("shows the project's indicators, week and commits", async () => {
    const { user } = setup();
    await user.click(await screen.findByRole("button", { name: /^hedy/ }));
    await user.click(await screen.findByRole("button", { name: "Estado" }));
    expect(await screen.findByRole("heading", { name: "hedy" })).toBeInTheDocument();
    const tests = await screen.findByRole("region", { name: "Tests" });
    expect(within(tests).getByText("Pasaron")).toBeInTheDocument();
    expect(within(screen.getByRole("region", { name: "Memoria" })).getByText(/notas/)).toBeInTheDocument();
    expect(screen.getByText(/rama main · 3 cambios sin commit/)).toBeInTheDocument();
    expect(await screen.findByText(/fue el día más intenso/)).toBeInTheDocument();
    expect(screen.getByText("a3f9c1e")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Memoria" }));
    expect(await screen.findByRole("heading", { name: "Memoria de hedy" })).toBeInTheDocument();
  });

  it("adds up the week per client and edits a project's client", async () => {
    const { api, user } = setup();
    await user.click(await screen.findByRole("button", { name: "Cuentas y modelos" }));
    const spend = await screen.findByRole("region", { name: "Uso de la semana por cliente" });
    expect(await within(spend).findByText("León Blanco")).toBeInTheDocument();
    expect(within(spend).getByText("Homming")).toBeInTheDocument();

    const input = screen.getByLabelText("Cliente de hedy");
    await user.clear(input);
    await user.type(input, "Moira SA");
    await user.tab();
    expect((await api.overview()).projects.find((p) => p.name === "hedy")?.client).toBe("Moira SA");
  });
});
