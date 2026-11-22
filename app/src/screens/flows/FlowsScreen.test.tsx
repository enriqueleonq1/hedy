import { QueryClient } from "@tanstack/react-query";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";
import { chooseOption } from "../../test/select";

// xterm needs a real canvas; no terminal is opened here.
vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

const TIENDA = "/code/tienda";
const HEDY = "/code/hedy";
const flowsOf = (project: string) => `#pf=${encodeURIComponent(project)}`;

async function setup(hash = flowsOf(TIENDA), api = MockApi.withSampleData()) {
  api.delayMs = 0;
  history.replaceState(null, "", hash);
  render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
  return { api, user: userEvent.setup() };
}

/** A step on the canvas, by its name. */
const step = (name: string | RegExp) => screen.findByRole("button", { name, pressed: false }, { timeout: 5000 });
const picker = () => screen.getByRole("combobox", { name: "Flujo" });
const panel = () => screen.getByRole("complementary", { name: "Detalles" });
/** "Guardado", "Cambios sin guardar"… beside the save button. */
const saveState = () => document.querySelector(".flows-bar__status") as HTMLElement;

afterEach(() => {
  history.replaceState(null, "", "#");
  sessionStorage.clear();
  localStorage.clear();
});

describe("A project's flows", () => {
  it("opens the project's own flow on the canvas, from the project's tabs", async () => {
    const { user } = await setup(`#ps=${encodeURIComponent(TIENDA)}`);
    await user.click(await screen.findByRole("button", { name: "Flujos" }, { timeout: 5000 }));
    expect(window.location.hash).toBe(flowsOf(TIENDA));
    await step("Arquitecto");
    expect(picker()).toHaveTextContent("Pago con revisión");
    for (const name of ["Inicio", "Programador", "¿Pasan los tests?", "Revisor", "¿Lo aprueban?", "Tu aprobación", "Final"]) {
      expect(await step(name)).toBeInTheDocument();
    }
    // The ways back, under the row.
    expect(screen.getByRole("button", { name: "fallan · máx. 2" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "piden cambios · máx. 2" })).toBeInTheDocument();
    expect(saveState()).toHaveTextContent("Guardado");
  });

  it("a template is read-only until duplicated, and the copy saves as the project's", async () => {
    const { api, user } = await setup(flowsOf(HEDY));
    // No flow of its own: the first template opens.
    await step("Programador con tests");
    expect(picker()).toHaveTextContent("Esencial");
    expect(screen.getByText("Es una plantilla de Hedy. Duplícala para cambiarla.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Agregar paso" })).not.toBeInTheDocument();
    await user.click(screen.getAllByRole("button", { name: "Duplicar para editar" })[0]!);
    expect(saveState()).toHaveTextContent("Cambios sin guardar");
    await user.click(screen.getByRole("button", { name: "Guardar" }));
    await waitFor(() => expect(saveState()).toHaveTextContent("Guardado"));
    const mine = (await api.flows(HEDY)).filter((f) => f.scope === "project");
    expect(mine.map((f) => f.flow.name)).toEqual(["Esencial (copia)"]);
    expect(window.location.hash).toBe(`${flowsOf(HEDY)}&${encodeURIComponent(mine[0]!.key)}`);
  });

  it("adds a step after the selected one, edits it in the panel, and undoes it", async () => {
    const { user } = await setup();
    await user.click(await step("Programador"));
    expect(within(panel()).getByRole("heading", { name: "Programador" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Agregar paso" }));
    await user.click(screen.getByRole("menuitem", { name: /Seguridad/ }));
    // The new step is selected, between the programmer and the tests.
    expect(within(panel()).getByRole("heading", { name: "Seguridad" })).toBeInTheDocument();
    expect(within(panel()).getByRole("combobox", { name: "Sigue con" })).toHaveTextContent("¿Pasan los tests?");
    await user.type(within(panel()).getByRole("textbox", { name: "Modelo" }), "haiku");
    expect(saveState()).toHaveTextContent("Cambios sin guardar");
    // One undo takes the typing back, the next one the step.
    await user.click(document.body);
    await user.keyboard("{Control>}z{/Control}");
    await user.keyboard("{Control>}z{/Control}");
    await waitFor(() => expect(screen.queryByRole("button", { name: "Seguridad" })).not.toBeInTheDocument());
    expect(saveState()).toHaveTextContent("Guardado");
  });

  it("suggests a step's models with their cost, and notes a second AI reviewer without blocking", async () => {
    const { user } = await setup();
    await user.click(await step("Programador"));
    const models = within(panel()).getByRole("group", { name: "Modelos sugeridos" });
    expect(within(models).getByRole("button", { name: "Sonnet", pressed: true })).toBeInTheDocument();
    expect(within(panel()).getByText("Equilibrado · $$")).toBeInTheDocument();
    await user.click(within(models).getByRole("button", { name: "Haiku" }));
    expect(within(panel()).getByRole("textbox", { name: "Modelo" })).toHaveValue("haiku");
    expect(within(panel()).getByText("El más rápido · $")).toBeInTheDocument();
    expect(screen.queryByRole("note")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Agregar paso" }));
    await user.click(screen.getByRole("menuitem", { name: /Seguridad/ }));
    expect(screen.getByRole("note")).toHaveTextContent("2 revisores de IA");
    expect(screen.getByRole("button", { name: "Guardar" })).toBeEnabled();
  });

  it("says when the open flow cost much more than one agent in this project, until told not to", async () => {
    const api = MockApi.withSampleData();
    api.flowValueSamples[TIENDA] = [
      { flow: "Pago con revisión", runs: 12, accepted: 10, tokens: 480_000, aloneRuns: 20, aloneAccepted: 17, aloneTokens: 120_000 },
    ];
    const { user } = await setup(flowsOf(TIENDA), api);
    await step("Arquitecto");
    const note = await screen.findByText(/En tus últimas 12 tareas con este flujo se gastó 4,0×/);
    expect(note).toHaveTextContent("aceptaste 10 de 12 (con un agente, 17 de 20)");
    await user.click(screen.getByRole("button", { name: "Ver Esencial" }));
    await waitFor(() => expect(picker()).toHaveTextContent("Esencial"));
    expect(screen.queryByText(/se gastó 4,0×/)).not.toBeInTheDocument();
    await chooseOption(user, picker(), "Pago con revisión");
    await user.click(await screen.findByRole("button", { name: "No volver a avisar" }));
    expect(screen.queryByText(/se gastó 4,0×/)).not.toBeInTheDocument();
    expect(localStorage.getItem("hedy.flows.valueQuiet")).toContain("Pago con revisión");
  });

  it("shows what keeps a flow from running on its step, and only saves once it can run", async () => {
    const { user } = await setup();
    await user.click(await step("Revisor"));
    await user.click(within(panel()).getByRole("button", { name: "Quitar este paso" }));
    // The verdict check lost its reviewer: marked on the step and listed.
    const problem = await screen.findByRole("button", { name: "1 por resolver" });
    expect(screen.getByRole("button", { name: "Guardar" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Ejecutar" })).toBeDisabled();
    await user.click(problem);
    expect(within(panel()).getByText(/la condición «¿Lo aprueban\?» decide por un veredicto/)).toBeInTheDocument();
    // Making it check the tests instead fixes it.
    await user.click(within(panel()).getByRole("button", { name: /¿Lo aprueban\?/ }));
    await chooseOption(user, within(panel()).getByRole("combobox", { name: "Qué comprueba" }), /¿Sigue las reglas\?/);
    await waitFor(() => expect(screen.queryByRole("button", { name: "1 por resolver" })).not.toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Guardar" })).toBeEnabled();
  });

  it("a saved flow is saved, in whatever order and shape the backend gives it back", async () => {
    const { api, user } = await setup();
    const save = api.saveFlow.bind(api);
    // The backend orders keys its own way and leaves empty fields out.
    const reorder = (v: unknown): unknown =>
      Array.isArray(v)
        ? v.map(reorder)
        : v && typeof v === "object"
          ? Object.fromEntries(
              Object.entries(v)
                .filter(([, x]) => x !== "" && !(Array.isArray(x) && x.length === 0))
                .reverse()
                .map(([k, x]) => [k, reorder(x)]),
            )
          : v;
    vi.spyOn(api, "saveFlow").mockImplementation(async (p, k, f) => {
      const entry = await save(p, k, f);
      return { ...entry, flow: reorder(entry.flow) as typeof entry.flow };
    });
    await user.click(await step("Programador"));
    await user.type(within(panel()).getByRole("textbox", { name: "Modelo" }), "-x");
    await user.clear(within(panel()).getByRole("textbox", { name: "Modelo" }));
    await user.type(within(panel()).getByRole("textbox", { name: "Modelo" }), "haiku");
    await user.click(screen.getByRole("button", { name: "Guardar" }));
    await waitFor(() => expect(saveState()).toHaveTextContent("Guardado"));
    expect(screen.getByRole("button", { name: "Ejecutar" })).toBeEnabled();
  });

  it("deleting a flow never leaves it open", async () => {
    const { api, user } = await setup();
    await step("Arquitecto");
    await user.click(screen.getByRole("button", { name: "Borrar" }));
    await user.click(screen.getByRole("button", { name: "¿Borrar el flujo?" }));
    await waitFor(() => expect(picker()).toHaveTextContent("Esencial"));
    expect((await api.flows(TIENDA)).some((f) => f.scope === "project")).toBe(false);
    expect(window.location.hash).toBe(flowsOf(TIENDA));
  });

  it("a new flow saves without the one before coming back", async () => {
    const { user } = await setup();
    await step("Arquitecto");
    await user.click(screen.getByRole("button", { name: "Nuevo flujo" }));
    expect(picker()).toHaveTextContent("Mi flujo");
    await user.click(screen.getByRole("button", { name: "Guardar" }));
    await waitFor(() => expect(saveState()).toHaveTextContent("Guardado"));
    expect(picker()).toHaveTextContent("Mi flujo");
    expect(screen.queryByRole("button", { name: "Arquitecto" })).not.toBeInTheDocument();
  });

  it("turns on following a task's end, and saves it", async () => {
    const { api, user } = await setup();
    await step("Arquitecto");
    // The sample flow already follows a task's end: turn it off and save.
    const toggle = within(panel()).getByRole("switch", { name: "Al terminar una tarea del proyecto" });
    expect(toggle).toHaveAttribute("aria-checked", "true");
    await user.click(toggle);
    await user.click(screen.getByRole("button", { name: "Guardar" }));
    await waitFor(() => expect(saveState()).toHaveTextContent("Guardado"));
    const saved = (await api.flows(TIENDA)).find((f) => f.scope === "project");
    expect(saved?.flow.triggers).toEqual(["manual"]);
  });

  it("a team's flow runs only once this PC allows its command", async () => {
    const { api, user } = await setup();
    await step("Arquitecto");
    await chooseOption(user, picker(), /Revisión de seguridad/);
    expect(screen.getByText("Lo comparte Marta con el equipo. Duplícalo para cambiarlo.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Guardar" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Ejecutar" }));
    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByText("npm run audit")).toBeInTheDocument();
    await user.type(within(dialog).getByRole("textbox"), "Revisa el cobro");
    const start = within(dialog).getByRole("button", { name: "Empezar" });
    expect(start).toBeDisabled();
    await user.click(within(dialog).getByRole("checkbox", { name: "Permito estos comandos en este PC" }));
    const approve = vi.spyOn(api, "approveFlowCommand");
    await user.click(start);
    await waitFor(() => expect(approve).toHaveBeenCalledWith("npm run audit"));
    await waitFor(() => expect(window.location.hash).toMatch(/^#p=.*&t=/));
  });

  it("an admin shares a flow of theirs with the project's team", async () => {
    const { api, user } = await setup();
    await step("Arquitecto");
    await user.click(screen.getByRole("button", { name: "Compartir con Café Aurora" }));
    await waitFor(async () => expect((await api.flows(TIENDA)).some((f) => f.scope === "team" && f.flow.name === "Pago con revisión")).toBe(true));
    // Shared without anyone's account on its steps.
    const shared = (await api.flows(TIENDA)).find((f) => f.scope === "team" && f.flow.name === "Pago con revisión");
    expect(shared?.flow.nodes.every((n) => n.kind !== "role" || !n.account)).toBe(true);
  });

  it("reads as a numbered list, with the ways back", async () => {
    const { user } = await setup();
    await step("Arquitecto");
    await user.click(screen.getByRole("radio", { name: "Lista" }));
    const items = within(screen.getByRole("list", { name: "Pasos del flujo" })).getAllByRole("listitem");
    expect(items[0]).toHaveTextContent("1. Inicio");
    expect(items.at(-1)).toHaveTextContent("Final");
    expect(screen.getByText("Si falla, vuelve a Programador · máx. 2")).toBeInTheDocument();
    // A step opens on the canvas, selected.
    await user.click(screen.getByRole("button", { name: /Revisor/ }));
    expect(within(panel()).getByRole("heading", { name: "Revisor" })).toBeInTheDocument();
  });

  it("runs a saved flow as a task and opens it", async () => {
    const { api, user } = await setup();
    await step("Arquitecto");
    await user.click(screen.getByRole("button", { name: "Ejecutar" }));
    const dialog = screen.getByRole("dialog");
    await user.type(within(dialog).getByRole("textbox"), "Agrega el pago con tarjeta");
    const start = vi.spyOn(api, "startTask");
    await user.click(within(dialog).getByRole("button", { name: "Empezar" }));
    await waitFor(() => expect(start).toHaveBeenCalled());
    expect(start.mock.calls[0]![0]).toMatchObject({ project: TIENDA, prompt: "Agrega el pago con tarjeta", flow: "#1" });
    await waitFor(() => expect(window.location.hash).toMatch(/^#p=.*&t=/));
  });
});
