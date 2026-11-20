import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { App } from "../../App";
import { ApiProvider } from "../../api/context";
import { MockApi } from "../../api/MockApi";
import type { Task } from "../../api/types";
import { I18nProvider } from "../../i18n";
import { TaskSwitcher } from "./TaskSwitcher";

// xterm needs a real canvas; the Terminal tab is not exercised here.
vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

const HEDY = "/code/hedy";
const PANEL = "Código, revisión, terminal y vista previa";
const TAXES = "Arregla el cálculo de impuestos en las facturas";
const BRANCH = "hedy/arregla-el-calculo-de-impuestos-s5";

/** The app over the sample tasks, on `hash`; `prepare` changes the data first. */
async function setup(hash = "#", prepare?: (api: MockApi) => Promise<void>) {
  const api = MockApi.withSampleData().withSampleActivity();
  api.delayMs = 0;
  await prepare?.(api);
  history.replaceState(null, "", hash);
  render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
  return { api, user: userEvent.setup() };
}

const taskHash = (id: string, project = HEDY) => `#p=${encodeURIComponent(project)}&t=${id}`;

/** The header around the level-1 heading named `name`. */
async function headerOf(name: string | RegExp) {
  const heading = await screen.findByRole("heading", { level: 1, name }, { timeout: 5000 });
  return heading.closest("header") as HTMLElement;
}

afterEach(() => {
  history.replaceState(null, "", "#");
  sessionStorage.clear();
  localStorage.clear();
});

describe("A project's home", () => {
  it("is where the project lands, never an old task: its tabs, the composer and its tasks by what they need", async () => {
    const { api, user } = await setup();
    const editor = vi.spyOn(api, "openInEditor");
    const sidebar = await screen.findByRole("complementary", { name: "Navegación" }, { timeout: 5000 });
    await user.click(await within(sidebar).findByRole("button", { name: /^hedy/ }));

    const head = await headerOf("hedy");
    expect(window.location.hash).toBe(`#p=${encodeURIComponent(HEDY)}`);
    // s1 is the latest open task: it is listed, not opened.
    expect(screen.queryByRole("heading", { level: 1, name: /Agrega recordatorios/ })).toBeNull();
    expect(screen.getByLabelText("Pídele algo a hedy")).toHaveValue("");

    const tabs = within(head).getByRole("navigation", { name: "Secciones del proyecto" });
    expect(within(tabs).getAllByRole("button").map((tab) => tab.textContent)).toEqual(["Tareas", "Memoria", "Resumen", "Ajustes"]);
    expect(within(tabs).getByRole("button", { name: "Tareas" })).toHaveAttribute("aria-current", "page");
    await user.click(within(head).getByRole("button", { name: "Abrir en VS Code" }));
    expect(editor).toHaveBeenCalledWith(HEDY);

    // What waits for the user first, then what is ready; nothing works in hedy.
    const waiting = screen.getByRole("region", { name: "Te esperan" });
    const plan = within(waiting).getByRole("button", { name: /^Agrega recordatorios por WhatsApp/ });
    expect(within(plan).getByText("Te espera: plan")).toHaveClass("status-pill--needs");
    const paused = within(waiting).getByRole("button", { name: /^Traduce los correos de confirmación/ });
    expect(within(paused).getByText(/^En pausa · retoma \d\d:\d\d$/)).toHaveClass("status-pill--blocked");
    const ready = screen.getByRole("region", { name: "Listas" });
    expect(within(within(ready).getByRole("button", { name: new RegExp(`^${TAXES}`) })).getByText("Lista")).toHaveClass("status-pill--done");
    expect(within(ready).getByRole("button", { name: /^¿Dónde se calculan las comisiones\?/ })).toHaveTextContent("Respondida");
    expect(screen.queryByRole("region", { name: "Trabajando" })).toBeNull();

    // Finished ones fold behind one line.
    expect(screen.queryByRole("button", { name: /^Cambia el color del botón/ })).toBeNull();
    const older = screen.getByRole("button", { name: "1 terminada · ver historial" });
    expect(older).toHaveAttribute("aria-expanded", "false");
    await user.click(older);
    expect(older).toHaveAttribute("aria-expanded", "true");
    expect(older).toHaveTextContent("Ocultar terminadas");
    expect(screen.getByRole("button", { name: /^Cambia el color del botón/ })).toHaveTextContent("Descartada");

    await user.click(within(ready).getByRole("button", { name: new RegExp(`^${TAXES}`) }));
    expect(await screen.findByRole("heading", { level: 1, name: TAXES })).toBeInTheDocument();
    expect(window.location.hash).toBe(taskHash("s5"));
  });

  it("opens its other sections from the tabs", async () => {
    const hash = `#p=${encodeURIComponent(HEDY)}`;
    const { user } = await setup(hash);
    const tab = async (name: string) =>
      user.click(within(await screen.findByRole("navigation", { name: "Secciones del proyecto" }, { timeout: 5000 })).getByRole("button", { name }));

    await tab("Memoria");
    expect(await screen.findByRole("heading", { name: "Memoria de hedy" })).toBeInTheDocument();
    history.back();
    await tab("Resumen");
    expect(await screen.findByRole("heading", { level: 1, name: "hedy" })).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByRole("navigation", { name: "Secciones del proyecto" })).toBeNull());
    expect(window.location.hash).not.toBe(hash);
    history.back();
    await tab("Ajustes");
    expect(await screen.findByRole("heading", { name: "Tus cuentas y modelos" })).toBeInTheDocument();
  });
});

