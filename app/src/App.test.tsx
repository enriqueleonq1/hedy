import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
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

    const codex = screen.getByRole("article", { name: "ChatGPT" });
    // Signed out: no plan figures at all.
    expect(within(codex).queryByText(/no se pudo actualizar/)).not.toBeInTheDocument();
    expect(within(codex).queryByRole("meter")).not.toBeInTheDocument();

    // The sidebar's foot shows the account in use (outside a project, the default one) with the
    // window closest to its limit: a name, a figure warning-colored only near the limit, and a meter.
    const sidebar = screen.getByRole("complementary", { name: "Navegación" });
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
    await user.click(within(dialog).getByRole("button", { name: /^Con una API key/ }));
    await user.click(within(dialog).getByRole("radio", { name: /API key de Google/ }));
    await user.type(within(dialog).getByLabelText("Nombre"), "Gemini personal");
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
    await user.click(screen.getByRole("button", { name: /^Con tu suscripción/ }));
    await user.click(screen.getByRole("radio", { name: /Gemini CLI/ }));
    expect(screen.getByText(/ya no acepta cuentas personales/)).toBeInTheDocument();
  });

  it("does not offer a protected project's account to other projects", async () => {
    const api = MockApi.withSampleData();
    await api.useAccount("/code/hobby", "claude-personal");
    history.replaceState(null, "", `#ps=${encodeURIComponent("/code/hobby")}`);
    const { user } = renderApp(api);

    await user.click(await screen.findByRole("combobox", { name: "Cuenta del proyecto" }, { timeout: 5000 }));
    const options = optionLabels();
    expect(options).not.toContain("Claude trabajo");
    expect(options).toContain("Claude personal");
    await user.keyboard("{Escape}");
    history.replaceState(null, "", "#");
  });
});

describe("Who does each step", () => {
  afterEach(() => history.replaceState(null, "", "#"));

  it("is set in the project's sheet and summed up in Cuentas", async () => {
    const { user } = renderApp();
    await user.click(await screen.findByRole("button", { name: /Abrir la cuenta$/ }));
    const row = () => within(screen.getByRole("table", { name: "Proyectos y cuentas" })).getByRole("row", { name: /^hedy/ });
    expect(within(row()).getAllByRole("cell").at(-1)).toHaveTextContent(/^Claude$/);

    await user.click(within(row()).getByRole("link", { name: "hedy" }));
    const steps = (await screen.findByRole("heading", { level: 2, name: "Quién hace cada paso" })).closest("section")!;
    await chooseOption(user, within(steps).getByRole("combobox", { name: "Quién hace «Implementar»" }), "ChatGPT");
    await waitFor(() => expect(within(steps).getByRole("combobox", { name: "Quién hace «Implementar»" })).toHaveTextContent("ChatGPT"));
    await chooseOption(user, within(steps).getByRole("combobox", { name: "Quién hace «Verificar»" }), "Kimi");
    await waitFor(() => expect(within(steps).getByRole("combobox", { name: "Quién hace «Verificar»" })).toHaveTextContent("Kimi"));

    history.back();
    await waitFor(() =>
      expect(within(row()).getAllByRole("cell").at(-1)).toHaveTextContent("Claude planifica · Codex implementa · Kimi verifica"),
    );
  });
});

