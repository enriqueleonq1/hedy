import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";

vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

function setup(api = MockApi.withSampleData().withSampleActivity()) {
  api.delayMs = 0;
  render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
  return { api, user: userEvent.setup() };
}

/** The titles of a board group's rows, in order. */
function titles(group: HTMLElement): string[] {
  return within(group)
    .getAllByRole("listitem")
    .map((row) => row.querySelector(".board-row__title")?.textContent ?? "");
}

describe("Activity", () => {
  afterEach(() => history.replaceState(null, "", "#"));

  it("lists every project's tasks, what needs you first", async () => {
    const { user } = setup();
    await user.click(await screen.findByRole("button", { name: "Actividad: 3 tareas te necesitan" }));
    expect(await screen.findByRole("heading", { name: "Actividad" })).toBeInTheDocument();

    const needs = await screen.findByRole("region", { name: "Te necesitan (3)" });
    expect(titles(needs)).toEqual([
      "Agrega recordatorios por WhatsApp antes de cada cita",
      "Migra el reporte de comisiones a la API nueva",
      "Traduce los correos de confirmación",
    ]);
    const plan = within(needs).getAllByRole("listitem")[0]!;
    expect(within(plan).getByText("Plan por aprobar")).toBeInTheDocument();
    expect(within(plan).getByText("hedy")).toBeInTheDocument();
    expect(within(plan).getByText("Claude Opus")).toBeInTheDocument();
    expect(within(plan).getByText(/^\d+ min en marcha$/)).toBeInTheDocument();
    expect(within(needs).getByText("Pide permiso")).toBeInTheDocument();
    expect(within(needs).getByText("Bloqueada por el límite")).toBeInTheDocument();
    expect(screen.getByText(/Hedy nunca cambia de cuenta solo/)).toBeInTheDocument();

    // The account's color is a dot next to its name, never the text's color.
    const who = within(plan).getByText("Claude personal");
    expect(who.style.color).toBe("");
    expect(who.querySelector<HTMLElement>(".dot")?.style.background).toBe("rgb(187, 154, 247)");

    const running = screen.getByRole("region", { name: "En curso (1)" });
    expect(within(running).getByText("Trabajando")).toBeInTheDocument();
    expect(within(running).getByText("homming")).toBeInTheDocument();

    const recent = screen.getByRole("region", { name: "Últimas 24 h (4)" });
    expect(titles(recent)).toEqual([
      "Arregla el cálculo de impuestos en las facturas",
      "Actualiza las dependencias del panel",
      "¿Dónde se calculan las comisiones?",
      "Cambia el color del botón de reservar",
    ]);
    for (const state of ["Cambios por revisar", "Falló", "Respondida", "Descartada"]) {
      expect(within(recent).getByText(state)).toBeInTheDocument();
    }
    expect(within(recent).getByText("hace 40 min")).toBeInTheDocument();
  });

  it("opens a task in its project's workspace", async () => {
    const { user } = setup();
    await user.click(await screen.findByRole("button", { name: "Actividad: 3 tareas te necesitan" }));
    const needs = await screen.findByRole("region", { name: "Te necesitan (3)" });
    await user.click(within(needs).getByRole("button", { name: /Migra el reporte de comisiones/ }));
    expect(await screen.findByRole("heading", { name: "Migra el reporte de comisiones a la API nueva" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "homming" })).toHaveAttribute("aria-current", "page");
    expect(window.location.hash).toBe(`#p=${encodeURIComponent("/code/homming")}`);
  });

  it("opens the task of a clicked desktop notification", async () => {
    const { api } = setup();
    expect(await screen.findByRole("heading", { name: /¡Hola, Enrique!/ })).toBeInTheDocument();
    api.simulateNotificationClick("s3");
    expect(await screen.findByRole("heading", { name: "Traduce los correos de confirmación" })).toBeInTheDocument();
  });

  it("starts empty and follows a task live", async () => {
    const { api, user } = setup(MockApi.withSampleData());
    await user.click(await screen.findByRole("button", { name: "Actividad" }));
    expect(await screen.findByText("No hay tareas en curso ni de las últimas 24 h.")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "Durmiendo" })).toBeInTheDocument();

    await api.startTask({
      project: "/code/hedy",
      prompt: "Agrega un recordatorio",
      mode: "project",
      model: null,
      planFirst: true,
    });
    // The plan arrives: the task moves to "needs you" and the badge counts it.
    const needs = await screen.findByRole("region", { name: "Te necesitan (1)" });
    expect(within(needs).getByText("Agrega un recordatorio")).toBeInTheDocument();
    expect(within(needs).getByText("Plan por aprobar")).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Actividad: 1 tarea te necesita" })).toBeInTheDocument(),
    );
  });
});
