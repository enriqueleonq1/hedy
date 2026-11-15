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

/** The board loads after the overview; on a busy machine that can take a while. */
const SLOW = { timeout: 8000 };

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
    await user.click(await screen.findByRole("button", { name: "Actividad: 8 pendientes te esperan" }, SLOW));
    expect(await screen.findByRole("heading", { name: "Actividad" })).toBeInTheDocument();

    const needs = await screen.findByRole("region", { name: "Te necesitan (8)" });
    // The tasks first, then the notes and skills the AIs proposed, newest first.
    expect(titles(needs)).toEqual([
      "Agrega recordatorios por WhatsApp antes de cada cita",
      "Migra el reporte de comisiones a la API nueva",
      "Traduce los correos de confirmación",
      "agregar-endpoint — Cómo agregar un endpoint nuevo a la API de citas.",
      "correr-tests — Correr los tests del backend, del frontend y los e2e antes de entregar una tarea.",
      "Los formularios validan con zod",
      "Las citas usan la zona horaria del salón; el servidor guarda las fechas en UTC",
      "Los precios se guardan en USD",
    ]);
    expect(within(needs).getAllByText("Habilidad por aprobar")).toHaveLength(1);
    expect(within(needs).getByText("Cambio de habilidad por aprobar")).toBeInTheDocument();
    expect(within(needs).getAllByText("Nota por aprobar")).toHaveLength(2);
    expect(within(needs).getByText("Cambio de nota por aprobar")).toBeInTheDocument();
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
    // What it is doing, as a word next to the state (decorative).
    const word = await within(running).findByText("Rastreando");
    expect(word.closest(".working-word")).toHaveAttribute("aria-hidden");
    expect(within(running).getByText("homming")).toBeInTheDocument();

    const recent = screen.getByRole("region", { name: "Últimas 24 h (5)" });
    expect(titles(recent)).toEqual([
      "Arregla el cálculo de impuestos en las facturas",
      "Actualiza las dependencias del panel",
      "¿Qué diferencia hay entre OAuth y OpenID Connect?",
      "¿Dónde se calculan las comisiones?",
      "Cambia el color del botón de reservar",
    ]);
    // A question outside any project reads as a conversation, not a project.
    expect(within(recent).getByText("Conversación")).toBeInTheDocument();
    for (const state of ["Cambios por revisar", "Falló", "Respondida", "Descartada"]) {
      expect(within(recent).getAllByText(state)[0]).toBeInTheDocument();
    }
    expect(within(recent).getByText("hace 40 min")).toBeInTheDocument();
  });

  it("opens a task in its project's workspace", async () => {
    const { user } = setup();
    await user.click(await screen.findByRole("button", { name: "Actividad: 8 pendientes te esperan" }, SLOW));
    const needs = await screen.findByRole("region", { name: "Te necesitan (8)" });
    await user.click(within(needs).getByRole("button", { name: /Migra el reporte de comisiones/ }));
    expect(await screen.findByRole("heading", { name: "Migra el reporte de comisiones a la API nueva" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "homming" })).toHaveAttribute("aria-current", "page");
    expect(window.location.hash).toBe(`#p=${encodeURIComponent("/code/homming")}&t=s2`);
  });

  it("answers a permission from its row, once or no, and never for the whole task", async () => {
    const { api, user } = setup();
    const answer = vi.spyOn(api, "answerPermission");
    await user.click(await screen.findByRole("button", { name: "Actividad: 8 pendientes te esperan" }, SLOW));
    const decide = await screen.findByRole("group", { name: "Responder el permiso de «Migra el reporte de comisiones a la API nueva»" }, SLOW);
    expect(within(decide).getByText("Bash: npm test")).toBeInTheDocument();
    expect(within(decide).queryByRole("button", { name: /en este chat/ })).not.toBeInTheDocument();
    await user.click(within(decide).getByRole("button", { name: "Permitir una vez" }));
    expect(answer).toHaveBeenCalledWith("s2", "req-s2", "allow");
    // Still on the board: answering did not open the task.
    expect(screen.getByRole("heading", { name: "Actividad" })).toBeInTheDocument();
  });

  it("approves and rejects proposed notes and skills from their rows", async () => {
    const { api, user } = setup();
    const approve = vi.spyOn(api, "approveNote");
    const reject = vi.spyOn(api, "rejectSkill");
    await user.click(await screen.findByRole("button", { name: "Actividad: 8 pendientes te esperan" }, SLOW));
    const zod = await screen.findByRole("group", { name: "Decidir sobre «Los formularios validan con zod»" }, SLOW);
    await user.click(within(zod).getByRole("button", { name: "Aprobar" }));
    expect(approve).toHaveBeenCalledTimes(1);
    expect(await screen.findByRole("region", { name: "Te necesitan (7)" })).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("button", { name: "Actividad: 7 pendientes te esperan" })).toBeInTheDocument());

    const skill = screen.getByRole("group", { name: /^Decidir sobre «agregar-endpoint/ });
    await user.click(within(skill).getByRole("button", { name: "Rechazar" }));
    expect(reject).toHaveBeenCalledTimes(1);
    expect(await screen.findByRole("region", { name: "Te necesitan (6)" })).toBeInTheDocument();
    // A change to an approved note says so on its button.
    const change = screen.getByRole("group", { name: /^Decidir sobre «Las citas usan la zona horaria/ });
    expect(within(change).getByRole("button", { name: "Aprobar cambio" })).toBeInTheDocument();
  });

  it("opens a proposal in its project's memory", async () => {
    const { user } = setup();
    await user.click(await screen.findByRole("button", { name: "Actividad: 8 pendientes te esperan" }, SLOW));
    const needs = await screen.findByRole("region", { name: "Te necesitan (8)" }, SLOW);
    await user.click(within(needs).getByRole("button", { name: /^agregar-endpoint/ }));
    expect(await screen.findByRole("heading", { name: "Memoria de hedy" })).toBeInTheDocument();
    expect(window.location.hash).toBe(`#k=${encodeURIComponent("/code/hedy")}`);
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
    const needs = await screen.findByRole("region", { name: "Te necesitan (6)" }, SLOW);
    expect(within(needs).getByText("Agrega un recordatorio")).toBeInTheDocument();
    expect(within(needs).getByText("Plan por aprobar")).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Actividad: 6 pendientes te esperan" })).toBeInTheDocument(),
    );
  });
});
