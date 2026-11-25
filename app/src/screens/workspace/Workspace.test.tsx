import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";
import { renderCode } from "../../code/testing";
import { TerminalPanel } from "../../components/Terminal/TerminalPanel";
import { TerminalProvider } from "../../components/Terminal/TerminalProvider";
import { chooseMode, chooseOption, optionLabels } from "../../test/select";
import { ChangesPanel } from "./ChangesPanel";

// xterm needs a real canvas: a stand-in that draws nothing, so the terminal's own
// logic (open the shell, type into it) still runs.
vi.mock("@xterm/xterm", () => ({
  Terminal: class {
    cols = 80;
    rows = 24;
    options = {};
    loadAddon() {}
    open() {}
    write() {}
    focus() {}
    dispose() {}
    onData() {
      return { dispose() {} };
    }
    onResize() {
      return { dispose() {} };
    }
  },
}));
vi.mock("@xterm/addon-fit", () => ({
  FitAddon: class {
    fit() {}
  },
}));

/** jsdom has no ResizeObserver; the terminal watches its size with one. */
function stubResizeObserver() {
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
}

afterEach(() => {
  vi.unstubAllGlobals();
});

/** The project has a `package.json` with these scripts, beside these other files (lock files). */
function withPackage(api: MockApi, scripts: Record<string, string>, files: string[] = []) {
  api.files.set("/code/hedy", [...(api.files.get("/code/hedy") ?? []), "package.json", ...files]);
  const read = api.readFile.bind(api);
  vi.spyOn(api, "readFile").mockImplementation(async (copy, path, rev) =>
    copy.kind === "folder" && path === "package.json"
      ? { kind: "text", text: JSON.stringify({ name: "hedy", scripts }), size: 64, version: "v1" }
      : read(copy, path, rev),
  );
}

function setup() {
  const api = MockApi.withSampleData();
  api.delayMs = 0;
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<App api={api} queryClient={queryClient} />);
  return { api, user: userEvent.setup() };
}

const PANEL = "Cambios, archivos, terminal y vista previa";

async function openProject(user: ReturnType<typeof userEvent.setup>, name: RegExp) {
  await user.click(await screen.findByRole("button", { name }));
}

/** A project opens on its home: its tasks are listed there, a click away. */
async function openTask(user: ReturnType<typeof userEvent.setup>, project: RegExp, title: RegExp) {
  await openProject(user, project);
  const tasks = await screen.findByRole("navigation", { name: "Secciones del proyecto" });
  await user.click(await within(tasks.closest(".chat") as HTMLElement).findByRole("button", { name: title }));
}

const RUNNING = /^Revisa los tests del carrito/;
const PLAN = /^Agrega recordatorios por WhatsApp/;

