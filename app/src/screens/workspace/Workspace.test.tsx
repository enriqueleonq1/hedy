import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";

// xterm needs a real canvas; the Terminal tab is not exercised here.
vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

function setup() {
  const api = MockApi.withSampleData();
  api.delayMs = 0;
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<App api={api} queryClient={queryClient} />);
  return { api, user: userEvent.setup() };
}

async function openProject(user: ReturnType<typeof userEvent.setup>, name: RegExp) {
  await user.click(await screen.findByRole("button", { name }));
}

describe("Workspace", () => {
  it("runs a task end to end: plan, permission, tests, diff and accept", async () => {
    const { user } = setup();
    await openProject(user, /^hedy/);
    expect(await screen.findByText(/¿Qué hacemos en hedy\?/)).toBeInTheDocument();
    expect(screen.getByRole("radiogroup", { name: "Modo" })).toBeInTheDocument();

    await user.type(screen.getByLabelText("Pídele algo a hedy"), "Agrega un recordatorio 24 horas antes");
    await user.click(screen.getByRole("button", { name: "Enviar" }));

    // Plan first: nothing changes until it is approved.
    const plan = await screen.findByRole("region", { name: "Plan de Claude Opus" });
    expect(within(plan).getByText(/Buscar dónde se crean las citas/)).toBeInTheDocument();
    await user.click(within(plan).getByRole("button", { name: "Aprobar plan" }));

    const card = await screen.findByRole("region", { name: "Claude quiere correr un comando" });
    expect(within(card).getByText("pytest tests/test_recordatorios.py")).toBeInTheDocument();
    expect(screen.getByText("Claude editó citas/servicio.py")).toBeInTheDocument();
    await user.click(within(card).getByRole("button", { name: "Permitir en este chat" }));

    expect(await screen.findByText(/Claude: pasan los tests/)).toBeInTheDocument();
    expect(await screen.findByText(/Terminó/, { selector: ".activity" })).toBeInTheDocument();
    expect(screen.getByText(/Permitido en este chat/)).toBeInTheDocument();
    const steps = screen.getByRole("region", { name: "Pasos" });
    expect(within(steps).getAllByText(/\(hecho\)/)).toHaveLength(3);

    const files = await screen.findByRole("list", { name: "Archivos cambiados" });
    expect(within(files).getByText("citas/servicio.py")).toBeInTheDocument();
    expect(screen.getByRole("table", { name: "Cambios en citas/servicio.py" })).toHaveTextContent(
      "programar_recordatorio(cita, horas=24)",
    );

    await user.click(screen.getByRole("button", { name: "Aceptar cambios" }));
    expect(await screen.findByText("Cambios unidos a main.")).toBeInTheDocument();
    expect(await screen.findByText(/ya se aceptó/)).toBeInTheDocument();
    // Many steps: well over the default 5 s on a busy machine.
  }, 20_000);

  it("shows what the agent is doing: the working mascot and its word", async () => {
    const api = MockApi.withSampleData().withSampleActivity();
    api.delayMs = 0;
    render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
    const user = userEvent.setup();
    // The running sample task in homming last searched the code (Grep).
    await openProject(user, /^homming/);
    const word = await screen.findByText("Rastreando");
    const live = word.closest(".agent-row--live");
    expect(live).toHaveAttribute("aria-hidden");
    expect(live?.querySelector(".mascot")).toHaveAttribute("data-activity", "search");
    expect(live?.querySelector(".m-lens")).not.toBeNull();
    // A single working mascot in the chat: the others rest.
    expect(document.querySelectorAll('.timeline .mascot[data-state="working"]')).toHaveLength(1);

    await user.click(screen.getByRole("button", { name: "Estado" }));
    expect(await screen.findByRole("heading", { name: "homming" })).toBeInTheDocument();
    await waitFor(() => expect(screen.getAllByText("Rastreando").length).toBeGreaterThanOrEqual(2));
  });

  it("sends the plan back with feedback", async () => {
    const { user } = setup();
    await openProject(user, /^hedy/);
    await user.type(await screen.findByLabelText("Pídele algo a hedy"), "Recordatorios");
    await user.click(screen.getByRole("button", { name: "Enviar" }));

    const plan = await screen.findByRole("region", { name: "Plan de Claude Opus" });
    await user.click(within(plan).getByRole("button", { name: "Pedir cambios" }));
    await user.type(within(plan).getByLabelText("¿Qué cambiarías?"), "usa Celery");
    await user.click(within(plan).getByRole("button", { name: "Enviar cambios" }));

    await waitFor(() => expect(screen.getAllByRole("region", { name: "Plan de Claude Opus" })).toHaveLength(2));
    expect(screen.getByText(/con cambios pedidos/)).toBeInTheDocument();
    expect(screen.getByText(/usa Celery/)).toBeInTheDocument();
  });

  it("asks without a worktree in Pregunta mode", async () => {
    const { user } = setup();
    await openProject(user, /^hedy/);
    await user.click(await screen.findByRole("radio", { name: "Pregunta" }));
    expect(screen.queryByLabelText("Planificar primero")).not.toBeInTheDocument();
    await user.type(screen.getByLabelText("Pídele algo a hedy"), "¿Dónde se crean las citas?");
    await user.click(screen.getByRole("button", { name: "Enviar" }));
    expect(await screen.findByText(/no cambian archivos/)).toBeInTheDocument();
    expect(await screen.findByText(/Terminó/, { selector: ".activity" })).toBeInTheDocument();
  });

  it("picks the mode in the composer, only for new tasks, remembered per project", async () => {
    const { user } = setup();
    await openProject(user, /^hedy/);
    const modes = await screen.findByRole("radiogroup", { name: "Modo" });
    expect(modes.closest(".composer")).not.toBeNull();
    expect(document.querySelector(".topbar [role=radiogroup]")).toBeNull();
    expect(within(modes).getByRole("radio", { name: "Investigar" })).toBeInTheDocument();
    expect(screen.getByLabelText("Planificar primero")).toBeInTheDocument();

    await user.click(within(modes).getByRole("radio", { name: "Investigar" }));
    expect(screen.queryByLabelText("Planificar primero")).not.toBeInTheDocument();
    expect(localStorage.getItem("hedy.mode./code/hedy")).toBe("research");

    // Another project keeps its own mode; coming back finds the one chosen here.
    await openProject(user, /^homming/);
    expect(await screen.findByRole("radio", { name: "Proyecto" })).toHaveAttribute("aria-checked", "true");
    await user.click(within(screen.getByRole("navigation", { name: "Proyectos abiertos" })).getByRole("button", { name: "hedy" }));
    expect(await screen.findByRole("radio", { name: "Investigar" })).toHaveAttribute("aria-checked", "true");

    // Inside a task, follow-ups keep its mode: no selector.
    await user.type(screen.getByLabelText("Pídele algo a hedy"), "¿Qué librerías hay para recordatorios?");
    await user.click(screen.getByRole("button", { name: "Enviar" }));
    expect(await screen.findByLabelText("Responde o pide algo más")).toBeInTheDocument();
    expect(screen.queryByRole("radiogroup", { name: "Modo" })).not.toBeInTheDocument();
  });

  it("answers the plan and permission cards with the keyboard", async () => {
    const { user } = setup();
    await openProject(user, /^hedy/);
    await user.type(await screen.findByLabelText("Pídele algo a hedy"), "Recordatorios");
    expect(screen.getByText("Ctrl+Enter")).toHaveClass("kbd");
    await user.keyboard("{Control>}{Enter}{/Control}");

    // The card takes the focus (nothing else had it) and shows its keys.
    const plan = await screen.findByRole("region", { name: "Plan de Claude Opus" });
    await waitFor(() => expect(plan).toHaveFocus());
    const approve = within(plan).getByRole("button", { name: "Aprobar plan" });
    expect(approve).toHaveAttribute("aria-keyshortcuts", "Enter");
    expect(within(approve).getByText("Enter")).toHaveClass("kbd");
    expect(within(within(plan).getByRole("button", { name: "Pedir cambios" })).getByText("E")).toHaveClass("kbd");
    // Meanwhile the composer says what a message does now: it waits for the answer.
    expect(screen.getByLabelText(/^Primero responde al plan de arriba/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Enviar después" })).toBeDisabled();

    await user.keyboard("e");
    const feedback = within(plan).getByLabelText("¿Qué cambiarías?");
    expect(feedback).toHaveFocus();
    expect(feedback).toHaveValue("");
    await user.click(within(plan).getByRole("button", { name: "Cancelar" }));
    expect(plan).toHaveFocus();

    await user.keyboard("{Enter}");
    const card = await screen.findByRole("region", { name: "Claude quiere correr un comando" });
    expect(screen.getByText(/aprobado/)).toBeInTheDocument();
    await waitFor(() => expect(card).toHaveFocus());
    await waitFor(() => expect(document.querySelector(".chat__announce")).toHaveTextContent("Pide permiso"));
    await user.keyboard("{Enter}");
    expect(await screen.findByText(/Claude: pasan los tests/)).toBeInTheDocument();
    await waitFor(() => expect(document.querySelector(".chat__announce")).toHaveTextContent("Terminó"));
    expect(document.querySelector(".timeline")).not.toHaveAttribute("aria-live");
  }, 20_000);

  it("says why the Changes panel is empty, case by case", async () => {
    const api = MockApi.withSampleData().withSampleActivity();
    api.delayMs = 0;
    render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
    const user = userEvent.setup();
    await openProject(user, /^hedy/);
    const panel = await screen.findByRole("complementary", { name: "Cambios, terminal y vista previa" });
    // A project task whose plan waits (s1), one that works (s3), one done (s5), a question (s6).
    expect(await within(panel).findByText("Aún no hay cambios: se crearán al aprobar el plan.")).toBeInTheDocument();
    await user.keyboard("{Control>}2{/Control}");
    expect(await within(panel).findByText("Aún no hay cambios: se crearán cuando la IA empiece a trabajar.")).toBeInTheDocument();
    await user.keyboard("{Control>}3{/Control}");
    expect(await within(panel).findByText("Esta tarea terminó sin cambiar archivos.")).toBeInTheDocument();
    await user.keyboard("{Control>}4{/Control}");
    expect(await within(panel).findByText("Las preguntas y la investigación no cambian archivos.")).toBeInTheDocument();
    expect(within(panel).queryByText(/Aún no hay cambios/)).not.toBeInTheDocument();
  });

  it("does not take the focus from the composer when a card appears", async () => {
    const api = MockApi.withSampleData().withSampleActivity();
    api.delayMs = 0;
    render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
    const user = userEvent.setup();
    await openProject(user, /^homming/);
    const composer = await screen.findByLabelText(/^Escribe mientras Claude Opus trabaja/);
    await user.type(composer, "Una cosa más");

    api.simulateEvent("s4", { kind: "permissionRequest", requestId: "req-s4", tool: "Bash", summary: "npm test", input: { command: "npm test" } });
    const card = await screen.findByRole("region", { name: "Claude quiere correr un comando" });
    expect(composer).toHaveFocus();
    expect(card).not.toHaveFocus();
    expect(await screen.findByLabelText(/^Primero responde al permiso de arriba/)).toBe(composer);
    expect(screen.getByRole("button", { name: "Enviar después" })).toBeEnabled();
    await waitFor(() => expect(document.querySelector(".chat__announce")).toHaveTextContent("Pide permiso"));
  });

  it("switches tasks from a list with their state, account and time, or with Ctrl+digit", async () => {
    const api = MockApi.withSampleData().withSampleActivity();
    api.delayMs = 0;
    render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
    const user = userEvent.setup();
    await openProject(user, /^hedy/);
    expect(await screen.findByRole("heading", { name: "Agrega recordatorios por WhatsApp antes de cada cita" })).toBeInTheDocument();
    expect(screen.queryByRole("combobox", { name: "Tarea" })).not.toBeInTheDocument();

    const toggle = screen.getByRole("button", { name: "Tareas (5)" });
    await user.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    const list = screen.getByRole("group", { name: "Tareas de este proyecto" });
    const current = within(list).getByRole("button", { name: /Agrega recordatorios por WhatsApp/ });
    expect(current).toHaveAttribute("aria-current", "true");
    expect(current).toHaveFocus();
    expect(within(current).getByText("Plan por aprobar")).toHaveClass("board-state--needs");
    expect(within(current).getByText("Claude personal")).toBeInTheDocument();
    expect(within(current).getByText("Ctrl+1")).toBeInTheDocument();
    const taxes = within(list).getByRole("button", { name: /Arregla el cálculo de impuestos/ });
    expect(within(taxes).getByText("Cambios por revisar")).toHaveClass("board-state--done");
    expect(within(taxes).getByText("hace 40 min")).toBeInTheDocument();

    // Escape closes it and gives the focus back.
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("group", { name: "Tareas de este proyecto" })).not.toBeInTheDocument();
    expect(toggle).toHaveFocus();

    await user.click(toggle);
    await user.click(within(screen.getByRole("group", { name: "Tareas de este proyecto" })).getByRole("button", { name: /Arregla el cálculo/ }));
    const taxesTitle = "Arregla el cálculo de impuestos en las facturas";
    expect(await screen.findByRole("heading", { name: taxesTitle })).toHaveAttribute("title", taxesTitle);
    expect(window.location.hash).toBe(`#p=${encodeURIComponent("/code/hedy")}&t=s5`);

    // Ctrl+2: the second task of the list; Back returns to the previous one.
    await user.keyboard("{Control>}2{/Control}");
    expect(await screen.findByRole("heading", { name: "Traduce los correos de confirmación" })).toBeInTheDocument();
    history.back();
    expect(await screen.findByRole("heading", { name: taxesTitle })).toBeInTheDocument();

    await user.click(toggle);
    await user.click(screen.getByRole("button", { name: "+ Nueva tarea" }));
    expect(await screen.findByText(/¿Qué hacemos en hedy\?/)).toBeInTheDocument();
    expect(window.location.hash).toBe(`#p=${encodeURIComponent("/code/hedy")}&new`);
  });

  it("goes Back and Forward between screens and tasks", async () => {
    const api = MockApi.withSampleData().withSampleActivity();
    api.delayMs = 0;
    render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: /^Actividad/ }, { timeout: 8000 }));
    const needs = await screen.findByRole("region", { name: "Te necesitan (3)" });
    await user.click(within(needs).getByRole("button", { name: /Migra el reporte de comisiones/ }));
    const title = "Migra el reporte de comisiones a la API nueva";
    expect(await screen.findByRole("heading", { name: title })).toBeInTheDocument();

    history.back();
    expect(await screen.findByRole("region", { name: "Te necesitan (3)" })).toBeInTheDocument();
    expect(window.location.hash).toBe("#activity");
    history.back();
    expect(await screen.findByRole("heading", { name: /¡Hola, Enrique!/ })).toBeInTheDocument();
    history.forward();
    expect(await screen.findByRole("region", { name: "Te necesitan (3)" })).toBeInTheDocument();
    history.forward();
    expect(await screen.findByRole("heading", { name: title })).toBeInTheDocument();
    expect(window.location.hash).toBe(`#p=${encodeURIComponent("/code/homming")}&t=s2`);
  });

  it("refreshes a stopped task even when its status stays the same", async () => {
    const { user } = setup();
    await openProject(user, /^hedy/);
    await user.click(await screen.findByRole("radio", { name: "Pregunta" }));
    await user.type(screen.getByLabelText("Pídele algo a hedy"), "¿Dónde se crean las citas?");
    await user.click(screen.getByRole("button", { name: "Enviar" }));
    expect(await screen.findByText(/Terminó/, { selector: ".activity" })).toBeInTheDocument();

    // Already in review: the backend sends no `task-updated` for this stop.
    await user.click(await screen.findByRole("button", { name: "Detener" }));
    await waitFor(() => expect(screen.queryByRole("button", { name: "Detener" })).toBeNull());
  });

  it("offers to wait or hand the task to another provider at a limit", async () => {
    const { api, user } = setup();
    await openProject(user, /^hedy/);
    await user.type(await screen.findByLabelText("Pídele algo a hedy"), "Recordatorios");
    await user.click(screen.getByLabelText("Planificar primero"));
    await user.click(screen.getByRole("button", { name: "Enviar" }));
    await screen.findByRole("region", { name: "Claude quiere correr un comando" });

    const task = (await api.tasks("/code/hedy"))[0]!;
    api.simulateLimit(task.id);
    const card = await screen.findByRole("region", { name: "Tu cuenta Claude personal va por el 92%" });
    // Never another Claude subscription: only another provider or an API key.
    expect(await within(card).findByRole("button", { name: "Pasar la tarea a ChatGPT" })).toBeInTheDocument();
    expect(within(card).getByRole("button", { name: "Pasar la tarea a Kimi" })).toBeInTheDocument();
    expect(within(card).queryByRole("button", { name: /Claude trabajo/ })).not.toBeInTheDocument();

    await user.click(within(card).getByRole("button", { name: "Pasar la tarea a ChatGPT" }));
    expect(await screen.findByText("Codex")).toBeInTheDocument();
    expect(await screen.findByText(/Codex: pasan los tests/)).toBeInTheDocument();
  });

  it("previews only local addresses", async () => {
    const { user } = setup();
    await openProject(user, /^hedy/);
    await user.click(await screen.findByRole("tab", { name: "Vista previa" }));
    const address = screen.getByLabelText("Dirección del servidor local");
    await user.clear(address);
    await user.type(address, "https://example.com");
    expect(screen.getByText(/Solo direcciones locales/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Abrir" })).toBeDisabled();
    await user.clear(address);
    await user.type(address, "http://localhost:5173");
    await user.click(screen.getByRole("button", { name: "Abrir" }));
    expect(screen.getByTitle("Vista previa")).toHaveAttribute("src", "http://localhost:5173");
  });

  it("keeps project tabs open and closes them", async () => {
    const { user } = setup();
    await openProject(user, /^hedy/);
    await openProject(user, /^homming/);
    const tabs = screen.getByRole("navigation", { name: "Proyectos abiertos" });
    expect(within(tabs).getByRole("button", { name: "homming" })).toHaveAttribute("aria-current", "page");
    await user.click(within(tabs).getByRole("button", { name: "Cerrar homming" }));
    expect(within(tabs).getByRole("button", { name: "hedy" })).toHaveAttribute("aria-current", "page");
  });
});