describe("Settings", () => {
  afterEach(() => history.replaceState(null, "", "#"));

  /** The app opened at that address. */
  function renderAt(hash: string, api?: MockApi) {
    history.replaceState(null, "", hash);
    return renderApp(api);
  }

  const index = () => screen.getByRole("navigation", { name: "Secciones de Ajustes" });
  const indexItems = () => within(index()).queryAllByRole("button").map((b) => b.textContent);
  const current = () => within(index()).getByRole("button", { current: "page" });

  it("opens General from the sidebar, with an index of its sections in groups", async () => {
    const { user } = renderApp();
    const sidebar = await screen.findByRole("complementary", { name: "Navegación" });
    await user.click(within(sidebar).getByRole("button", { name: "Ajustes" }));
    expect(await screen.findByRole("heading", { level: 1, name: "General" })).toBeInTheDocument();
    expect(window.location.hash).toBe("#settings");
    expect(within(sidebar).getByRole("button", { name: "Ajustes" })).toHaveAttribute("aria-current", "page");

    expect(indexItems()).toEqual([
      "General",
      "Cuentas",
      "Apariencia",
      "Ahorro de contexto",
      "Plantillas",
      "Sincronización",
      "Equipos",
      "Avanzado",
    ]);
    const groups = index().querySelectorAll(".settings__index-group");
    expect(groups).toHaveLength(3);
    expect(groups[1]).toHaveTextContent(/^Entre computadoras/);
    expect(within(groups[1] as HTMLElement).getAllByRole("button").map((b) => b.textContent)).toEqual(["Sincronización", "Equipos"]);
    expect(groups[2]).toHaveTextContent(/^Más/);
    expect(within(groups[2] as HTMLElement).getAllByRole("button").map((b) => b.textContent)).toEqual(["Avanzado"]);
    expect(current()).toHaveTextContent("General");
    expect(within(index()).getAllByRole("button", { current: "page" })).toHaveLength(1);

    // Only one section at a time.
    expect(screen.queryByRole("radiogroup", { name: "Modo" })).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Diagnóstico" })).not.toBeInTheDocument();
  });

  it("goes to each section from its index", async () => {
    const { user } = renderAt("#settings");
    await screen.findByRole("heading", { level: 1, name: "General" });
    const go = async (name: string) => user.click(within(index()).getByRole("button", { name }));

    await go("Avanzado");
    expect(await screen.findByRole("heading", { level: 1, name: "Avanzado" })).toBeInTheDocument();
    expect(window.location.hash).toBe("#settings/advanced");
    expect(current()).toHaveTextContent("Avanzado");

    await go("Cuentas");
    expect(await screen.findByRole("heading", { level: 1, name: "Tus cuentas y modelos" })).toBeInTheDocument();
    expect(window.location.hash).toBe("#accounts");
    expect(current()).toHaveTextContent("Cuentas");

    await go("Sincronización");
    expect(await screen.findByRole("heading", { level: 1, name: "Sincronización" })).toBeInTheDocument();
    expect(window.location.hash).toBe("#sync");

    await go("General");
    expect(await screen.findByRole("heading", { level: 1, name: "General" })).toBeInTheDocument();
    expect(window.location.hash).toBe("#settings");
  });

  it.each([
    ["#settings", "General", "General"],
    ["#settings/general", "General", "General"],
    ["#settings/appearance", "Apariencia", "Apariencia"],
    ["#settings/saving", "Ahorro de contexto", "Ahorro de contexto"],
    ["#settings/themes", "Apariencia", "Apariencia"],
    ["#settings/templates", "Plantillas", "Plantillas"],
    ["#settings/advanced", "Avanzado", "Avanzado"],
    ["#sync", "Sincronización", "Sincronización"],
    ["#teams", "Equipos", "Equipos"],
    ["#accounts", "Cuentas", "Tus cuentas y modelos"],
    ["#a=claude-trabajo", "Cuentas", "Tus cuentas y modelos"],
    ["#settings/nothing-here", "General", "General"],
  ])("lands %s on its section", async (hash, item, title) => {
    renderAt(hash);
    expect(await screen.findByRole("heading", { level: 1, name: title })).toBeInTheDocument();
    expect(screen.getAllByRole("heading", { level: 1 })).toHaveLength(1);
    expect(current()).toHaveTextContent(item);
  });

  it("filters the index by section names and what is inside them", async () => {
    const { user } = renderAt("#settings");
    await screen.findByRole("heading", { level: 1, name: "General" });
    const search = within(index()).getByRole("searchbox", { name: "Buscar en Ajustes" });

    await user.type(search, "apar");
    expect(indexItems()).toEqual(["Apariencia"]);
    expect(within(index()).queryByText("Entre computadoras")).not.toBeInTheDocument();

    // Words of what a section holds find it too.
    await user.clear(search);
    await user.type(search, "mascota");
    expect(indexItems()).toEqual(["Apariencia"]);
    await user.clear(search);
    await user.type(search, "diagnóstico");
    expect(indexItems()).toEqual(["Avanzado"]);
    expect(within(index()).getByText("Más")).toBeInTheDocument();
    await user.clear(search);
    await user.type(search, "mapa del código");
    expect(indexItems()).toEqual(["Ahorro de contexto"]);
    await user.clear(search);
    await user.type(search, "IDIOMA");
    expect(indexItems()).toEqual(["General"]);


    // Common words, without accents or case, and the groups' titles.
    const finds = async (word: string, items: string[]) => {
      await user.clear(search);
      await user.type(search, word);
      expect(indexItems()).toEqual(items);
    };
    await finds("diagnostico", ["Avanzado"]);
    await finds("tema", ["Apariencia"]);
    await finds("precio", ["Cuentas", "Avanzado"]);
    await finds("contexto", ["Ahorro de contexto"]);
    await finds("invitacion", ["Equipos"]);
    await finds("conflicto", ["Sincronización"]);
    await finds("prompt", ["Plantillas"]);
    await finds("proteger", ["Cuentas"]);
    await finds("notificaciones", ["General"]);
    await finds("entre computadoras", ["Sincronización", "Equipos"]);

    await user.clear(search);
    await user.type(search, "zzz");
    expect(indexItems()).toEqual([]);
    expect(within(index()).getByText("Nada coincide.")).toBeInTheDocument();
    // The section on screen stays while searching.
    expect(screen.getByRole("heading", { level: 1, name: "General" })).toBeInTheDocument();

    await user.clear(search);
    expect(indexItems()).toHaveLength(8);
    expect(within(index()).queryByText("Nada coincide.")).not.toBeInTheDocument();

    // A result opens its section.
    await user.type(search, "plant");
    await user.click(within(index()).getByRole("button", { name: "Plantillas" }));
    expect(await screen.findByRole("heading", { level: 1, name: "Plantillas" })).toBeInTheDocument();
  });

  it("puts focus on the section's title on arrival, and leaves it in the index while moving through it", async () => {
    const { user } = renderAt("#sync");
    const title = await screen.findByRole("heading", { level: 1, name: "Sincronización" });
    await waitFor(() => expect(title).toHaveFocus());

    const teams = within(index()).getByRole("button", { name: "Equipos" });
    await user.click(teams);
    expect(await screen.findByRole("heading", { level: 1, name: "Equipos" })).not.toHaveFocus();
    expect(teams).toHaveFocus();
    await user.click(within(index()).getByRole("button", { name: "Sincronización" }));
    expect(await screen.findByRole("heading", { level: 1, name: "Sincronización" })).not.toHaveFocus();
  });

  it("opens Apariencia at the theme gallery from #settings/themes", async () => {
    renderAt("#settings/themes");
    expect(await screen.findByRole("heading", { level: 1, name: "Apariencia" })).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("heading", { level: 2, name: "Temas" })).toHaveFocus());
  });

  it("keeps each section's headings in order: h1, then h2, then h3", async () => {
    const levels = () =>
      [...document.querySelectorAll(".settings__content :is(h1, h2, h3, h4)")].map((h) => Number(h.tagName[1]));
    const inOrder = (list: number[]) => list.every((level, i) => i === 0 || level <= list[i - 1]! + 1);
    for (const hash of ["#settings/appearance", "#settings/templates", "#sync", "#teams", "#settings/advanced"]) {
      renderAt(hash);
      await screen.findByRole("heading", { level: 1 });
      await waitFor(() => expect(levels().length).toBeGreaterThan(0));
      expect(levels()[0]).toBe(1);
      expect(inOrder(levels())).toBe(true);
      cleanup();
    }
  });

  it("points to each API account's Avanzado for the APIs' addresses and prices", async () => {
    const { user } = renderAt("#settings/advanced");
    await screen.findByRole("heading", { name: "APIs" });
    expect(screen.queryByRole("textbox", { name: "Kimi: dirección" })).not.toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: "Gemini: precio" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Ir a Cuentas" }));
    expect(await screen.findByRole("heading", { level: 1, name: "Tus cuentas y modelos" })).toBeInTheDocument();
    expect(window.location.hash).toBe("#accounts");
  });

  it("changes the theme", async () => {
    const { user } = renderAt("#settings/appearance");
    expect(await screen.findByRole("heading", { level: 1, name: "Apariencia" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Temas" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Colores de cuenta" })).toBeInTheDocument();
    await user.click(screen.getByRole("radio", { name: "Claro" }));
    await waitFor(() => expect(document.documentElement.dataset.theme).toBe("light"));
    expect(screen.getByRole("radio", { name: "Claro" })).toHaveAttribute("aria-checked", "true");
  });

  it("lists the CLI versions in the diagnosis, with a pointer to hedy doctor", async () => {
    renderAt("#settings/advanced");
    expect(await screen.findByRole("heading", { name: "Diagnóstico" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Rutas de las CLIs" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "APIs" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Datos" })).toBeInTheDocument();
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
    expect(await screen.findByRole("heading", { level: 1, name: "General" })).toBeInTheDocument();
    expect(screen.getByLabelText("Tu nombre")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Ver la guía de bienvenida" })).toBeInTheDocument();
    const toggle = screen.getByRole("switch", { name: "Avisarme cuando una tarea me necesita o termina" });
    expect(toggle).toHaveAttribute("aria-checked", "true");
    await user.click(toggle);
    await waitFor(() => expect(toggle).toHaveAttribute("aria-checked", "false"));
    expect((await api.overview()).notifications).toBe(false);
    await user.click(toggle);
    await waitFor(() => expect(toggle).toHaveAttribute("aria-checked", "true"));
  });

  it("turns context saving off and explains how it works", async () => {
    const { api, user } = renderAt("#settings/saving");
    expect(await screen.findByRole("heading", { level: 1, name: "Ahorro de contexto" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Mapa del código" })).toBeInTheDocument();
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
    renderAt("#settings/saving", new MockApi({ contextSavingByEnv: true }));
    expect(await screen.findByRole("switch", { name: "Gastar menos en cada tarea" })).toBeDisabled();
    expect(screen.getByText("Ahora lo decide la variable HEDY_EXPERIMENTS.")).toBeInTheDocument();
  });

  it("previews the four mascot states behind Ver la mascota", async () => {
    const { user } = renderAt("#settings/appearance");
    const show = await screen.findByRole("button", { name: "Ver la mascota" });
    expect(show).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByLabelText("Estados de la mascota")).not.toBeInTheDocument();
    await user.click(show);
    const hide = screen.getByRole("button", { name: "Ocultar la mascota" });
    expect(hide).toHaveAttribute("aria-expanded", "true");

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

    await user.click(hide);
    expect(screen.queryByLabelText("Estados de la mascota")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Ver la mascota" })).toBeInTheDocument();
  });
});