describe("Workspace", () => {
  it("runs a task end to end: plan, permission, tests, diff and accept", async () => {
    const { user } = setup();
    await openProject(user, /^hedy/);
    expect(await screen.findByText(/¿Qué hacemos en hedy\?/)).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Modo" })).toHaveTextContent("Cambiar código");

    await user.type(screen.getByLabelText("Pídele algo a hedy"), "Agrega un recordatorio 24 horas antes");
    await user.click(screen.getByRole("button", { name: "Enviar" }));

    // Plan first: nothing changes until it is approved.
    const plan = await screen.findByRole("region", { name: "Plan de Claude Opus" });
    expect(within(plan).getByText(/Buscar dónde se crean las citas/)).toBeInTheDocument();
    await user.click(within(plan).getByRole("button", { name: "Aprobar plan" }));

    const card = await screen.findByRole("region", { name: "Claude quiere correr un comando" });
    expect(within(card).getByText("pytest tests/test_recordatorios.py")).toBeInTheDocument();
    // The steps before the card, summed up in one line that opens on demand.
    const group = screen.getByRole("button", { name: "2 pasos · editó 2 archivos" });
    expect(group).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("button", { name: "citas/servicio.py" })).toBeNull();
    await user.click(group);
    expect(group).toHaveAttribute("aria-expanded", "true");
    // The edited file is a link to it in the viewer.
    expect(screen.getByRole("button", { name: "citas/servicio.py" }).parentElement).toHaveTextContent("Claude editó citas/servicio.py");
    await user.click(within(card).getByRole("button", { name: "Permitir siempre en esta tarea" }));

    expect(await screen.findByText(/Claude: pasan los tests/)).toBeInTheDocument();
    expect(await screen.findByText(/Terminó/, { selector: ".turn-end" })).toBeInTheDocument();
    expect(screen.getByText(/Permitido siempre en esta tarea/)).toBeInTheDocument();
    const steps = screen.getByRole("region", { name: "Pasos" });
    expect(within(steps).getAllByText(/\(hecho\)/)).toHaveLength(3);

    const files = await screen.findByRole("list", { name: "Archivos cambiados" });
    expect(within(files).getByRole("button", { name: /^citas\/servicio\.py/ })).toBeInTheDocument();
    expect(screen.getByRole("table", { name: "Cambios en citas/servicio.py" })).toHaveTextContent(
      "programar_recordatorio(cita, horas=24)",
    );

    await user.click(screen.getByRole("button", { name: "Aceptar cambios…" }));
    const sheet = await screen.findByRole("dialog", { name: "Aceptar cambios" });
    await user.click(await within(sheet).findByRole("button", { name: "Aceptar y unir" }));
    expect(await screen.findByText("Cambios integrados en main.")).toBeInTheDocument();
    expect(await screen.findByText(/ya se aceptó/)).toBeInTheDocument();
    // Many steps: well over the default 5 s on a busy machine.
  }, 20_000);

  it("shows what the agent is doing: the working mascot and its word", async () => {
    const api = MockApi.withSampleData().withSampleActivity();
    api.delayMs = 0;
    render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
    const user = userEvent.setup();
    // The running sample task in tienda last searched the code (Grep).
    await openTask(user, /^tienda/, RUNNING);
    const word = await screen.findByText("Rastreando");
    const live = word.closest(".agent-row--live");
    expect(live).toHaveAttribute("aria-hidden");
    expect(live?.querySelector(".mascot")).toHaveAttribute("data-activity", "search");
    expect(live?.querySelector(".m-lens")).not.toBeNull();
    // A single working mascot in the chat: the others rest.
    expect(document.querySelectorAll('.timeline .mascot[data-state="working"]')).toHaveLength(1);

    await user.click(screen.getByRole("button", { name: "Más acciones" }));
    await user.click(screen.getByRole("menuitem", { name: "Estado" }));
    expect(await screen.findByRole("heading", { name: "tienda" })).toBeInTheDocument();
    await waitFor(() => expect(screen.getAllByText("Rastreando").length).toBeGreaterThanOrEqual(2));
  });

  it("corrects the plan from the composer", async () => {
    const { user } = setup();
    await openProject(user, /^hedy/);
    await user.type(await screen.findByLabelText("Pídele algo a hedy"), "Recordatorios");
    await user.click(screen.getByRole("button", { name: "Enviar" }));

    const plan = await screen.findByRole("region", { name: "Plan de Claude Opus" });
    await user.click(within(plan).getByRole("button", { name: "Corregir abajo" }));
    const composer = screen.getByLabelText("Corrige el plan de arriba");
    expect(composer).toHaveFocus();
    await user.type(composer, "usa Celery{Enter}");

    await waitFor(() => expect(screen.getAllByRole("region", { name: "Plan de Claude Opus" })).toHaveLength(2));
    expect(screen.getByText(/con cambios pedidos/)).toBeInTheDocument();
    expect(screen.getByText(/usa Celery/)).toBeInTheDocument();
  });

  it("asks without a worktree in Pregunta mode", async () => {
    const { user } = setup();
    await openProject(user, /^hedy/);
    await chooseMode(user, "Solo preguntar");
    await user.click(screen.getByRole("combobox", { name: "Modo" }));
    expect(screen.queryByRole("option", { name: "Cuando apruebes el plan" })).not.toBeInTheDocument();
    await user.keyboard("{Escape}");
    await user.type(screen.getByLabelText("Pídele algo a hedy"), "¿Dónde se crean las citas?");
    await user.click(screen.getByRole("button", { name: "Enviar" }));
    expect(await screen.findByText(/no cambian archivos/)).toBeInTheDocument();
    expect(await screen.findByText(/Terminó/, { selector: ".turn-end" })).toBeInTheDocument();
  });

  it("picks the mode in the composer, only for new tasks, remembered per project", async () => {
    const { user } = setup();
    await openProject(user, /^hedy/);
    const modes = await screen.findByRole("combobox", { name: "Modo" });
    expect(modes.closest(".composer")).not.toBeNull();
    expect(document.querySelector(".topbar [role=combobox]")).toBeNull();

    await user.click(modes);
    expect(screen.getByRole("listbox", { name: "Modo" })).toBeInTheDocument();
    // What it does; when it starts changing code (one AI); or a flow of several AIs.
    expect(optionLabels()).toEqual([
      "Cambiar código",
      "Solo preguntar",
      "Investigar",
      "Enseguida",
      "Cuando apruebes el plan",
      "Esencial",
      "Revisión acotada",
    ]);
    expect(screen.getByRole("option", { name: "Cuando apruebes el plan" })).toHaveAccessibleDescription(
      "Te enseña el plan; empieza cuando lo apruebas",
    );
    expect(screen.getByRole("option", { name: "Cambiar código" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("option", { name: "Solo preguntar" })).toHaveAccessibleDescription("Responde sin tocar archivos");
    await user.click(screen.getByRole("option", { name: "Investigar" }));
    expect(modes).toHaveTextContent("Investigar");
    await user.click(modes);
    expect(optionLabels()).toEqual(["Cambiar código", "Solo preguntar", "Investigar"]);
    await user.keyboard("{Escape}");
    expect(localStorage.getItem("hedy.mode./code/hedy")).toBe("research");

    // Another project keeps its own mode; coming back finds the one chosen here.
    await openProject(user, /^tienda/);
    await waitFor(() => expect(screen.getByRole("combobox", { name: "Modo" })).toHaveTextContent("Cambiar código"));
    await openProject(user, /^hedy/);
    await waitFor(() => expect(screen.getByRole("combobox", { name: "Modo" })).toHaveTextContent("Investigar"));

    // Inside a question, follow-ups keep its mode (it may only go on to change code), and one
    // placeholder (no label line above).
    await user.type(screen.getByLabelText("Pídele algo a hedy"), "¿Qué librerías hay para recordatorios?");
    await user.click(screen.getByRole("button", { name: "Enviar" }));
    expect(await screen.findByLabelText("Responde o pide algo más")).toHaveAttribute("placeholder", "Responde o pide algo más…");
    expect(screen.getByText("Responde o pide algo más")).toHaveClass("sr-only");
    await waitFor(() => expect(screen.getByRole("combobox", { name: "Modo" })).toHaveTextContent("Investigar"));

    // The project's crumb (its home, with the new task's composer) brings the selector back; so does Ctrl+N.
    await user.click(screen.getByRole("button", { name: "Volver a hedy" }));
    expect(await screen.findByRole("combobox", { name: "Modo" })).toBeInTheDocument();
    await user.click(await within(screen.getByRole("main")).findByRole("button", { name: /^¿Qué librerías hay para recordatorios\?/ }));
    await waitFor(() => expect(screen.getByLabelText("Responde o pide algo más")).toBeInTheDocument());
    await user.keyboard("{Control>}n{/Control}");
    expect(await screen.findByRole("combobox", { name: "Modo" })).toBeInTheDocument();
    expect(window.location.hash).toBe(`#p=${encodeURIComponent("/code/hedy")}&new`);
  });

  it("answers the plan and permission cards with the keyboard", async () => {
    const { user } = setup();
    await openProject(user, /^hedy/);
    await user.type(await screen.findByLabelText("Pídele algo a hedy"), "Recordatorios");
    expect(screen.getByText("Enter enviar · Shift+Enter salto")).toHaveClass("composer__hint");
    // Ctrl+Enter still sends, as an alias of Enter.
    await user.keyboard("{Control>}{Enter}{/Control}");

    // The card takes the focus (nothing else had it) and shows its keys.
    const plan = await screen.findByRole("region", { name: "Plan de Claude Opus" });
    await waitFor(() => expect(plan).toHaveFocus());
    const approve = within(plan).getByRole("button", { name: "Aprobar plan" });
    expect(approve).toHaveAttribute("aria-keyshortcuts", "Enter");
    expect(within(approve).getByText("Enter")).toHaveClass("kbd");
    expect(within(within(plan).getByRole("button", { name: "Corregir abajo" })).getByText("E")).toHaveClass("kbd");
    // Meanwhile the composer says what a message does now: it corrects the plan.
    const composer = screen.getByLabelText("Corrige el plan de arriba");
    expect(composer).toHaveAttribute("placeholder", "Escribe qué corregir del plan, o Ctrl+Enter para aprobarlo");
    // Empty while the agent runs: the round button stops it instead.
    expect(screen.getByRole("button", { name: "Detener" })).toBeEnabled();

    await user.keyboard("e");
    expect(composer).toHaveFocus();
    expect(composer).toHaveValue("");
    plan.focus();

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

  it("shows the side panel only while there are changes, unless the user chooses", async () => {
    const api = MockApi.withSampleData().withSampleActivity();
    api.delayMs = 0;
    render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
    const user = userEvent.setup();
    await openTask(user, /^hedy/, PLAN);
    // s1's plan waits: nothing to show, so no panel and the chat takes the width.
    await screen.findByRole("heading", { name: "Agrega recordatorios por WhatsApp antes de cada cita" });
    const toggle = screen.getByRole("button", { name: /^Panel lateral/ });
    expect(toggle).toHaveAttribute("aria-pressed", "false");
    expect(screen.queryByRole("complementary", { name: PANEL })).not.toBeInTheDocument();
    expect(document.querySelector(".workspace--no-panel")).not.toBeNull();

    // s5 has changes to review: the panel opens by itself.
    await user.keyboard("{Control>}3{/Control}");
    const panel = await screen.findByRole("complementary", { name: PANEL });
    expect(await within(panel).findByRole("button", { name: /Aceptar cambios/ })).toBeInTheDocument();
    expect(toggle).toHaveAttribute("aria-pressed", "true");
    expect(document.querySelector(".workspace--no-panel")).toBeNull();

    // Hidden by hand, it stays hidden for the session; the toggle says changes wait.
    await user.click(toggle);
    expect(screen.queryByRole("complementary", { name: PANEL })).not.toBeInTheDocument();
    expect(toggle).toHaveAttribute("aria-pressed", "false");
    expect(toggle).toHaveAccessibleName("Panel lateral: cambios, archivos, terminal y vista previa, hay cambios por revisar");
    expect(sessionStorage.getItem("hedy.sidePanel")).toBe("hidden");
    await user.keyboard("{Control>}1{/Control}");
    await user.keyboard("{Control>}3{/Control}");
    expect(await screen.findByRole("heading", { name: "Arregla el cálculo de impuestos en las facturas" })).toBeInTheDocument();
    expect(screen.queryByRole("complementary", { name: PANEL })).not.toBeInTheDocument();
  });

  it("puts the side panel away in focus mode; the toggle brings it back and leaves focus", async () => {
    const api = MockApi.withSampleData().withSampleActivity();
    api.delayMs = 0;
    render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
    const user = userEvent.setup();
    await openProject(user, /^hedy/);
    await user.keyboard("{Control>}3{/Control}");
    expect(await screen.findByRole("complementary", { name: PANEL })).toBeInTheDocument();

    await user.keyboard("{Control>}\\{/Control}");
    await waitFor(() => expect(screen.queryByRole("complementary", { name: PANEL })).not.toBeInTheDocument());
    const toggle = screen.getByRole("button", { name: /^Panel lateral/ });
    expect(toggle).toHaveAttribute("aria-pressed", "false");

    await user.click(toggle);
    expect(await screen.findByRole("complementary", { name: PANEL })).toBeInTheDocument();
    expect(document.querySelector(".app--focus")).toBeNull();
  });

  it("says why the Changes panel is empty, case by case, once opened", async () => {
    const api = MockApi.withSampleData().withSampleActivity();
    api.delayMs = 0;
    render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
    const user = userEvent.setup();
    await openTask(user, /^hedy/, PLAN);
    await user.click(await screen.findByRole("button", { name: /^Panel lateral/ }));
    const panel = await screen.findByRole("complementary", { name: PANEL });
    // A project task whose plan waits (s1), one that works (s3), one with changes (s5), a question (s6).
    expect(await within(panel).findByText("Aún no hay cambios: se crearán al aprobar el plan.")).toBeInTheDocument();
    await user.keyboard("{Control>}2{/Control}");
    expect(await within(panel).findByText("Aún no hay cambios: se crearán cuando la IA empiece a trabajar.")).toBeInTheDocument();
    // s5 has a worktree with changes to review: no empty message at all.
    await user.keyboard("{Control>}3{/Control}");
    expect(await within(panel).findByRole("button", { name: /Aceptar cambios/ })).toBeInTheDocument();
    await user.keyboard("{Control>}4{/Control}");
    expect(await within(panel).findByText("Las preguntas y la investigación no cambian archivos.")).toBeInTheDocument();
    expect(within(panel).queryByText(/Aún no hay cambios/)).not.toBeInTheDocument();
  });

  it("does not take the focus from the composer when a card appears", async () => {
    const api = MockApi.withSampleData().withSampleActivity();
    api.delayMs = 0;
    render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
    const user = userEvent.setup();
    await openTask(user, /^tienda/, RUNNING);
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
    await openTask(user, /^hedy/, PLAN);
    expect(await screen.findByRole("heading", { name: "Agrega recordatorios por WhatsApp antes de cada cita" })).toBeInTheDocument();
    expect(screen.queryByRole("combobox", { name: "Tarea" })).not.toBeInTheDocument();

    const toggle = screen.getByRole("button", { name: "Tareas (5)" });
    await user.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    const list = screen.getByRole("group", { name: "Tareas de este proyecto" });
    const current = within(list).getByRole("button", { name: /Agrega recordatorios por WhatsApp/ });
    expect(current).toHaveAttribute("aria-current", "true");
    expect(current).toHaveFocus();
    expect(within(current).getByText("Te espera: plan")).toHaveClass("status-pill--needs");
    expect(within(current).getByText("Claude personal")).toBeInTheDocument();
    expect(within(current).getByText("Ctrl+1")).toBeInTheDocument();
    const taxes = within(list).getByRole("button", { name: /Arregla el cálculo de impuestos/ });
    expect(within(taxes).getByText("Lista")).toHaveClass("status-pill--done");
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
    await user.click(within(screen.getByRole("group", { name: /^Tareas/ })).getByRole("button", { name: "Nueva tarea" }));
    expect(await screen.findByText(/¿Qué hacemos en hedy\?/)).toBeInTheDocument();
    expect(window.location.hash).toBe(`#p=${encodeURIComponent("/code/hedy")}&new`);
  });

  it("goes Back and Forward between screens and tasks", async () => {
    const api = MockApi.withSampleData().withSampleActivity();
    api.delayMs = 0;
    render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: /^Inicio/ }, { timeout: 8000 }));
    const needs = await screen.findByRole("region", { name: /^Te necesitan/ });
    await user.click(within(needs).getByRole("button", { name: /Migra el reporte de comisiones/ }));
    const title = "Migra el reporte de comisiones a la API nueva";
    expect(await screen.findByRole("heading", { name: title })).toBeInTheDocument();

    history.back();
    expect(await screen.findByRole("region", { name: /^Te necesitan/ })).toBeInTheDocument();
    expect(window.location.hash).toBe("#welcome");
    history.back();
    expect(await screen.findByRole("heading", { name: /¡Hola, Alex!/ })).toBeInTheDocument();
    history.forward();
    expect(await screen.findByRole("region", { name: /^Te necesitan/ })).toBeInTheDocument();
    history.forward();
    expect(await screen.findByRole("heading", { name: title })).toBeInTheDocument();
    expect(window.location.hash).toBe(`#p=${encodeURIComponent("/code/tienda")}&t=s2`);
  });

  it("refreshes a stopped task even when its status stays the same", async () => {
    const { user } = setup();
    await openProject(user, /^hedy/);
    await chooseMode(user, "Solo preguntar");
    await user.type(screen.getByLabelText("Pídele algo a hedy"), "¿Dónde se crean las citas?");
    await user.click(screen.getByRole("button", { name: "Enviar" }));
    expect(await screen.findByText(/Terminó/, { selector: ".turn-end" })).toBeInTheDocument();

    // Already in review: the backend sends no `task-updated` for this stop.
    // Stop lives in the composer (empty while the agent runs), not in the header.
    expect(document.querySelector(".chat__head")?.textContent).not.toMatch(/Detener/);
    await user.click(await screen.findByRole("button", { name: "Detener" }));
    await waitFor(() => expect(screen.queryByRole("button", { name: "Detener" })).toBeNull());
  });

  it("offers to wait or hand the task to another provider at a limit", async () => {
    const { api, user } = setup();
    await openProject(user, /^hedy/);
    await user.type(await screen.findByLabelText("Pídele algo a hedy"), "Recordatorios");
    await chooseMode(user, "Enseguida");
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
    const api = MockApi.withSampleData().withSampleActivity();
    api.delayMs = 0;
    render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
    const user = userEvent.setup();
    await openTask(user, /^hedy/, PLAN);
    await user.click(await screen.findByRole("button", { name: /^Panel lateral/ }));
    await user.click(await screen.findByRole("tab", { name: "Vista previa" }));
    const address = screen.getByLabelText("Dirección del servidor local");
    await user.clear(address);
    await user.type(address, "https://example.com");
    expect(screen.getByText(/Solo direcciones locales/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Abrir" })).toBeDisabled();
    await user.clear(address);
    await user.type(address, "http://localhost:5173");
    await user.click(screen.getByRole("button", { name: "Abrir" }));
    expect(screen.getAllByTitle("Vista previa").find((el) => el.tagName === "IFRAME")).toHaveAttribute("src", "http://localhost:5173");
  });

  it("expands the preview over the workspace, with device sizes, and opens it in the browser", async () => {
    const api = MockApi.withSampleData().withSampleActivity();
    api.delayMs = 0;
    const browser = vi.spyOn(api, "openLink");
    render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
    const user = userEvent.setup();
    await openTask(user, /^hedy/, PLAN);
    await user.click(await screen.findByRole("button", { name: /^Panel lateral/ }));
    await user.click(await screen.findByRole("tab", { name: "Vista previa" }));
    // Nothing to expand before a page is open.
    expect(screen.queryByRole("button", { name: "Ampliar" })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Abrir" }));
    const frame = () => screen.getAllByTitle("Vista previa").find((el) => el.tagName === "IFRAME")!;
    const page = frame();

    await user.click(screen.getByRole("button", { name: "Ampliar" }));
    const expanded = screen.getByRole("dialog", { name: "Vista previa" });
    expect(expanded).toHaveClass("preview--expanded");
    expect(frame()).toBe(page);
    await user.click(within(expanded).getByRole("radio", { name: "Móvil" }));
    expect(frame()).toHaveStyle({ width: "390px" });

    await user.click(within(expanded).getByRole("button", { name: "Abrir en el navegador" }));
    expect(browser).toHaveBeenCalledWith("http://localhost:3000");

    // Escape with the composer focused closes the preview, and only that.
    const stop = vi.spyOn(api, "stopTask");
    document.querySelector<HTMLTextAreaElement>(".composer textarea")!.focus();
    expect(document.activeElement?.tagName).toBe("TEXTAREA");
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "Vista previa" })).toBeNull();
    expect(stop).not.toHaveBeenCalled();
    // The same page, back in the panel at its width.
    expect(frame()).toBe(page);
    expect(frame()).toHaveStyle({ width: "100%" });
  });

  it("starts the dev server from the preview: a terminal in the task's copy with the command typed", async () => {
    stubResizeObserver();
    const api = MockApi.withSampleData().withSampleActivity();
    api.delayMs = 0;
    withPackage(api, { dev: "vite", build: "vite build" });
    const open = vi.spyOn(api, "terminalOpen");
    const write = vi.spyOn(api, "terminalWrite");
    render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
    const user = userEvent.setup();
    await openTask(user, /^hedy/, /^Arregla el cálculo de impuestos/);
    // s5 has changes: the panel opens by itself.
    const panel = await screen.findByRole("complementary", { name: PANEL });
    await user.click(within(panel).getByRole("tab", { name: "Vista previa" }));
    expect(within(panel).getByText(/Arranca el servidor/)).toBeInTheDocument();
    await user.click(await within(panel).findByRole("button", { name: "Arrancar «npm run dev»" }));

    const terminals = await screen.findByRole("tablist", { name: "Terminales" });
    expect(within(terminals).getByRole("tab", { name: "Terminal · npm run dev" })).toHaveAttribute("aria-selected", "true");
    await waitFor(() => expect(write).toHaveBeenCalledWith(expect.any(Number), "npm run dev\r"));
    expect(open).toHaveBeenCalledWith({ kind: "shell", path: "/code/hedy/.hedy/worktrees/s5" }, expect.anything(), expect.any(Function));
    expect(write).toHaveBeenCalledTimes(1);
    expect(write).toHaveBeenCalledWith(await open.mock.results[0]?.value, "npm run dev\r");
  });

});