describe("A task's header", () => {
  it("names its project as a way home and says its one state; the rest waits in Detalles", async () => {
    const { api, user } = await setup(taskHash("s5"));
    const editor = vi.spyOn(api, "openInEditor");
    const head = await headerOf(TAXES);
    expect(within(head).getByText("Lista")).toHaveClass("status-pill", "status-pill--done");
    expect(head.querySelectorAll(".status-pill")).toHaveLength(1);
    // No subtitle with the branch or the mode, and no "Nueva tarea" of its own.
    expect(head).not.toHaveTextContent(BRANCH);
    expect(head).not.toHaveTextContent("Cambiar código");
    expect(within(head).queryByRole("button", { name: "Nueva tarea" })).toBeNull();

    await user.click(within(head).getByRole("button", { name: "Detalles" }));
    const details = screen.getByRole("dialog", { name: "Detalles de la tarea" });
    expect(within(details).getByText("Cambiar código")).toBeInTheDocument();
    expect(within(details).getByText(`${BRANCH}, desde main`)).toBeInTheDocument();
    expect(within(details).getByText("/code/hedy/.hedy/worktrees/s5")).toBeInTheDocument();
    expect(within(details).getByText("Quién hace cada paso")).toBeInTheDocument();
    expect(within(details).getByText(/^Implementación: Claude personal/)).toHaveAttribute("aria-current", "step");
    // Nothing spent on record: no line for it.
    expect(within(details).queryByText("Gasto")).toBeNull();

    await user.click(within(details).getByRole("button", { name: "Copiar rama" }));
    await expect(navigator.clipboard.readText()).resolves.toBe(BRANCH);
    expect(await screen.findByText(`Rama copiada: ${BRANCH}`)).toBeInTheDocument();
    await user.click(within(details).getByRole("button", { name: "Abrir en VS Code" }));
    expect(editor).toHaveBeenCalledWith("/code/hedy/.hedy/worktrees/s5");

    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Detalles de la tarea" })).toBeNull());
    const crumb = within(head).getByRole("button", { name: "hedy /" });
    expect(crumb).toHaveAttribute("title", "Inicio de hedy");
    await user.click(crumb);
    expect(await screen.findByRole("navigation", { name: "Secciones del proyecto" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 1, name: "hedy" })).toBeInTheDocument();
  });

  it("says a task waits for a plan, or is paused by a limit with the mascot asleep", async () => {
    const { user } = await setup(taskHash("s1"));
    const head = await headerOf(/^Agrega recordatorios/);
    expect(within(head).getByText("Te espera: plan")).toHaveClass("status-pill--needs");

    await user.keyboard("{Control>}2{/Control}");
    const paused = await headerOf("Traduce los correos de confirmación");
    expect(within(paused).getByText(/^En pausa · retoma \d\d:\d\d$/)).toHaveClass("status-pill--blocked");
    await waitFor(() => expect(document.querySelector('.timeline .mascot[data-state="working"]')).toBeNull());
    expect(document.querySelector('.timeline .mascot[data-state="sleeping"]')).not.toBeNull();
  });

  it("lists the project's tasks grouped by what they need, finished ones folded, Ctrl+digit in the list's order", async () => {
    const { user } = await setup(taskHash("s1"));
    await headerOf(/^Agrega recordatorios/);
    const toggle = screen.getByRole("button", { name: "Tareas (5)" });
    await user.click(toggle);
    const list = screen.getByRole("group", { name: "Tareas de este proyecto" });
    expect(within(list).queryByRole("searchbox", { name: "Filtrar tareas" })).toBeNull();
    expect(within(list).getAllByRole("region").map((g) => g.getAttribute("aria-label"))).toEqual(["Te esperan", "Listas"]);

    const waiting = within(list).getByRole("region", { name: "Te esperan" });
    const current = within(waiting).getByRole("button", { name: /^Agrega recordatorios por WhatsApp/ });
    expect(current).toHaveAttribute("aria-current", "true");
    expect(current).toHaveFocus();
    expect(within(current).getByText("Te espera: plan")).toHaveClass("status-pill--needs");
    expect(within(current).getByText("Claude personal")).toBeInTheDocument();
    expect(within(current).getByText("Ctrl+1")).toBeInTheDocument();
    expect(within(waiting).getByRole("button", { name: /^Traduce los correos/ })).toHaveTextContent(/En pausa/);
    expect(current.querySelector(".board-state")).toBeNull();

    const ready = within(list).getByRole("region", { name: "Listas" });
    const taxes = within(ready).getByRole("button", { name: new RegExp(`^${TAXES}`) });
    expect(within(taxes).getByText("Lista")).toHaveClass("status-pill--done");
    expect(within(taxes).getByText("hace 40 min")).toBeInTheDocument();
    expect(within(taxes).getByText("Ctrl+3")).toBeInTheDocument();

    expect(within(list).queryByRole("button", { name: /^Cambia el color/ })).toBeNull();
    await user.click(within(list).getByRole("button", { name: "Terminadas (1)" }));
    const done = within(list).getByRole("region", { name: "Terminadas (1)" });
    expect(within(done).getByRole("button", { name: /^Cambia el color/ })).toHaveTextContent("Descartada");

    await user.keyboard("{Escape}");
    await user.keyboard("{Control>}3{/Control}");
    expect(await screen.findByRole("heading", { level: 1, name: TAXES })).toBeInTheDocument();
  });
});

