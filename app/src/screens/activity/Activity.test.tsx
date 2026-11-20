import { act, render, screen, waitFor, within } from "@testing-library/react";
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

/** The sample inbox: three tasks, a sync conflict, a team's suggestions and five proposals. */
const NEEDS = "Te necesitan (10)";

/** The titles of a board group's rows, in order. */
function titles(group: HTMLElement): string[] {
  return within(group)
    .getAllByRole("listitem")
    .map((row) => row.querySelector(".board-row__title")?.textContent ?? "");
}

/** The selected row's title, if any. */
function selectedTitle(): string | null {
  return document.querySelector(".board-entry--selected .board-row__title")?.textContent ?? null;
}

describe("Inicio's board", () => {
  afterEach(() => history.replaceState(null, "", "#"));

  it("lists what needs you first, then what is ready to review", async () => {
    setup();
    const needs = await screen.findByRole("region", { name: NEEDS }, SLOW);
    // Inicio is the board: the greeting is the page's title, Actividad is gone.
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("¡Hola, Alex!");
    expect(screen.queryByRole("heading", { name: "Actividad" })).not.toBeInTheDocument();
    expect(screen.getByRole("radio", { name: /^Te necesitan/ })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("radio", { name: /^Te necesitan/ })).toHaveTextContent(/10$/);
    expect(screen.getByRole("radio", { name: /^En marcha/ })).toHaveTextContent(/1$/);
    expect(screen.getByRole("radio", { name: /^Listas/ })).toHaveTextContent(/3$/);
    expect(screen.getByRole("button", { name: "Inicio: 10 cosas te esperan" })).toBeInTheDocument();

    // The tasks first, then sync and the team, then the notes and skills the AIs proposed, newest first.
    expect(titles(needs)).toEqual([
      "Agrega recordatorios por WhatsApp antes de cada cita",
      "Migra el reporte de comisiones a la API nueva",
      "Traduce los correos de confirmación",
      "Un cambio en tus computadoras para resolver",
      "2 sugerencias del equipo Plataforma",
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
    expect(within(needs).getByText("Sincronización")).toBeInTheDocument();
    expect(within(needs).getByText("Equipo")).toBeInTheDocument();
    const plan = within(needs).getAllByRole("listitem")[0]!;
    expect(within(plan).getByText("Te espera: plan")).toBeInTheDocument();
    expect(within(plan).getByText("hedy")).toBeInTheDocument();
    expect(within(plan).getByText("Claude Opus")).toBeInTheDocument();
    expect(within(plan).getByText(/^\d+ min en marcha$/)).toBeInTheDocument();
    expect(within(needs).getByText("Te espera: permiso")).toBeInTheDocument();
    expect(within(needs).getByText(/^En pausa/)).toBeInTheDocument();
    expect(within(needs).getByText(/^sigue en 4\d min$/)).toBeInTheDocument();
    expect(screen.getByText(/Hedy nunca cambia de cuenta solo/)).toBeInTheDocument();

    // The account's color is a dot next to its name, never the text's color.
    const who = within(plan).getByText("Claude personal");
    expect(who.style.color).toBe("");
    expect(who.querySelector<HTMLElement>(".dot")?.style.background).toBe("rgb(187, 154, 247)");

    const ready = screen.getByRole("region", { name: "Listas para revisar (3)" });
    expect(titles(ready)).toEqual([
      "Arregla el cálculo de impuestos en las facturas",
      "¿Qué diferencia hay entre OAuth y OpenID Connect?",
      "¿Dónde se calculan las comisiones?",
    ]);
    // A question outside any project reads as a conversation, not a project.
    expect(within(ready).getByText("Conversación")).toBeInTheDocument();
    expect(within(ready).getAllByText("Lista")[0]).toBeInTheDocument();
    expect(within(ready).getAllByText("Respondida")).toHaveLength(2);
    expect(within(ready).getByText("hace 40 min")).toBeInTheDocument();
    // An answered task offers its answer; changes, their review.
    expect(within(ready).getAllByRole("button", { name: "Ver la respuesta" })).toHaveLength(2);
    expect(within(ready).getByRole("button", { name: "Revisar cambios" })).toBeInTheDocument();
    // A task stopped by an error stays in sight, under what is ready, out of the badge.
    const failed = screen.getByRole("region", { name: "Con error (1)" });
    expect(titles(failed)).toEqual(["Actualiza las dependencias del panel"]);
    expect(within(failed).getByText("Falló")).toBeInTheDocument();
    expect(ready.compareDocumentPosition(failed) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    // Finished tasks no longer get a group of their own: they are in the history.
    expect(screen.queryByRole("region", { name: /^Últimas/ })).not.toBeInTheDocument();
    expect(screen.queryByText("Cambia el color del botón de reservar")).not.toBeInTheDocument();
  });

  it("opens a failed task from its group to see what happened", async () => {
    const { user } = setup();
    const failed = await screen.findByRole("region", { name: "Con error (1)" }, SLOW);
    await user.click(within(failed).getByRole("button", { name: "Ver qué pasó" }));
    expect(await screen.findByRole("heading", { name: "Actualiza las dependencias del panel" })).toBeInTheDocument();
    expect(window.location.hash).toBe(`#p=${encodeURIComponent("/code/tienda")}&t=s7`);
  });

  it("shows nothing in what needs you while the board loads", async () => {
    const api = MockApi.withSampleData().withSampleActivity();
    let release!: () => void;
    const gate = new Promise<void>((resolve) => (release = resolve));
    const activity = api.activity.bind(api);
    vi.spyOn(api, "activity").mockImplementation(async () => {
      await gate;
      return activity();
    });
    setup(api);
    expect(await screen.findByRole("heading", { level: 1 }, SLOW)).toHaveTextContent("¡Hola, Alex!");
    await screen.findByRole("radiogroup", { name: "Mostrar" });
    // Not "nothing waits" (it may), nor a half inbox without the tasks.
    expect(screen.queryByText("Nada te espera")).not.toBeInTheDocument();
    expect(screen.queryByRole("region", { name: /^Te necesitan/ })).not.toBeInTheDocument();
    release();
    expect(await screen.findByRole("region", { name: NEEDS }, SLOW)).toBeInTheDocument();
  });

  it("shows what is running, what is ready and the whole history", async () => {
    const { user } = setup();
    await screen.findByRole("region", { name: NEEDS }, SLOW);

    await user.click(screen.getByRole("radio", { name: /^En marcha/ }));
    const running = screen.getByRole("region", { name: "En marcha (1)" });
    expect(within(running).getByText("Revisa los tests del carrito")).toBeInTheDocument();
    expect(within(running).getByText("Trabajando")).toBeInTheDocument();
    // What it is doing, as a word next to the state (decorative).
    const word = await within(running).findByText("Rastreando");
    expect(word.closest(".working-word")).toHaveAttribute("aria-hidden");
    expect(within(running).getByText("tienda")).toBeInTheDocument();
    expect(within(running).getByText("2 en cola")).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: NEEDS })).not.toBeInTheDocument();

    await user.click(screen.getByRole("radio", { name: /^Listas/ }));
    const ready = screen.getByRole("region", { name: "Listas (3)" });
    expect(within(ready).getByRole("button", { name: "Revisar cambios" })).toBeInTheDocument();

    await user.click(screen.getByRole("radio", { name: /^Historial/ }));
    const all = await screen.findByRole("region", { name: "Historial" });
    await waitFor(() => expect(within(all).getAllByRole("listitem")).toHaveLength(9));
    expect(titles(all)).toEqual([
      "Revisa los tests del carrito",
      "Agrega recordatorios por WhatsApp antes de cada cita",
      "Migra el reporte de comisiones a la API nueva",
      "Traduce los correos de confirmación",
      "¿Qué diferencia hay entre OAuth y OpenID Connect?",
      "Arregla el cálculo de impuestos en las facturas",
      "¿Dónde se calculan las comisiones?",
      "Actualiza las dependencias del panel",
      "Cambia el color del botón de reservar",
    ]);
    // Each row's state is the task's one pill.
    expect(within(all).getByText("Falló")).toHaveClass("status-pill--failed");
    expect(within(all).getByText("Descartada")).toHaveClass("status-pill--closed");
    expect(within(all).getByText("Sin proyecto")).toBeInTheDocument();
  });

  it("narrows the board to a project or to what was searched", async () => {
    const { user } = setup();
    await screen.findByRole("region", { name: NEEDS }, SLOW);
    // The palette's field is a combobox too: this is Inicio's own.
    await user.selectOptions(screen.getByRole("combobox", { name: "Proyecto" }), "/code/tienda");
    const needs = screen.getByRole("region", { name: "Te necesitan (1)" });
    expect(titles(needs)).toEqual(["Migra el reporte de comisiones a la API nueva"]);
    expect(screen.queryByRole("region", { name: /^Listas para revisar/ })).not.toBeInTheDocument();

    await user.selectOptions(screen.getByRole("combobox", { name: "Proyecto" }), "");
    await user.type(screen.getByRole("searchbox", { name: "Buscar tareas y pendientes" }), "zod");
    expect(titles(screen.getByRole("region", { name: "Te necesitan (1)" }))).toEqual(["Los formularios validan con zod"]);

    await user.clear(screen.getByRole("searchbox", { name: "Buscar tareas y pendientes" }));
    await user.type(screen.getByRole("searchbox", { name: "Buscar tareas y pendientes" }), "nada de esto");
    expect(screen.getByText("Nada te espera")).toBeInTheDocument();
  });

  it("opens a task in its project's workspace", async () => {
    const { user } = setup();
    const needs = await screen.findByRole("region", { name: NEEDS }, SLOW);
    await user.click(within(needs).getByRole("button", { name: /Migra el reporte de comisiones/ }));
    expect(await screen.findByRole("heading", { name: "Migra el reporte de comisiones a la API nueva" })).toBeInTheDocument();
    expect(within(screen.getByRole("complementary", { name: "Navegación" })).getByRole("button", { name: /^tienda/ })).toHaveAttribute("aria-current", "page");
    expect(window.location.hash).toBe(`#p=${encodeURIComponent("/code/tienda")}&t=s2`);
  });

  it("answers a permission from its row, once or no, and never for the whole task", async () => {
    const { api, user } = setup();
    const answer = vi.spyOn(api, "answerPermission");
    const decide = await screen.findByRole("group", { name: "Responder el permiso de «Migra el reporte de comisiones a la API nueva»" }, SLOW);
    expect(within(decide).getByText("Bash: npm test")).toBeInTheDocument();
    expect(within(decide).queryByRole("button", { name: /en este chat/ })).not.toBeInTheDocument();
    await user.click(within(decide).getByRole("button", { name: "Rechazar" }));
    expect(answer).toHaveBeenCalledWith("s2", "req-s2", "deny");
    // Still on the board: answering did not open the task.
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("¡Hola, Alex!");
    expect(window.location.hash).not.toMatch(/^#p=/);
  });

  it("allows a permission once from its row", async () => {
    const { api, user } = setup();
    const answer = vi.spyOn(api, "answerPermission");
    const decide = await screen.findByRole("group", { name: /^Responder el permiso de/ }, SLOW);
    await user.click(within(decide).getByRole("button", { name: "Permitir una vez" }));
    expect(answer).toHaveBeenCalledWith("s2", "req-s2", "allow");
    // Answered, the task leaves what needs you.
    expect(await screen.findByRole("region", { name: "Te necesitan (9)" }, SLOW)).toBeInTheDocument();
  });

  it("approves a plan right on its row, and shows the plan on the selected row", async () => {
    const { api, user } = setup();
    const answer = vi.spyOn(api, "answerPlan");
    const group = await screen.findByRole("group", { name: "Plan de Agrega recordatorios por WhatsApp antes de cada cita" }, SLOW);
    const row = group.closest("li")!;
    expect(row.querySelector(".board-item__plan")).toBeNull();
    // `j` selects the first row: its plan shows, to read before approving.
    await user.keyboard("j");
    expect(selectedTitle()).toBe("Agrega recordatorios por WhatsApp antes de cada cita");
    expect(row.querySelector(".board-item__plan")).toHaveTextContent("1. Buscar 2. Programar 3. Tests");
    await user.click(within(group).getByRole("button", { name: "Aprobar plan" }));
    expect(answer).toHaveBeenCalledWith("s1", "plan-s1", true, null);
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("¡Hola, Alex!");
    expect(await screen.findByRole("region", { name: "Te necesitan (9)" }, SLOW)).toBeInTheDocument();
  });

  it("approves a plan with `a`, and asks for changes in the task", async () => {
    const { api, user } = setup();
    const answer = vi.spyOn(api, "answerPlan");
    const group = await screen.findByRole("group", { name: /^Plan de Agrega recordatorios/ }, SLOW);
    // Changes are asked in the task, where the plan can be read and answered in full.
    await user.click(within(group).getByRole("button", { name: "Pedir cambios" }));
    expect(await screen.findByRole("heading", { name: "Agrega recordatorios por WhatsApp antes de cada cita" })).toBeInTheDocument();
    expect(answer).not.toHaveBeenCalled();

    await user.click(screen.getByRole("button", { name: /^Inicio/ }));
    await screen.findByRole("group", { name: /^Plan de Agrega recordatorios/ }, SLOW);
    // The focus is still on the sidebar's Inicio: there, the board's keys are not the board's.
    expect(screen.getByRole("button", { name: /^Inicio/ })).toHaveFocus();
    await user.keyboard("ja");
    expect(selectedTitle()).toBeNull();
    expect(answer).not.toHaveBeenCalled();
    (document.activeElement as HTMLElement).blur();
    await user.keyboard("ja");
    expect(answer).toHaveBeenCalledWith("s1", "plan-s1", true, null);
  });

  it("leaves the board's keys to the sidebar while it has the focus", async () => {
    const { api, user } = setup();
    const approve = vi.spyOn(api, "answerPlan");
    await screen.findByRole("group", { name: /^Plan de Agrega recordatorios/ }, SLOW);
    const sidebar = screen.getByRole("complementary", { name: "Navegación" });
    within(sidebar).getByRole("button", { name: /^tienda/ }).focus();
    await user.keyboard("j{ArrowDown}a{Enter}");
    // Enter on the sidebar's row is that row's: the project opens, nothing on the board moved.
    expect(approve).not.toHaveBeenCalled();
    expect(await screen.findByRole("heading", { level: 1, name: "tienda" })).toBeInTheDocument();
    expect(selectedTitle()).toBeNull();
  });

  it("waits for a limit or hands the task to another provider from its row, never by itself", async () => {
    const api = MockApi.withSampleData().withSampleActivity();
    await api.cancelWait("s3");
    const wait = vi.spyOn(api, "waitForLimit");
    const handoff = vi.spyOn(api, "handoffTask");
    const { user } = setup(api);
    const group = await screen.findByRole("group", { name: "Límite de Traduce los correos de confirmación" }, SLOW);
    expect(await within(group).findByRole("button", { name: "Pasar a ChatGPT" })).toBeInTheDocument();
    expect(wait).not.toHaveBeenCalled();
    expect(handoff).not.toHaveBeenCalled();

    await user.click(within(group).getByRole("button", { name: "Esperar" }));
    expect(wait).toHaveBeenCalledWith("s3");
    // Waiting, the row counts down to the reset and "Esperar" goes away.
    expect(await screen.findByText(/^sigue en 4\d min$/, undefined, SLOW)).toBeInTheDocument();
    await waitFor(() => expect(within(group).queryByRole("button", { name: "Esperar" })).not.toBeInTheDocument());

    await user.click(within(group).getByRole("button", { name: "Pasar a ChatGPT" }));
    expect(handoff).toHaveBeenCalledWith("s3", "chatgpt", null);
  });

  it("stops waiting for a limit from its row, and continues once the limit renewed", async () => {
    const api = MockApi.withSampleData().withSampleActivity();
    const cancel = vi.spyOn(api, "cancelWait");
    const resume = vi.spyOn(api, "resumeAfterLimit");
    const { user } = setup(api);
    // The sample task is already waiting for its reset: it can stop waiting, not yet continue.
    const group = await screen.findByRole("group", { name: "Límite de Traduce los correos de confirmación" }, SLOW);
    expect(within(group).getByRole("button", { name: "Cancelar espera" })).toBeInTheDocument();
    expect(within(group).queryByRole("button", { name: "Esperar" })).not.toBeInTheDocument();
    expect(within(group).queryByRole("button", { name: "Continuar" })).not.toBeInTheDocument();

    await user.click(within(group).getByRole("button", { name: "Cancelar espera" }));
    expect(cancel).toHaveBeenCalledWith("s3");
    expect(await within(group).findByRole("button", { name: "Esperar" })).toBeInTheDocument();
    expect(within(group).queryByRole("button", { name: "Cancelar espera" })).not.toBeInTheDocument();

    // Waiting again, and the reset passed while Hedy was closed: the user continues.
    await user.click(within(group).getByRole("button", { name: "Esperar" }));
    await within(group).findByRole("button", { name: "Cancelar espera" });
    act(() => api.simulateRestartAfterReset("s3"));
    const go = await within(group).findByRole("button", { name: "Continuar" });
    expect(within(group).getByRole("button", { name: "Cancelar espera" })).toBeInTheDocument();
    expect(resume).not.toHaveBeenCalled();
    await user.click(go);
    expect(resume).toHaveBeenCalledWith("s3");
  });

  it("shows a sync that keeps failing as something to look at", async () => {
    const api = MockApi.withSampleData().withSampleActivity();
    api.syncConflictList = [];
    api.sync!.lastError = "no pude conectar con github.com";
    // A day and more without a sync that worked: now it asks for the user.
    api.sync!.lastSync = Date.now() / 1000 - 26 * 60 * 60;
    const { user } = setup(api);
    const needs = await screen.findByRole("region", { name: NEEDS }, SLOW);
    const row = within(needs).getByText("La sincronización falla desde hace un día").closest("li")!;
    expect(within(row).getByText("Sincronización")).toBeInTheDocument();
    // Its own words, not the conflicts' text nor the raw error.
    expect(within(row).getByText(/Tus otras computadoras no reciben los cambios/)).toBeInTheDocument();
    expect(within(row).queryByText("no pude conectar con github.com")).not.toBeInTheDocument();
    expect(within(row).queryByRole("button", { name: "Resolver" })).not.toBeInTheDocument();

    // The palette lists it among what needs you, with the same words.
    await user.keyboard("{Control>}k{/Control}");
    const palette = screen.getByRole("dialog", { name: "Paleta de comandos" });
    expect(within(palette).getByRole("option", { name: /La sincronización falla desde hace un día/ })).toBeInTheDocument();
    await user.keyboard("{Escape}");

    await user.click(within(row).getByRole("button", { name: "Ver qué pasó" }));
    await waitFor(() => expect(window.location.hash).toBe("#sync"));
  });

  it("leaves a single failed sync alone", async () => {
    const api = MockApi.withSampleData().withSampleActivity();
    api.syncConflictList = [];
    api.sync!.lastError = "no pude conectar con github.com";
    api.sync!.lastSync = Date.now() / 1000 - 10 * 60;
    setup(api);
    const needs = await screen.findByRole("region", { name: /^Te necesitan/ }, SLOW);
    expect(within(needs).queryByText(/La sincronización falla/)).not.toBeInTheDocument();
  });

  it("approves and rejects proposed notes and skills from their rows", async () => {
    const { api, user } = setup();
    const approve = vi.spyOn(api, "approveNote");
    const reject = vi.spyOn(api, "rejectSkill");
    const zod = await screen.findByRole("group", { name: "Decidir sobre «Los formularios validan con zod»" }, SLOW);
    await user.click(within(zod).getByRole("button", { name: "Aprobar" }));
    expect(approve).toHaveBeenCalledTimes(1);
    expect(await screen.findByRole("region", { name: "Te necesitan (9)" })).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("button", { name: "Inicio: 9 cosas te esperan" })).toBeInTheDocument());

    const skill = screen.getByRole("group", { name: /^Decidir sobre «agregar-endpoint/ });
    await user.click(within(skill).getByRole("button", { name: "Rechazar" }));
    expect(reject).toHaveBeenCalledTimes(1);
    expect(await screen.findByRole("region", { name: "Te necesitan (8)" })).toBeInTheDocument();
    // A change to an approved note says so on its button.
    const change = screen.getByRole("group", { name: /^Decidir sobre «Las citas usan la zona horaria/ });
    expect(within(change).getByRole("button", { name: "Aprobar cambio" })).toBeInTheDocument();
  });

  it("sends a note like an approved one to the memory to decide there", async () => {
    const { api, user } = setup();
    const approve = vi.spyOn(api, "approveNote");
    const like = await screen.findByRole("group", { name: "Decidir sobre «Los precios se guardan en USD»" }, SLOW);
    expect(within(like.closest("li")!).getByText("se parece a una nota aprobada")).toBeInTheDocument();
    expect(within(like).queryByRole("button", { name: "Aprobar" })).not.toBeInTheDocument();
    expect(within(like).queryByRole("button", { name: "Rechazar" })).not.toBeInTheDocument();
    await user.click(within(like).getByRole("button", { name: "Ver y decidir" }));
    expect(await screen.findByRole("heading", { name: "Memoria de hedy" })).toBeInTheDocument();
    expect(window.location.hash).toBe(`#m=${encodeURIComponent("/code/hedy")}`);
    expect(approve).not.toHaveBeenCalled();
  });

  it("sends a team project's notes to the memory to decide there, and keeps its skills on the row", async () => {
    const api = MockApi.withSampleData().withSampleActivity();
    await api.teamLinkProject("team-1", "/code/hedy");
    setup(api);
    const zod = await screen.findByRole("group", { name: "Decidir sobre «Los formularios validan con zod»" }, SLOW);
    // Who a team's note is for is asked in the memory, never assumed here.
    expect(await within(zod).findByRole("button", { name: "Ver y decidir" })).toBeInTheDocument();
    expect(within(zod).queryByRole("button", { name: "Aprobar" })).not.toBeInTheDocument();
    const skill = screen.getByRole("group", { name: /^Decidir sobre «agregar-endpoint/ });
    expect(within(skill).getByRole("button", { name: "Aprobar" })).toBeInTheDocument();
  });

  it("takes sync conflicts and team suggestions to where they are resolved", async () => {
    const { user } = setup();
    const needs = await screen.findByRole("region", { name: NEEDS }, SLOW);
    const sync = within(needs).getByText("Un cambio en tus computadoras para resolver").closest("li")!;
    await user.click(within(sync).getByRole("button", { name: "Resolver" }));
    await waitFor(() => expect(window.location.hash).toBe("#sync"));

    await user.click(screen.getByRole("button", { name: /^Inicio/ }));
    const again = await screen.findByRole("region", { name: NEEDS }, SLOW);
    const team = within(again).getByText("2 sugerencias del equipo Plataforma").closest("li")!;
    await user.click(within(team).getByRole("button", { name: "Revisar" }));
    await waitFor(() => expect(window.location.hash).toBe("#teams"));
  });

  it("tells the waiting kinds apart by color", async () => {
    setup();
    const needs = await screen.findByRole("region", { name: NEEDS }, SLOW);
    expect(within(needs).getByText("Te espera: plan")).toHaveClass("board-state--needs");
    expect(within(needs).getByText("Te espera: permiso")).toHaveClass("board-state--needs");
    expect(within(needs).getByText(/^En pausa/)).toHaveClass("board-state--blocked");
    expect(within(needs).getByText("Sincronización")).toHaveClass("board-state--needs");
    expect(within(needs).getByText("Equipo")).toHaveClass("board-state--needs");
    expect(within(needs).getByText("Los formularios validan con zod").closest(".board-row")).toHaveClass("board-row--proposal");
    const ready = screen.getByRole("region", { name: "Listas para revisar (3)" });
    expect(within(ready).getAllByText("Lista")[0]).toHaveClass("board-state--done");
  });

  it("moves through the rows with j/k and the arrows, opens with Enter and decides with a and r", async () => {
    const { api, user } = setup();
    const answer = vi.spyOn(api, "answerPermission");
    const approve = vi.spyOn(api, "approveNote");
    const reject = vi.spyOn(api, "rejectSkill");
    const wait = vi.spyOn(api, "waitForLimit");
    const handoff = vi.spyOn(api, "handoffTask");
    const needs = await screen.findByRole("region", { name: NEEDS }, SLOW);
    await screen.findByRole("group", { name: /^Responder el permiso de/ }, SLOW);
    expect(screen.getByText("para moverte")).toBeInTheDocument();
    const rows = () => within(needs).getAllByRole("listitem");

    // The first key selects the first row and gives it the focus.
    await user.keyboard("j");
    expect(selectedTitle()).toBe("Agrega recordatorios por WhatsApp antes de cada cita");
    expect(rows()[0]!.querySelector(".board-row")).toHaveFocus();

    await user.keyboard("{ArrowDown}");
    expect(selectedTitle()).toBe("Migra el reporte de comisiones a la API nueva");
    await user.keyboard("a");
    expect(answer).toHaveBeenCalledWith("s2", "req-s2", "allow");
    // Answered, the task leaves the list and the row now in its place is selected.
    await waitFor(() => expect(selectedTitle()).toBe("Traduce los correos de confirmación"));
    // Already waiting for its limit: `a` has nothing to do, and Hedy never moves it by itself.
    await user.keyboard("a");
    expect(wait).not.toHaveBeenCalled();
    expect(handoff).not.toHaveBeenCalled();

    // Down to the first note, approved with `a`; again, the next row takes over.
    await user.keyboard("jjjjj");
    expect(selectedTitle()).toBe("Los formularios validan con zod");
    await user.keyboard("a");
    expect(approve).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(selectedTitle()).toBe("Las citas usan la zona horaria del salón; el servidor guarda las fechas en UTC"));

    // Up to a skill, rejected with `r`.
    await user.keyboard("kk");
    expect(selectedTitle()).toMatch(/^agregar-endpoint/);
    await user.keyboard("r");
    expect(reject).toHaveBeenCalledTimes(1);

    // Enter opens the selected task.
    await waitFor(() => expect(selectedTitle()).not.toBeNull());
    await user.keyboard("kkkkkkkk");
    expect(selectedTitle()).toBe("Agrega recordatorios por WhatsApp antes de cada cita");
    (document.activeElement as HTMLElement).blur();
    await user.keyboard("{Enter}");
    expect(await screen.findByRole("heading", { name: "Agrega recordatorios por WhatsApp antes de cada cita" })).toBeInTheDocument();
  });

  it("leaves the board's keys alone while typing or with a dialog open", async () => {
    const { user } = setup();
    await screen.findByRole("region", { name: NEEDS }, SLOW);
    await user.type(screen.getByRole("searchbox", { name: "Buscar tareas y pendientes" }), "j");
    expect(document.querySelector(".board-entry--selected")).toBeNull();
    await user.clear(screen.getByRole("searchbox", { name: "Buscar tareas y pendientes" }));
    (document.activeElement as HTMLElement).blur();

    await user.keyboard("{Control>}k{/Control}");
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    await user.keyboard("j");
    expect(document.querySelector(".board-entry--selected")).toBeNull();
  });

  it("leaves the board's keys alone on the filters", async () => {
    const { user } = setup();
    await screen.findByRole("region", { name: NEEDS }, SLOW);
    screen.getByRole("radio", { name: /^Listas/ }).focus();
    await user.keyboard("jja");
    expect(document.querySelector(".board-entry--selected")).toBeNull();
    expect(screen.getByRole("region", { name: NEEDS })).toBeInTheDocument();
  });

  it("opens a proposal in its project's memory", async () => {
    const { user } = setup();
    const needs = await screen.findByRole("region", { name: NEEDS }, SLOW);
    await user.click(within(needs).getByRole("button", { name: /^agregar-endpoint/ }));
    expect(await screen.findByRole("heading", { name: "Memoria de hedy" })).toBeInTheDocument();
    expect(window.location.hash).toBe(`#k=${encodeURIComponent("/code/hedy")}`);
  });

  it("asks again for a plan it could not read, for that account only", async () => {
    const { api, user } = setup();
    const refresh = vi.spyOn(api, "refreshPlanUsage");
    const plans = await screen.findByRole("region", { name: "Tus planes" }, SLOW);
    expect(within(plans).getByRole("button", { name: /^ChatGPT/ })).toHaveTextContent("Sin datos");
    const retry = within(plans).getByRole("button", { name: "Volver a consultar el plan de ChatGPT" });
    expect(retry).toHaveTextContent("Reintentar");
    // Only the account without data offers it.
    expect(within(plans).getAllByRole("button", { name: /^Volver a consultar/ })).toHaveLength(1);
    await user.click(retry);
    expect(refresh).toHaveBeenCalledWith("chatgpt");
  });

  it("is where Actividad's old address lands", async () => {
    history.replaceState(null, "", "#activity");
    setup();
    expect(await screen.findByRole("region", { name: NEEDS }, SLOW)).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("¡Hola, Alex!");
  });

  it("opens the task of a clicked desktop notification", async () => {
    const { api } = setup();
    expect(await screen.findByRole("heading", { name: /¡Hola, Alex!/ })).toBeInTheDocument();
    api.simulateNotificationClick("s3");
    expect(await screen.findByRole("heading", { name: "Traduce los correos de confirmación" })).toBeInTheDocument();
  });

  it("starts empty and follows a task live", async () => {
    const api = MockApi.withSampleData();
    // Nothing pending: no proposals, no sync conflicts, no team.
    for (const proposal of (await api.memory("/code/hedy")).proposed) await api.forgetNote(proposal.note.id);
    for (const proposal of (await api.skills("/code/hedy")).proposed) await api.rejectSkill(proposal.skill.id);
    api.syncConflictList = [];
    api.teamList = [];
    const { user } = setup(api);
    expect(await screen.findByText("Nada te espera", undefined, SLOW)).toBeInTheDocument();
    expect(screen.getByText("Cuando una IA necesite algo de ti, aparecerá aquí.")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "Durmiendo" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Inicio" })).toBeInTheDocument();
    await user.click(screen.getByRole("radio", { name: /^En marcha/ }));
    expect(screen.getByText("Ninguna IA está trabajando ahora")).toBeInTheDocument();
    await user.click(screen.getByRole("radio", { name: /^Te necesitan/ }));

    await api.startTask({
      project: "/code/hedy",
      prompt: "Agrega un recordatorio",
      mode: "project",
      model: null,
      planFirst: true,
    });
    // The plan arrives: the task moves to "needs you" and the badge counts it.
    const needs = await screen.findByRole("region", { name: "Te necesitan (1)" }, SLOW);
    expect(within(needs).getByText("Agrega un recordatorio")).toBeInTheDocument();
    expect(within(needs).getByText("Te espera: plan")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("button", { name: "Inicio: 1 cosa te espera" })).toBeInTheDocument());
  });
});