describe("Preview's start button", () => {
  const PROJECT = "/code/hedy";

  async function showPreview(api: MockApi) {
    api.delayMs = 0;
    renderCode(
      <TerminalProvider>
        <ChangesPanel project={PROJECT} task={null} />
        <TerminalPanel />
      </TerminalProvider>,
      api,
    );
    const user = userEvent.setup();
    await user.click(screen.getByRole("tab", { name: "Vista previa" }));
    return user;
  }

  it.each([
    ["npm without a lock file", { dev: "vite" }, [], "npm run dev"],
    ["npm with its lock file", { dev: "vite" }, ["package-lock.json"], "npm run dev"],
    ["pnpm", { dev: "vite" }, ["pnpm-lock.yaml"], "pnpm dev"],
    ["yarn", { dev: "vite" }, ["yarn.lock"], "yarn dev"],
    ["bun (bun.lockb)", { dev: "vite" }, ["bun.lockb"], "bun run dev"],
    ["bun (bun.lock)", { dev: "vite" }, ["bun.lock"], "bun run dev"],
    ["start without dev", { start: "node server.js" }, [], "npm run start"],
    ["dev before start", { start: "node server.js", dev: "vite" }, ["yarn.lock"], "yarn dev"],
    ["start with pnpm", { start: "next start" }, ["pnpm-lock.yaml"], "pnpm start"],
  ])("runs the script with the project's package manager: %s", async (_, scripts, files, command) => {
    const api = MockApi.withSampleData();
    withPackage(api, scripts, files);
    await showPreview(api);
    expect(await screen.findByRole("button", { name: `Arrancar «${command}»` })).toBeInTheDocument();
  });

  it("opens the terminal in the project's folder without a task", async () => {
    stubResizeObserver();
    const api = MockApi.withSampleData();
    withPackage(api, { dev: "vite" }, ["pnpm-lock.yaml"]);
    const open = vi.spyOn(api, "terminalOpen");
    const write = vi.spyOn(api, "terminalWrite");
    const user = await showPreview(api);
    await user.click(await screen.findByRole("button", { name: "Arrancar «pnpm dev»" }));
    expect(await screen.findByRole("tab", { name: "Terminal · pnpm dev" })).toBeInTheDocument();
    await waitFor(() => expect(write).toHaveBeenCalledWith(expect.any(Number), "pnpm dev\r"));
    expect(open).toHaveBeenCalledWith({ kind: "shell", path: PROJECT }, expect.anything(), expect.any(Function));
  });

  it("offers nothing without a dev or start script, or without a package.json", async () => {
    const api = MockApi.withSampleData();
    withPackage(api, { build: "tsc", test: "vitest" }, ["pnpm-lock.yaml"]);
    await showPreview(api);
    await waitFor(() => expect(api.readFile).toHaveBeenCalledWith({ kind: "folder", project: PROJECT }, "package.json"));
    await act(async () => {});
    expect(screen.queryByRole("button", { name: /^Arrancar/ })).not.toBeInTheDocument();
  });

  it.each([
    ["scripts as a string", JSON.stringify({ name: "hedy", scripts: "dev" })],
    ["scripts as a list", JSON.stringify({ name: "hedy", scripts: ["dev", "start"] })],
    ["scripts null", JSON.stringify({ name: "hedy", scripts: null })],
    ["a manifest that is not an object", JSON.stringify("dev")],
    ["a manifest that is null", "null"],
    ["broken JSON", '{ "scripts": { "dev": '],
  ])("offers nothing, and keeps working, for a bad package.json: %s", async (_, text) => {
    const api = MockApi.withSampleData();
    api.files.set(PROJECT, [...(api.files.get(PROJECT) ?? []), "package.json"]);
    const read = api.readFile.bind(api);
    const manifest = vi
      .spyOn(api, "readFile")
      .mockImplementation(async (copy, path, rev) =>
        copy.kind === "folder" && path === "package.json" ? { kind: "text", text, size: text.length, version: "v1" } : read(copy, path, rev),
      );
    await showPreview(api);
    await waitFor(() => expect(manifest).toHaveBeenCalledWith({ kind: "folder", project: PROJECT }, "package.json"));
    await act(async () => {});
    expect(screen.queryByRole("button", { name: /^Arrancar/ })).not.toBeInTheDocument();
    // The preview is still there to type an address.
    expect(screen.getByRole("tabpanel", { name: "Vista previa" })).toBeInTheDocument();
    expect(screen.getByText(/Arranca el servidor/)).toBeInTheDocument();
  });

  it("offers nothing for a project without a package.json", async () => {
    const api = MockApi.withSampleData();
    const files = vi.spyOn(api, "projectFiles");
    const read = vi.spyOn(api, "readFile");
    await showPreview(api);
    await waitFor(() => expect(files).toHaveBeenCalledWith(PROJECT));
    await act(async () => {});
    expect(read).not.toHaveBeenCalledWith(expect.anything(), "package.json");
    expect(screen.queryByRole("button", { name: /^Arrancar/ })).not.toBeInTheDocument();
  });
});

