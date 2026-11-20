import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "./App";
import { MockApi } from "./api/MockApi";
import { chooseOption, optionLabels } from "./test/select";

function renderApp(api = MockApi.withSampleData()) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<App api={api} queryClient={queryClient} />);
  return { api, user: userEvent.setup() };
}

describe("Inicio", () => {
  afterEach(() => history.replaceState(null, "", "#"));

  it("greets the user and sums up what waits and the plan closest to its limit", async () => {
    renderApp();
    expect(await screen.findByRole("heading", { level: 1, name: "¡Hola, Alex!" })).toBeInTheDocument();
    // The sync conflict, the team's suggestions and the five proposals: seven things.
    expect(
      await screen.findByText(
        "7 cosas te esperan; las puedes responder aquí mismo. Claude personal ya va en el 83 % (esta semana).",
      ),
    ).toBeInTheDocument();
    // The week of sample usage: 166k tokens in hedy and 50k in tienda, all on
    // subscriptions, so no cost (the CLI's guess is not a bill).
    expect(
      await screen.findByText(/^En Hedy: 216k tokens esta semana; hedy fue el proyecto más activo\.$/),
    ).toBeInTheDocument();
    // Nothing open yet: no "Seguir en", only a new task.
    expect(screen.queryByRole("button", { name: /^Seguir en / })).not.toBeInTheDocument();
    // The old home is gone: no project cards, no "Para ti", no latest tasks, no "Nuevo" menu.
    expect(screen.queryByRole("article", { name: "hedy" })).not.toBeInTheDocument();
    expect(screen.queryByRole("list", { name: "Para ti" })).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Últimas tareas" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Empezar algo nuevo" })).not.toBeInTheDocument();
    // Version drift is a diagnosis for Settings, not something for home.
    expect(screen.queryByText(/0\.171\.0/)).not.toBeInTheDocument();
  });

  it("starts a new task in the project at hand, or picks up the latest one", async () => {
    const { user } = renderApp(MockApi.withSampleData().withSampleActivity());
    const resume = await screen.findByRole("button", { name: "Seguir en «Revisa los tests del carrito»" });
    expect(screen.getByRole("button", { name: "Nueva tarea" })).toHaveClass("btn--primary");
    // The latest open task's project gets the new task.
    await user.click(screen.getByRole("button", { name: "Nueva tarea" }));
    await waitFor(() => expect(window.location.hash).toBe(`#p=${encodeURIComponent("/code/tienda")}&new`));

    await user.click(screen.getByRole("button", { name: /^Inicio/ }));
    await user.click(await screen.findByRole("button", { name: "Seguir en «Revisa los tests del carrito»" }));
    expect(await screen.findByRole("heading", { name: "Revisa los tests del carrito" })).toBeInTheDocument();
    expect(resume).not.toBeInTheDocument();
  });

  it("shows each plan as one meter, the window closest to its limit, and opens its account", async () => {
    const { user } = renderApp();
    const plans = await screen.findByRole("region", { name: "Tus planes" });
    const work = within(plans).getByRole("button", { name: /^Claude trabajo/ });
    expect(within(work).getByRole("meter", { name: "Claude trabajo: Sesión actual (5 h) 34 %" })).toHaveAttribute(
      "aria-valuenow",
      "34",
    );
    expect(work).toHaveTextContent(/Sesión actual \(5 h\) · Se renueva (hoy|mañana|el .+) a las .+$/);
    // Near its limit: the figure and the meter warn.
    const personal = within(plans).getByRole("button", { name: /^Claude personal/ });
    const meter = within(personal).getByRole("meter", { name: "Claude personal: Esta semana 83 %" });
    expect(meter).toHaveClass("meter--warn");
    expect(within(personal).getByText("83 %")).toHaveClass("plan-row__pct--warn");
    // No rings any more, and no empty meter: a plan that could not be read says so, an API key pays as it goes.
    expect(plans.querySelector(".rings")).toBeNull();
    const chatgpt = within(plans).getByRole("button", { name: /^ChatGPT/ });
    expect(chatgpt).toHaveTextContent("Sin datos");
    expect(within(chatgpt).queryByRole("meter")).not.toBeInTheDocument();
    const kimi = await within(plans).findByRole("button", { name: "KimiPago por uso" });
    expect(within(kimi).queryByRole("meter")).not.toBeInTheDocument();

    await user.click(personal);
    expect(await screen.findByRole("article", { name: "Claude personal" })).toBeInTheDocument();
    expect(window.location.hash).toBe("#a=claude-personal");
  });

  it("puts tasks, proposals, sync conflicts and team suggestions in one inbox, with what is ready below", async () => {
    const { user } = renderApp(MockApi.withSampleData().withSampleActivity());
    const needsPill = await screen.findByRole("radio", { name: /^Te necesitan\s*10$/ });
    expect(needsPill).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("radio", { name: /^En marcha\s*1$/ })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: /^Listas\s*3$/ })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Historial" })).toBeInTheDocument();

    const needs = screen.getByRole("region", { name: "Te necesitan (10)" });
    // Answered right where they show.
    expect(within(needs).getByRole("button", { name: "Aprobar plan" })).toBeInTheDocument();
    expect(within(needs).getByRole("button", { name: "Permitir una vez" })).toBeInTheDocument();
    expect(within(needs).getByRole("button", { name: "Pasar a ChatGPT" })).toBeInTheDocument();
    expect(within(needs).getByText(/^Hedy nunca cambia de cuenta solo: /)).toBeInTheDocument();
    expect(within(needs).getByText("Los formularios validan con zod")).toBeInTheDocument();
    expect(within(needs).getByText("Un cambio en tus computadoras para resolver")).toBeInTheDocument();
    expect(within(needs).getByText("2 sugerencias del equipo Plataforma")).toBeInTheDocument();

    const ready = screen.getByRole("region", { name: "Listas para revisar (3)" });
    expect(within(ready).getByRole("button", { name: "Revisar cambios" })).toBeInTheDocument();
    expect(within(ready).getByText("¿Qué diferencia hay entre OAuth y OpenID Connect?")).toBeInTheDocument();

    // On the side, what is running.
    const side = screen.getByRole("complementary", { name: "Tus planes y lo que está en marcha" });
    expect(within(side).getByRole("button", { name: /Revisa los tests del carrito/ })).toBeInTheDocument();

    // Sync and teams are settled in Settings, on their cards.
    await user.click(within(needs).getByRole("button", { name: "Resolver" }));
    await waitFor(() => expect(window.location.hash).toBe("#sync"));
    await user.click(screen.getByRole("button", { name: /^Inicio/ }));
    await user.click(within(await screen.findByRole("region", { name: "Te necesitan (10)" })).getByRole("button", { name: "Revisar" }));
    await waitFor(() => expect(window.location.hash).toBe("#teams"));
  });

  it("filters by state, project and words, and keeps the history", async () => {
    const { user } = renderApp(MockApi.withSampleData().withSampleActivity());
    await screen.findByRole("region", { name: "Te necesitan (10)" });

    await user.click(screen.getByRole("radio", { name: /^En marcha\s*1$/ }));
    const running = screen.getByRole("region", { name: "En marcha (1)" });
    expect(within(running).getByText("Revisa los tests del carrito")).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: /^Te necesitan/ })).not.toBeInTheDocument();
    // The side card does not repeat the list on screen.
    expect(within(screen.getByRole("complementary", { name: /^Tus planes/ })).queryByRole("heading", { name: "En marcha" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("radio", { name: /^Listas\s*3$/ }));
    expect(screen.getByRole("radio", { name: /^Listas\s*3$/ })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("region", { name: "Listas (3)" })).toBeInTheDocument();

    // One project: the inbox and the counts follow.
    await user.click(screen.getByRole("radio", { name: /^Te necesitan/ }));
    await user.selectOptions(screen.getByRole("combobox", { name: "Proyecto" }), "tienda");
    const tienda = await screen.findByRole("region", { name: "Te necesitan (1)" });
    expect(within(tienda).getByText("Migra el reporte de comisiones a la API nueva")).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: /^Listas para revisar/ })).not.toBeInTheDocument();

    // Words, across the inbox.
    await user.selectOptions(screen.getByRole("combobox", { name: "Proyecto" }), "");
    await user.type(screen.getByRole("searchbox", { name: "Buscar tareas y pendientes" }), "zod");
    const found = await screen.findByRole("region", { name: "Te necesitan (1)" });
    expect(within(found).getByText("Los formularios validan con zod")).toBeInTheDocument();

    // Nothing matches: a calm empty state.
    await user.type(screen.getByRole("searchbox", { name: "Buscar tareas y pendientes" }), "xyz");
    expect(await screen.findByText("Nada te espera")).toBeInTheDocument();

    // The history lists every task, closed ones too.
    await user.clear(screen.getByRole("searchbox", { name: "Buscar tareas y pendientes" }));
    await user.click(screen.getByRole("radio", { name: "Historial" }));
    const history = await screen.findByRole("region", { name: "Historial" });
    expect(await within(history).findByRole("button", { name: /Revisa los tests del carrito/ })).toBeInTheDocument();
    expect(within(history).getByRole("button", { name: /OAuth y OpenID Connect/ })).toBeInTheDocument();
  });

  it("is also where the old Actividad address lands", async () => {
    history.replaceState(null, "", "#activity");
    renderApp(MockApi.withSampleData().withSampleActivity());
    expect(await screen.findByRole("heading", { level: 1, name: "¡Hola, Alex!" })).toBeInTheDocument();
    expect(await screen.findByRole("region", { name: "Te necesitan (10)" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Inicio: 10 cosas te esperan" })).toHaveAttribute("aria-current", "page");
  });

  it("invites to add an account when there are none", async () => {
    const { user } = renderApp(new MockApi());
    const add = await screen.findByRole("button", { name: "Agregar mi primera cuenta" });
    expect(screen.getByText("Primero agrega una cuenta; luego asígnala a tus proyectos.")).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Tus planes" })).not.toBeInTheDocument();
    expect(screen.queryByRole("radiogroup", { name: "Mostrar" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Nueva tarea" })).not.toBeInTheDocument();
    await user.click(add);
    expect(await screen.findByRole("heading", { name: "Tus cuentas y modelos" })).toBeInTheDocument();
  });

  it("offers the two ways to start when there are no projects", async () => {
    const api = MockApi.withSampleData();
    for (const p of (await api.overview()).projects.map((p) => p.path)) await api.removeProject(p);
    renderApp(api);
    await screen.findByRole("heading", { level: 1, name: "¡Hola, Alex!" });
    expect(screen.getByRole("button", { name: /^Nuevo proyecto.*Una carpeta nueva, lista para trabajar$/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^Abrir carpeta….*Un proyecto que ya tienes en tu equipo$/ })).toBeInTheDocument();
    // The header's primary action creates a project instead of a task.
    expect(screen.getByRole("button", { name: "Nuevo proyecto" })).toHaveClass("btn--primary");
    expect(screen.queryByRole("button", { name: "Nueva tarea" })).not.toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Tus planes" })).toBeInTheDocument();
  });
});

describe("Accounts", () => {
  async function openAccounts() {
    const ctx = renderApp();
    await ctx.user.click(await screen.findByRole("button", { name: /Abrir la cuenta$/ }));
    return ctx;
  }

  it("shows one card per account with its login state", async () => {
    await openAccounts();
    const card = screen.getByRole("article", { name: "Claude trabajo" });
    expect(within(card).getByText("Anthropic · Team")).toBeInTheDocument();
    expect(within(card).getByText("Conectada")).toBeInTheDocument();
    expect(within(card).getByText(/carpeta por defecto de la CLI/)).toBeInTheDocument();
    expect(within(card).getByRole("listitem")).toHaveTextContent("tienda");
    const personal = screen.getByRole("article", { name: "Claude personal" });
    expect(within(personal).getByText("182k tokens")).toBeInTheDocument();
    expect(within(personal).getByText("Tokens en Hedy esta semana")).toBeInTheDocument();
    const codex = screen.getByRole("article", { name: "ChatGPT" });
    expect(within(codex).getByRole("button", { name: "Iniciar sesión" })).toBeInTheDocument();
  });

  it("shows the plan's session and week from /usage, with thresholds and the source", async () => {
    const { user } = await openAccounts();
    const work = screen.getByRole("article", { name: "Claude trabajo" });
    expect(within(work).getByRole("meter", { name: "Sesión actual (5 h): 34%" })).toBeInTheDocument();
    expect(within(work).getByRole("meter", { name: "Esta semana: 29%" })).toBeInTheDocument();
    expect(within(work).getAllByText(/^Se renueva (hoy|mañana|el .+) a las .+$/)).toHaveLength(2);
    expect(within(work).getByText("según /usage de Claude Code · hace 2 min")).toBeInTheDocument();
    // A per-model window at 0 % is left out.
    expect(within(work).queryByText(/Fable/)).not.toBeInTheDocument();

    const personal = screen.getByRole("article", { name: "Claude personal" });
    expect(within(personal).getByText("83%")).toHaveClass("usage__value--warning");
    expect(within(personal).getByText("58%")).not.toHaveClass("usage__value--warning");
    expect(within(personal).getByText("Por modelo: Opus 12%")).toBeInTheDocument();

    // A source that failed keeps the last reading quiet: a muted note.
    const codex = screen.getByRole("article", { name: "ChatGPT" });
    expect(within(codex).getByText("no se pudo actualizar")).toBeInTheDocument();

    // The sidebar's foot shows the account in use (outside a project, the default one) with the
    // window closest to its limit: a name, a figure warning-colored only near the limit, and a meter.
    const sidebar = screen.getByRole("complementary", { name: "Proyectos y cuentas" });
    const personalRow = within(sidebar).getByRole("button", { name: "Claude personal: Semana · 83%. Abrir la cuenta" });
    expect(within(personalRow).getByText("83%")).toHaveClass("level--warning");
    expect(within(personalRow).getByRole("meter", { name: "Semana · 83%" })).toHaveAttribute("aria-valuenow", "83");
    // Its tooltip says how old the reading is.
    expect(personalRow).toHaveAttribute("title", "Claude personal · Semana · 83% · actualizado hace 2 min");
    // Only that account: the others are a click away, in their cards.
    expect(within(sidebar).queryByRole("button", { name: /^Claude trabajo: / })).not.toBeInTheDocument();
    // In a project, it is that project's account.
    await user.click(within(sidebar).getByRole("button", { name: /^tienda/ }));
    const workRow = await within(sidebar).findByRole("button", { name: "Claude trabajo: Sesión 5 h · 34%. Abrir la cuenta" });
    expect(within(workRow).getByText("34%")).not.toHaveClass("level--warning");
    // The status bar no longer repeats the account.
    const bar = screen.getByRole("contentinfo", { name: "Barra de estado" });
    expect(within(bar).queryByRole("button", { name: /Abrir la cuenta$/ })).not.toBeInTheDocument();
  });

  it("asks for fresh readings while the sidebar is on screen and says which providers do not report", async () => {
    const api = MockApi.withSampleData();
    await api.addAccount({ name: "Gemini", provider: "gemini", apiKey: false, defaultFolder: false, color: null });
    const refresh = vi.spyOn(api, "refreshPlanUsage");
    const { user } = renderApp(api);
    await waitFor(() => expect(refresh).toHaveBeenCalledWith("claude-trabajo"));
    expect(refresh).toHaveBeenCalledWith("claude-personal");
    expect(refresh).not.toHaveBeenCalledWith("gemini");
    await user.click(await screen.findByRole("button", { name: /Abrir la cuenta$/ }));
    const gemini = screen.getByRole("article", { name: "Gemini" });
    expect(within(gemini).getByText("Gemini aún no informa el límite del plan a Hedy")).toBeInTheDocument();
  });

  it("adds an API key account and stores its key", async () => {
    const { user } = await openAccounts();
    await user.click(screen.getByRole("button", { name: "+ Agregar cuenta" }));
    const dialog = screen.getByRole("dialog", { name: "Agregar cuenta" });
    await user.type(within(dialog).getByLabelText("Nombre"), "Gemini personal");
    await user.click(within(dialog).getByRole("radio", { name: /API key de Google/ }));
    await user.click(within(dialog).getByRole("button", { name: "Crear y guardar key" }));

    const keyDialog = await screen.findByRole("dialog", { name: "API key de Gemini personal" });
    await user.type(within(keyDialog).getByLabelText("API key"), "AIza-test");
    await user.click(within(keyDialog).getByRole("button", { name: "Guardar en el llavero" }));

    const card = await screen.findByRole("article", { name: "Gemini personal" });
    await waitFor(() => expect(within(card).getByText(/Key guardada/)).toBeInTheDocument());
  });

  it("warns that Gemini CLI does not accept personal accounts", async () => {
    const { user } = await openAccounts();
    await user.click(screen.getByRole("button", { name: "+ Agregar cuenta" }));
    await user.click(screen.getByRole("radio", { name: /Gemini CLI/ }));
    expect(screen.getByText(/ya no acepta cuentas personales/)).toBeInTheDocument();
  });

  it("does not offer a protected project's account to other projects", async () => {
    const api = MockApi.withSampleData();
    await api.useAccount("/code/hobby", "claude-personal");
    const { user } = renderApp(api);
    await user.click(await screen.findByRole("button", { name: /Abrir la cuenta$/ }));

    await user.click(await screen.findByRole("combobox", { name: "Cuenta de hobby" }));
    const options = optionLabels();
    expect(options).not.toContain("trabajo");
    expect(options).toContain("personal");
    await user.keyboard("{Escape}");
    expect(screen.getByRole("combobox", { name: "Cuenta de tienda" })).toBeDisabled();
  });
});

describe("Models per phase", () => {
  it("assigns phases to other providers and summarises them", async () => {
    const { user } = renderApp();
    await user.click(await screen.findByRole("button", { name: /Abrir la cuenta$/ }));
    const edit = screen.getByRole("button", { name: "Modelos por fase de hedy" });
    expect(edit).toHaveTextContent(/^Claude$/);
    await user.click(edit);

    const dialog = screen.getByRole("dialog", { name: "Modelos por fase · hedy" });
    await chooseOption(user, within(dialog).getByLabelText("Cuenta para Implementar"), "ChatGPT");
    await chooseOption(user, within(dialog).getByLabelText("Cuenta para Escribir tests"), "Kimi");
    await user.type(within(dialog).getByLabelText("Modelo para Escribir tests"), "kimi-k3");
    await user.click(within(dialog).getByRole("button", { name: "Guardar" }));

    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Modelos por fase de hedy" })).toHaveTextContent(
        "Claude planifica · Codex implementa · Kimi tests",
      ),
    );
  });

  it("turns an optional phase off even after a model was typed", async () => {
    const { api, user } = renderApp();
    await user.click(await screen.findByRole("button", { name: /Abrir la cuenta$/ }));
    await user.click(screen.getByRole("button", { name: "Modelos por fase de hedy" }));

    const dialog = screen.getByRole("dialog", { name: "Modelos por fase · hedy" });
    await chooseOption(user, within(dialog).getByLabelText("Cuenta para Revisar"), "Kimi");
    await user.type(within(dialog).getByLabelText("Modelo para Revisar"), "kimi-k3");
    await chooseOption(user, within(dialog).getByLabelText("Cuenta para Revisar"), "No usar esta fase");
    expect(within(dialog).getByLabelText("Modelo para Revisar")).toHaveValue("");
    await user.click(within(dialog).getByRole("button", { name: "Guardar" }));

    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect((await api.overview()).projects.find((p) => p.name === "hedy")?.phases).toEqual({});
  });
});

describe("Settings", () => {
  it("changes the theme", async () => {
    const { user } = renderApp();
    await user.click(await screen.findByRole("button", { name: "Ajustes" }));
    await user.click(screen.getByRole("radio", { name: "Claro" }));
    await waitFor(() => expect(document.documentElement.dataset.theme).toBe("light"));
    expect(screen.getByRole("radio", { name: "Claro" })).toHaveAttribute("aria-checked", "true");
  });

  it("lists the CLI versions in the diagnosis, with a pointer to hedy doctor", async () => {
    const { user } = renderApp();
    await user.click(await screen.findByRole("button", { name: "Ajustes" }));
    expect(screen.getByRole("heading", { name: "Diagnóstico" })).toBeInTheDocument();
    expect(screen.getByText("hedy doctor")).toBeInTheDocument();
    const diagnosis = await screen.findByRole("list", { name: "Resultado del diagnóstico" });
    const codex = within(diagnosis).getByText("Codex").closest("li")!;
    expect(codex).toHaveTextContent(/0\.169\.0 → 0\.171\.0/);
    expect(within(codex).getByRole("img", { name: "Aviso" })).toBeInTheDocument();
    // What is fine and outside the CLIs stays in `hedy doctor`.
    expect(within(diagnosis).queryByText("git")).not.toBeInTheDocument();
  });

  it("turns task notifications off and on", async () => {
    const { api, user } = renderApp();
    await user.click(await screen.findByRole("button", { name: "Ajustes" }));
    expect(screen.getByRole("heading", { name: "Avisos" })).toBeInTheDocument();
    const toggle = screen.getByRole("switch", { name: "Avisarme cuando una tarea me necesita o termina" });
    expect(toggle).toHaveAttribute("aria-checked", "true");
    await user.click(toggle);
    await waitFor(() => expect(toggle).toHaveAttribute("aria-checked", "false"));
    expect((await api.overview()).notifications).toBe(false);
    await user.click(toggle);
    await waitFor(() => expect(toggle).toHaveAttribute("aria-checked", "true"));
  });

  it("turns context saving off and explains how it works", async () => {
    const { api, user } = renderApp();
    await user.click(await screen.findByRole("button", { name: "Ajustes" }));
    expect(screen.getByRole("heading", { name: "Ahorro" })).toBeInTheDocument();
    const toggle = screen.getByRole("switch", { name: "Gastar menos en cada tarea" });
    expect(toggle).toHaveAttribute("aria-checked", "true");
    await user.click(toggle);
    await waitFor(() => expect(toggle).toHaveAttribute("aria-checked", "false"));
    expect((await api.overview()).contextSaving).toBe(false);

    const how = screen.getByRole("button", { name: "Cómo funciona" });
    expect(how).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText(/Memoria enfocada/)).not.toBeVisible();
    await user.click(how);
    expect(how).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText(/Memoria enfocada/)).toBeVisible();
  });

  it("leaves context saving to HEDY_EXPERIMENTS when it is set", async () => {
    const { user } = renderApp(new MockApi({ contextSavingByEnv: true }));
    await user.click(await screen.findByRole("button", { name: "Ajustes" }));
    expect(screen.getByRole("switch", { name: "Gastar menos en cada tarea" })).toBeDisabled();
    expect(screen.getByText("Ahora lo decide la variable HEDY_EXPERIMENTS.")).toBeInTheDocument();
  });

  it("previews the four mascot states", async () => {
    const { user } = renderApp();
    await user.click(await screen.findByRole("button", { name: "Ajustes" }));
    const gallery = screen.getByLabelText("Estados de la mascota");
    for (const name of ["Durmiendo", "Pensando", "Trabajando", "¡Listo!"]) {
      expect(within(gallery).getByRole("img", { name })).toBeInTheDocument();
    }
    const working = screen.getByLabelText("La mascota trabajando, según lo que hace la IA");
    expect(within(working).getAllByRole("img")).toHaveLength(10);
    for (const name of ["Leyendo archivos", "Corriendo las pruebas", "Repartiendo con subagentes", "Lleva un buen rato"]) {
      expect(within(working).getByRole("img", { name })).toBeInTheDocument();
    }
    expect(within(working).getByText("Sudando la gota · Sin rendirse · Dándolo todo")).toBeInTheDocument();
  });
});