describe("The end of a turn", () => {
  it("asks to review a finished task, and to fix the tests that fail", async () => {
    // The panel was put away by hand: the card brings it back.
    sessionStorage.setItem("hedy.sidePanel", "hidden");
    const { api, user } = await setup(taskHash("s5"));
    const queue = vi.spyOn(api, "queueMessage");
    const card = await screen.findByRole("region", { name: "Terminó. Te toca revisar" }, { timeout: 5000 });
    expect(await within(card).findByText("3 archivos cambiados · hay tests que fallan")).toBeInTheDocument();
    expect(screen.queryByRole("complementary", { name: PANEL })).toBeNull();

    const review = within(card).getByRole("button", { name: "Revisar cambios" });
    expect(review).toHaveClass("btn--primary");
    await user.click(review);
    const panel = await screen.findByRole("complementary", { name: PANEL });
    expect(await within(panel).findByRole("list", { name: "Archivos cambiados" })).toBeInTheDocument();

    await user.click(within(card).getByRole("button", { name: "Pedir que arregle los tests" }));
    expect(queue).toHaveBeenCalledWith("s5", "Los tests fallan. Arréglalos sin cambiar lo que comprueban.");
  });

  it("offers to retry a task that failed", async () => {
    const { api, user } = await setup(taskHash("s7", "/code/tienda"));
    const queue = vi.spyOn(api, "queueMessage");
    const card = await screen.findByRole("region", { name: "Se detuvo con un error" }, { timeout: 5000 });
    await user.click(within(card).getByRole("button", { name: "Reintentar" }));
    expect(queue).toHaveBeenCalledWith("s7", "Inténtalo de nuevo desde donde lo dejaste.");
  });

  it("stays out of an answered question", async () => {
    await setup(taskHash("s6"));
    const head = await headerOf("¿Dónde se calculan las comisiones?");
    expect(within(head).getByText("Respondida")).toHaveClass("status-pill--done");
    expect(screen.queryByRole("region", { name: /Terminó|Se detuvo/ })).toBeNull();
  });
});