describe("Composer", () => {
  it("keeps the chips and the round Send button in one row, and sends with Enter in a project", async () => {
    const { api, user } = setup();
    const start = vi.spyOn(api, "startTask");
    await openProject(user, /^hedy/);
    const composer = await screen.findByLabelText("Pídele algo a hedy");
    const send = screen.getByRole("button", { name: "Enviar" });
    const bar = send.closest(".composer__bar");
    // Send sits in the bar itself, never among the chips that scroll.
    expect(send.parentElement).toBe(bar);
    expect(bar?.querySelector(".composer__chips")).toContainElement(screen.getByRole("combobox", { name: "Modo" }));
    expect(send).toBeDisabled();
    expect(screen.getByText("Enter enviar · Shift+Enter salto")).toBeInTheDocument();

    const model = screen.getByRole("combobox", { name: "Modelo" });
    expect(model).toHaveTextContent("Automático");
    await user.click(model);
    expect(optionLabels()).toEqual(["Automático", "Fable", "Opus", "Sonnet", "Haiku"]);
    expect(screen.getByRole("option", { name: "Haiku" })).toHaveAccessibleDescription("El más rápido · $");
    await user.click(screen.getByRole("option", { name: "Sonnet" }));
    expect(model).toHaveTextContent("Sonnet");

    // When it starts changing code, in the mode menu by keyboard: it stays on the mode.
    const modes = screen.getByRole("combobox", { name: "Modo" });
    modes.focus();
    await user.keyboard("{ArrowDown}{Home}{ArrowDown}{ArrowDown}{ArrowDown}");
    const now = screen.getByRole("option", { name: "Enseguida" });
    expect(modes).toHaveAttribute("aria-activedescendant", now.id);
    expect(modes).toHaveTextContent("Cambiar código · tras tu OK");
    await user.keyboard("{Enter}");
    expect(modes).not.toHaveTextContent("tras tu OK");
    await user.click(modes);
    expect(screen.getByRole("option", { name: "Enseguida" })).toHaveAccessibleDescription("Implementa y te deja los cambios");
    await user.keyboard("{Escape}");
    await user.click(composer);

    // Shift+Enter is a new line; Enter sends, as everywhere.
    await user.type(composer, "Primera línea{Shift>}{Enter}{/Shift}segunda");
    expect(start).not.toHaveBeenCalled();
    expect(composer).toHaveValue("Primera línea\nsegunda");
    expect(send).toBeEnabled();
    await user.keyboard("{Enter}");
    expect(start).toHaveBeenCalledWith({
      project: "/code/hedy",
      prompt: "Primera línea\nsegunda",
      mode: "project",
      model: "sonnet",
      planFirst: false,
      permissions: "manual",
    });
  });

  it("starts a task with the permission mode picked, and changes an open task's", async () => {
    const { api, user } = setup();
    const start = vi.spyOn(api, "startTask");
    const change = vi.spyOn(api, "setTaskPermissions");
    await openProject(user, /^hedy/);
    const composer = await screen.findByLabelText("Pídele algo a hedy");
    const permissions = screen.getByRole("combobox", { name: "Permisos" });
    expect(permissions).toHaveTextContent("Manual");
    await user.click(permissions);
    expect(optionLabels()).toEqual(["Manual", "Aceptar ediciones", "Automático"]);
    expect(screen.getByRole("option", { name: "Automático" })).toHaveAccessibleDescription(
      /dentro de su copia; pregunta por lo arriesgado o lo de fuera/,
    );
    await user.click(screen.getByRole("option", { name: "Automático" }));
    await user.type(composer, "Arregla el login{Enter}");
    await waitFor(() => expect(start).toHaveBeenCalledWith(expect.objectContaining({ permissions: "auto" })));
    const created = await start.mock.results[0]!.value;
    expect(created.permissions).toBe("auto");
    // The open task says so next to its status, and changes it from the same chip.
    expect(await screen.findByText("Automático", { selector: ".mode-tag" })).toBeInTheDocument();
    await chooseOption(user, screen.getByRole("combobox", { name: "Permisos" }), "Aceptar ediciones");
    expect(change).toHaveBeenCalledWith(created.id, "acceptEdits");
  });

  it("asks with Enter in a conversation, on the account and model of its chips", async () => {
    window.history.replaceState(null, "", "#c");
    const { api, user } = setup();
    const start = vi.spyOn(api, "startConversation");
    const composer = await screen.findByRole("textbox", { name: "¿Qué quieres saber?" });
    expect(screen.getByText("Enter enviar · Shift+Enter salto")).toBeInTheDocument();

    const account = await screen.findByRole("combobox", { name: "Cuenta de la pregunta" });
    expect(account).toHaveTextContent("Claude personal");
    await user.click(account);
    expect(screen.getByRole("listbox", { name: "Cuenta" })).toBeInTheDocument();
    // Each account says how much of its plan is used; Hedy never switches by itself.
    expect(screen.getByRole("option", { name: "Claude personal" })).toHaveAccessibleDescription("Claude Code · Semana · 83%");
    expect(screen.getByText("Hedy nunca cambia de cuenta por su cuenta")).toBeInTheDocument();
    await user.click(screen.getByRole("option", { name: "Kimi" }));
    expect(account).toHaveTextContent("Kimi");

    await chooseOption(user, screen.getByRole("combobox", { name: "Modelo" }), "kimi-k3");
    await chooseMode(user, "Investigar");
    await user.click(screen.getByRole("combobox", { name: "Modo" }));
    expect(screen.queryByRole("option", { name: "Cuando apruebes el plan" })).not.toBeInTheDocument();
    await user.keyboard("{Escape}");

    // Shift+Enter is a new line; Enter sends.
    await user.type(composer, "Uno{Shift>}{Enter}{/Shift}dos");
    expect(start).not.toHaveBeenCalled();
    await user.keyboard("{Enter}");
    expect(start).toHaveBeenCalledWith({ prompt: "Uno\ndos", mode: "research", account: "kimi", model: "kimi-k3" });
  });

  it("turns Send into Stop while the agent runs and nothing is written; Esc stops too", async () => {
    const api = MockApi.withSampleData().withSampleActivity();
    api.delayMs = 0;
    const stop = vi.spyOn(api, "stopTask");
    render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
    const user = userEvent.setup();
    await openTask(user, /^tienda/, RUNNING);
    const composer = await screen.findByLabelText(/^Escribe mientras Claude Opus trabaja/);
    expect(composer).toHaveAttribute("placeholder", "Escribe; se lo paso cuando termine su turno…");

    const stopButton = screen.getByRole("button", { name: "Detener" });
    expect(stopButton).toHaveAttribute("aria-keyshortcuts", "Escape");
    expect(stopButton.closest(".composer__bar")).not.toBeNull();
    expect(screen.getByText("Esc detener")).toHaveClass("composer__hint");

    // With text it queues, as before; empty again, it stops.
    await user.type(composer, "Una cosa más");
    expect(screen.queryByRole("button", { name: "Detener" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Poner en cola" })).toBeEnabled();
    await user.clear(composer);
    expect(screen.getByRole("button", { name: "Detener" })).toBeInTheDocument();

    // Esc in the composer stops, unless a popover is open there.
    await user.type(composer, "/");
    await user.keyboard("{Escape}");
    expect(stop).not.toHaveBeenCalled();
    await user.keyboard("{Escape}");
    expect(stop).toHaveBeenCalledWith("s4");
  });

  it("keeps the task's header to one quiet row: Status and Memory in the ⋯ menu", async () => {
    const api = MockApi.withSampleData().withSampleActivity();
    api.delayMs = 0;
    render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
    const user = userEvent.setup();
    await openTask(user, /^hedy/, PLAN);
    const head = (await screen.findByRole("heading", { level: 1, name: PLAN })).closest("header") as HTMLElement;
    expect(within(head).getAllByRole("button").map((b) => b.textContent)).toEqual([
      "hedy /",
      "Detalles",
      expect.stringMatching(/^Tareas \(5\)/),
      expect.stringMatching(/^Panel lateral/),
      expect.anything(),
    ]);
    await user.click(within(head).getByRole("button", { name: "Más acciones" }));
    const menu = screen.getByRole("menu", { name: "Más acciones" });
    expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Estado", "Memoria"]);
    await user.click(within(menu).getByRole("menuitem", { name: "Memoria" }));
    expect(await screen.findByRole("heading", { level: 1, name: "Memoria de hedy" })).toBeInTheDocument();
    expect(window.location.hash).toBe(`#m=${encodeURIComponent("/code/hedy")}`);
    expect(screen.getByRole("combobox", { name: "Para" })).toHaveValue("project:/code/hedy");
  });

  it("keeps templates one quiet button away: use one, or save the message as one", async () => {
    const api = MockApi.withSampleData().withSampleTemplates();
    api.delayMs = 0;
    render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
    const user = userEvent.setup();
    await openProject(user, /^hedy/);
    const composer = await screen.findByLabelText("Pídele algo a hedy");
    const templates = screen.getByRole("button", { name: "Plantillas" });

    await user.click(templates);
    expect(screen.getByRole("menuitem", { name: "Guardar como plantilla" })).toBeDisabled();
    await user.click(screen.getByRole("menuitem", { name: "Usar una plantilla" }));
    expect(await screen.findByRole("listbox", { name: "Plantillas y comandos" })).toBeInTheDocument();
    expect(composer).toHaveValue("/");
    await waitFor(() => expect(composer).toHaveFocus());

    await user.clear(composer);
    await user.type(composer, "Algo que repito");
    await user.click(templates);
    expect(screen.getByRole("menuitem", { name: "Usar una plantilla" })).toBeDisabled();
    await user.keyboard("{Escape}");
    expect(templates).toHaveFocus();
  });
});

