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

/** A project's summary: its home's "Resumen" tab. */
async function openSummary(user: ReturnType<typeof userEvent.setup>) {
  const sections = await screen.findByRole("navigation", { name: "Secciones del proyecto" });
  await user.click(within(sections).getByRole("button", { name: "Resumen" }));
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
    await openSummary(user);
    expect(await screen.findByRole("heading", { name: "hedy" })).toBeInTheDocument();
    const tests = await screen.findByRole("region", { name: "Tests" });
    expect(within(tests).getByText("Pasaron")).toBeInTheDocument();
    // The value stays in ink; a dot says what it means.
    expect(tests.querySelector(".stat__dot--ok")).not.toBeNull();
    const memory = screen.getByRole("region", { name: "Memoria" });
    expect(within(memory).getByText(/notas/)).toBeInTheDocument();
    expect(memory.querySelector(".stat__dot--warn")).not.toBeNull();
    // Uncommitted work on the branch, instead of a setting.
    const changes = screen.getByRole("region", { name: "Cambios" });
    expect(within(changes).getByText("3 sin commit")).toBeInTheDocument();
    expect(within(changes).getByText("rama main")).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Mapa del código" })).not.toBeInTheDocument();
    expect(await screen.findByText(/fue el día más intenso/)).toBeInTheDocument();
    expect(screen.getByText("a3f9c1e")).toBeInTheDocument();

    const hero = screen.getByRole("region", { name: "Estado del proyecto" });
    await user.click(within(hero).getByRole("button", { name: "Memoria" }));
    expect(await screen.findByRole("heading", { level: 1, name: "Memoria" })).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Para" })).toHaveDisplayValue("hedy");
  });

  it("opens the project's memory and summary from its home's tabs", async () => {
    const { user } = setup();
    await user.click(await screen.findByRole("button", { name: /^hedy/ }));
    // A project opens on its home, not on a task.
    expect(await screen.findByRole("heading", { level: 1, name: "hedy" })).toBeInTheDocument();
    const sections = screen.getByRole("navigation", { name: "Secciones del proyecto" });
    expect(within(sections).getByRole("button", { name: "Tareas" })).toHaveAttribute("aria-current", "page");

    await user.click(within(sections).getByRole("button", { name: "Resumen" }));
    expect(await screen.findByRole("region", { name: "Tests" })).toBeInTheDocument();
    expect(window.location.hash).toBe(`#s=${encodeURIComponent("/code/hedy")}`);

    await user.click(screen.getByRole("button", { name: /^hedy/ }));
    const again = await screen.findByRole("navigation", { name: "Secciones del proyecto" });
    await user.click(within(again).getByRole("button", { name: "Memoria" }));
    expect(await screen.findByRole("heading", { level: 1, name: "Memoria" })).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Para" })).toHaveDisplayValue("hedy");
    expect(window.location.hash).toBe(`#m=${encodeURIComponent("/code/hedy")}`);
  });

  it("offers to turn on the code map in Settings while it is off", async () => {
    const { user } = setup();
    await user.click(await screen.findByRole("button", { name: /^hedy/ }));
    await openSummary(user);
    await user.click(await screen.findByRole("button", { name: "Activar mapa del código" }));
    expect(await screen.findByRole("heading", { name: "Ajustes" })).toBeInTheDocument();
  });

  it("adds up the week per client and edits a project's client", async () => {
    const { api, user } = setup();
    await user.click(await screen.findByRole("button", { name: /Abrir la cuenta$/ }));
    const spend = await screen.findByRole("region", { name: "Uso de la semana por cliente" });
    expect(await within(spend).findByText("Estudio Norte")).toBeInTheDocument();
    expect(within(spend).getByText("Café Aurora")).toBeInTheDocument();
    // Only subscriptions worked: usage in tokens, no cost (the CLI's guess is not a bill).
    expect(within(spend).queryByRole("columnheader", { name: /Costo estimado/ })).not.toBeInTheDocument();
    expect(within(spend).queryByText(/\$/)).not.toBeInTheDocument();

    const input = screen.getByLabelText("Cliente de hedy");
    expect(input).toHaveAccessibleDescription(/agrupa proyectos del mismo cliente para ver cuánto uso lleva cada uno/);
    await user.clear(input);
    await user.type(input, "Taller Sur");
    await user.tab();
    expect((await api.overview()).projects.find((p) => p.name === "hedy")?.client).toBe("Taller Sur");
  });

  it("shows the week per client only once a project has a client", async () => {
    const api = MockApi.withSampleData();
    api.delayMs = 0;
    await api.setProjectClient("/code/hedy", null);
    await api.setProjectClient("/code/tienda", null);
    render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: /Abrir la cuenta$/ }));
    const input = await screen.findByLabelText("Cliente de hedy");
    expect(screen.queryByRole("region", { name: "Uso de la semana por cliente" })).not.toBeInTheDocument();

    await user.type(input, "Estudio Norte");
    await user.tab();
    const spend = await screen.findByRole("region", { name: "Uso de la semana por cliente" });
    expect(await within(spend).findByText("Estudio Norte")).toBeInTheDocument();
  });

  it("adds an estimated cost only for what an API key spent", async () => {
    const api = MockApi.withSampleData();
    api.delayMs = 0;
    api.recordUsage({ project: "/code/hedy", account: "kimi", at: Math.floor(Date.now() / 1000), tokens: 1000, costUsd: 0.5 });
    render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: /Abrir la cuenta$/ }));
    const spend = await screen.findByRole("region", { name: "Uso de la semana por cliente" });
    expect(await within(spend).findByRole("columnheader", { name: "Costo estimado (API key)" })).toBeInTheDocument();
    const norte = within(spend).getByRole("row", { name: /^Estudio Norte/ });
    expect(within(norte).getByText("$0,50")).toBeInTheDocument();
    expect(within(spend).getByText(/solo cuenta las cuentas con API key/)).toBeInTheDocument();
  });
});