describe("A closed task", () => {
  it("merged: says where it went, with undo, publish and a new task instead of a composer", async () => {
    sessionStorage.setItem("hedy.sidePanel", "hidden");
    const { api, user } = await setup(taskHash("s5"), (api) => api.acceptTask("s5"));
    const undo = vi.spyOn(api, "undoAccept");
    const head = await headerOf(TAXES);
    expect(within(head).getByText("Unida a main")).toHaveClass("status-pill--closed");

    const bar = screen.getByRole("region", { name: "Tarea cerrada" });
    expect(bar).toHaveTextContent("Unida a main. Esta tarea ya está cerrada.");
    expect(screen.queryByRole("textbox", { name: /Responde o pide algo más/ })).toBeNull();
    expect(within(bar).getAllByRole("button").map((b) => b.textContent)).toEqual(["Deshacer", "Publicar", "Nueva tarea"]);

    await user.click(within(bar).getByRole("button", { name: "Publicar" }));
    expect(await screen.findByRole("complementary", { name: PANEL })).toBeInTheDocument();
    await user.click(within(bar).getByRole("button", { name: "Deshacer" }));
    expect(undo).toHaveBeenCalledWith("s5");
  });

  it("kept on its branch: names the branch and offers the pull request, no undo", async () => {
    const { user } = await setup(taskHash("s5"), (api) => api.acceptTask("s5", null, "branch"));
    const head = await headerOf(TAXES);
    expect(within(head).getByText("En su rama")).toHaveClass("status-pill--closed");
    const bar = screen.getByRole("region", { name: "Tarea cerrada" });
    expect(bar).toHaveTextContent(`Guardada en su rama ${BRANCH}. Esta tarea ya está cerrada.`);
    expect(within(bar).getAllByRole("button").map((b) => b.textContent)).toEqual(["Abrir PR", "Nueva tarea"]);

    await user.click(within(bar).getByRole("button", { name: "Nueva tarea" }));
    expect(await screen.findByLabelText("Pídele algo a hedy")).toHaveValue("");
    expect(window.location.hash).toBe(`#p=${encodeURIComponent(HEDY)}&new`);
  });

  it("discarded: offers a similar new task, its request already in the composer", async () => {
    const { user } = await setup(taskHash("s8"));
    const head = await headerOf("Cambia el color del botón de reservar");
    expect(within(head).getByText("Descartada")).toHaveClass("status-pill--closed");
    const bar = screen.getByRole("region", { name: "Tarea cerrada" });
    expect(bar).toHaveTextContent("Descartada: no se unió nada. Esta tarea ya está cerrada.");
    expect(within(bar).queryByRole("button", { name: "Deshacer" })).toBeNull();
    const similar = within(bar).getByRole("button", { name: "Nueva tarea parecida" });
    expect(similar).toHaveClass("btn--primary");

    await user.click(similar);
    expect(await screen.findByRole("navigation", { name: "Secciones del proyecto" })).toBeInTheDocument();
    await waitFor(() => expect(screen.getByLabelText("Pídele algo a hedy")).toHaveValue("Cambia el color del botón de reservar"));
  });
});

describe("The task switcher's filter", () => {
  function tasks(n: number): Task[] {
    const now = Math.floor(Date.now() / 1000);
    return Array.from({ length: n }, (_, i) => ({
      id: `t${i + 1}`,
      project: HEDY,
      mode: "project",
      title: i === 0 ? "Sube la versión" : `Tarea número ${i + 1}`,
      prompt: "",
      account: "claude-personal",
      model: "opus",
      workspace: null,
      sessionId: null,
      // The first one is closed: it lists last, but keeps Ctrl+1.
      status: i === 0 ? "accepted" : "review",
      createdAt: now - i * 60,
      stages: [],
      stage: 0,
      live: false,
    })) as unknown as Task[];
  }

  async function renderSwitcher(count: number) {
    const api = MockApi.withSampleData();
    api.delayMs = 0;
    const onSelect = vi.fn();
    render(
      <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
        <ApiProvider api={api}>
          <I18nProvider setting="es">
            <TaskSwitcher tasks={tasks(count)} current="t2" accounts={(await api.overview()).accounts} onSelect={onSelect} />
          </I18nProvider>
        </ApiProvider>
      </QueryClientProvider>,
    );
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: `Tareas (${count})` }));
    return { user, onSelect, list: screen.getByRole("group", { name: "Tareas de este proyecto" }) };
  }

  it("shows from nine tasks and unfolds the finished ones that match", async () => {
    const { user, onSelect, list } = await renderSwitcher(9);
    const filter = within(list).getByRole("searchbox", { name: "Filtrar tareas" });
    expect(within(list).getAllByRole("button", { name: /^Tarea número/ })).toHaveLength(8);
    const done = within(list).getByRole("button", { name: "Terminadas (1)" });
    expect(within(list).queryByRole("button", { name: /^Sube la versión/ })).toBeNull();

    await user.type(filter, "versión");
    expect(done).not.toBeInTheDocument();
    const first = within(within(list).getByRole("region", { name: "Terminadas (1)" })).getByRole("button", { name: /^Sube la versión/ });
    expect(within(first).getByText("Ctrl+1")).toBeInTheDocument();
    expect(within(list).queryByRole("button", { name: /^Tarea número/ })).toBeNull();

    await user.clear(filter);
    await user.type(filter, "nada así");
    expect(within(list).getByText("Ninguna tarea coincide.")).toBeInTheDocument();

    await user.clear(filter);
    await user.type(filter, "número 4");
    await user.click(within(list).getByRole("button", { name: /^Tarea número 4/ }));
    expect(onSelect).toHaveBeenCalledWith("t4");
  });

  it("has no filter under nine tasks", async () => {
    const { list } = await renderSwitcher(8);
    expect(within(list).queryByRole("searchbox")).toBeNull();
  });
});
