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

describe("Welcome", () => {
  it("greets the user and lists projects with their accounts", async () => {
    renderApp();
    expect(await screen.findByRole("heading", { name: /¡Hola, Enrique!/ })).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "Hedy te saluda" })).toHaveAttribute("data-state", "idle");
    const homming = screen.getByRole("heading", { name: "homming" }).closest("article")!;
    expect(within(homming).getByText("Claude trabajo")).toBeInTheDocument();
    expect(within(homming).getByText("Protegido")).toBeInTheDocument();
    // The week of sample usage: 166k tokens in hedy and 50k in homming.
    expect(
      await screen.findByText(/^En Hedy: 216k tokens esta semana \(unos \$4,1\d\); hedy fue el proyecto más activo\.$/),
    ).toBeInTheDocument();
  });

  it("shows each plan as rings, what needs a look and the latest tasks", async () => {
    renderApp(MockApi.withSampleData().withSampleActivity());
    const plans = (await screen.findByRole("heading", { name: "Tus planes" })).closest("section")!;
    const work = within(plans).getByRole("button", { name: "Claude trabajo: Sesión 5 h · 34%, Semana · 29%" });
    // Each figure is labeled next to the ring, the one closest to its limit first: no legend needed.
    expect(work.querySelector(".plan-usage__value")).toHaveTextContent("Sesión 5 h 34%");
    expect(work.querySelector(".plan-usage__second")).toHaveTextContent("Semana 29%");
    expect(work.querySelectorAll(".plan-usage__swatch")).toHaveLength(2);
    expect(within(plans).queryByText(/Anillo exterior/)).not.toBeInTheDocument();
    const personal = within(plans).getByRole("button", { name: /^Claude personal: / });
    expect(personal.querySelector(".plan-usage__value")).toHaveTextContent("Semana 83%");
    expect(personal.querySelector(".plan-usage__second")).toHaveTextContent("Sesión 5 h 58%");
    // No empty rings: a plan that could not be read says so quietly, and an API key pays as it goes.
    const chatgpt = within(plans).getByRole("button", { name: "ChatGPT: Sin datos: no se pudo actualizar" });
    expect(chatgpt.querySelector(".rings")).toBeNull();
    expect(within(plans).getByRole("button", { name: "Volver a consultar el plan de ChatGPT" })).toBeInTheDocument();
    // Kimi was never asked: a skeleton first, then "pago por uso" once the backend answers.
    const kimi = await within(plans).findByRole("button", { name: "Kimi: Pago por uso" });
    expect(kimi.querySelector(".rings")).toBeNull();

    // "Para ti" is an inbox: each row is a button, warnings marked by their icon, not a colored pill.
    const forYou = screen.getByRole("list", { name: "Para ti" });
    const waiting = await within(forYou).findByRole("button", { name: /tareas esperan tu respuesta/ });
    expect(waiting).toHaveClass("inbox__row--warn");
    // The proposals of the Activity inbox count here too.
    expect(await within(forYou).findByRole("button", { name: "5 notas o habilidades propuestas por aprobar" })).toBeInTheDocument();
    // A plan near its limit, with when it renews on the right.
    const limit = within(forYou).getByRole("button", { name: /^Claude personal va en 83 % esta semana/ });
    expect(limit.querySelector(".inbox__meta")).toHaveTextContent(/^Se renueva/);
    const recent = (await screen.findByRole("heading", { name: "Últimas tareas" })).closest("section")!;
    expect(within(recent).getAllByRole("button").length).toBeGreaterThan(0);
    expect(screen.getByRole("button", { name: /^Seguir en / })).toBeInTheDocument();
  });

  it("asks again for a plan that could not be read", async () => {
    const api = MockApi.withSampleData();
    const { user } = renderApp(api);
    const plans = (await screen.findByRole("heading", { name: "Tus planes" })).closest("section")!;
    const refresh = vi.spyOn(api, "refreshPlanUsage");
    await user.click(within(plans).getByRole("button", { name: "Volver a consultar el plan de ChatGPT" }));
    expect(refresh).toHaveBeenCalledWith("chatgpt");
  });

  it("keeps the CLI version check off home and lists at most five things for you", async () => {
    const api = MockApi.withSampleData().withSampleActivity();
    // Two more accounts near their limit: six rows in all.
    const overview = api.overview.bind(api);
    vi.spyOn(api, "overview").mockImplementation(async () => {
      const o = await overview();
      const personal = o.accounts.find((a) => a.id === "claude-personal")!;
      const clone = (id: string, name: string) => ({ ...personal, id, name });
      return { ...o, accounts: [...o.accounts, clone("extra-1", "Extra uno"), clone("extra-2", "Extra dos")] };
    });
    const { user } = renderApp(api);
    const forYou = await screen.findByRole("list", { name: "Para ti" });
    await within(forYou).findByRole("button", { name: /tareas esperan tu respuesta/ });
    await within(forYou).findByRole("button", { name: /propuestas por aprobar/ });
    expect(within(forYou).getAllByRole("button")).toHaveLength(5);
    // Version drift is a diagnosis for Settings, not something for you.
    expect(screen.queryByText(/0\.171\.0/)).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Ver todo en Actividad" }));
    expect(await screen.findByRole("heading", { name: "Actividad" })).toBeInTheDocument();
  });

  it("points to the diagnosis in Settings when something is broken", async () => {
    const api = MockApi.withSampleData();
    const doctor = api.doctor.bind(api);
    vi.spyOn(api, "doctor").mockImplementation(async () => {
      const report = await doctor();
      const broken = { area: "cli" as const, subject: "Codex", level: "error" as const, message: "no arranca", fix: null };
      return { ...report, checks: [...report.checks, broken], errors: 1 };
    });
    const { user } = renderApp(api);
    await user.click(await screen.findByRole("button", { name: "El diagnóstico encontró problemas (1)" }));
    const diagnosis = await screen.findByRole("list", { name: "Resultado del diagnóstico" });
    expect(within(diagnosis).getByText("no arranca")).toBeInTheDocument();
  });

  it("shows each project's live state and keeps its secondary actions in a menu", async () => {
    const { user } = renderApp(MockApi.withSampleData().withSampleActivity());
    const hedy = (await screen.findByRole("article", { name: "hedy" }));
    const homming = screen.getByRole("article", { name: "homming" });
    expect(await within(hedy).findByText("2 tareas te esperan")).toHaveClass("project-card__live--needs");
    expect(within(homming).getByText("1 tarea te espera · trabajando")).toBeInTheDocument();
    // One primary action; the rest behind "⋯".
    expect(within(hedy).getByRole("button", { name: "Trabajar aquí" })).toBeInTheDocument();
    expect(within(hedy).queryByRole("button", { name: "Abrir en VS Code" })).not.toBeInTheDocument();

    const more = within(hedy).getByRole("button", { name: "Más acciones para hedy" });
    expect(more).toHaveAttribute("aria-haspopup", "menu");
    await user.click(more);
    const menu = within(hedy).getByRole("menu", { name: "Más acciones para hedy" });
    expect(within(menu).getAllByRole("menuitem").map((i) => i.textContent)).toEqual([
      "Claude Code en terminal",
      "Terminal",
      "Abrir en VS Code",
      "Estado",
      "Memoria",
    ]);
    expect(within(menu).getByRole("menuitem", { name: "Claude Code en terminal" })).toHaveFocus();
    await user.keyboard("{ArrowUp}");
    expect(within(menu).getByRole("menuitem", { name: "Memoria" })).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(within(hedy).queryByRole("menu")).not.toBeInTheDocument();
    expect(more).toHaveFocus();

    await user.click(more);
    await user.click(within(hedy).getByRole("menuitem", { name: "Memoria" }));
    expect(await screen.findByRole("heading", { name: "Memoria de hedy" })).toBeInTheDocument();
  });

  it("says when a project had no recent activity", async () => {
    renderApp();
    const hedy = await screen.findByRole("article", { name: "hedy" });
    expect(await within(hedy).findByText("Sin actividad en las últimas 24 h")).toBeInTheDocument();
  });

  it("invites to add an account when there are none", async () => {
    renderApp(new MockApi());
    expect(await screen.findByRole("button", { name: "Agregar mi primera cuenta" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Tus planes" })).not.toBeInTheDocument();
    expect(screen.getByRole("img", { name: "Hedy te saluda" })).toHaveAttribute("data-state", "sleeping");
  });
});

describe("Accounts", () => {
  async function openAccounts() {
    const ctx = renderApp();
    await ctx.user.click(await screen.findByRole("button", { name: "Cuentas y modelos" }));
    return ctx;
  }

  it("shows one card per account with its login state", async () => {
    await openAccounts();
    const card = screen.getByRole("article", { name: "Claude trabajo" });
    expect(within(card).getByText("Anthropic · Team")).toBeInTheDocument();
    expect(within(card).getByText("Conectada")).toBeInTheDocument();
    expect(within(card).getByText(/carpeta por defecto de la CLI/)).toBeInTheDocument();
    expect(within(card).getByRole("listitem")).toHaveTextContent("homming");
    const personal = screen.getByRole("article", { name: "Claude personal" });
    expect(within(personal).getByText("182k tokens")).toBeInTheDocument();
    expect(within(personal).getByText("Tokens en Hedy esta semana")).toBeInTheDocument();
    const codex = screen.getByRole("article", { name: "ChatGPT" });
    expect(within(codex).getByRole("button", { name: "Iniciar sesión" })).toBeInTheDocument();
  });

  it("shows the plan's session and week from /usage, with thresholds and the source", async () => {
    await openAccounts();
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

    // The sidebar and the header use the window closest to its limit.
    const sidebar = screen.getByRole("complementary", { name: "Proyectos y cuentas" });
    // The sidebar shows a name and a quiet figure, warning-colored only near the limit; the window is in its name.
    const personalRow = within(sidebar).getByRole("button", { name: "Claude personal: Semana · 83%. Abrir la cuenta" });
    expect(within(personalRow).getByText("83%")).toHaveClass("level--warning");
    // Its tooltip says how old the reading is.
    expect(personalRow).toHaveAttribute("title", "Claude personal · Semana · 83% · actualizado hace 2 min");
    expect(personalRow.querySelector(".bar")).toBeNull();
    const workRow = within(sidebar).getByRole("button", { name: "Claude trabajo: Sesión 5 h · 34%. Abrir la cuenta" });
    expect(within(workRow).getByText("34%")).not.toHaveClass("level--warning");
    // Outside a project, the status bar names the account closest to its limit, the same way.
    const bar = screen.getByRole("contentinfo", { name: "Barra de estado" });
    const indicator = within(bar).getByRole("button", { name: "Claude personal: Semana · 83%. Abrir la cuenta" });
    expect(within(indicator).getByText("83%")).toHaveClass("level--warning");
  });

  it("asks for fresh readings while the sidebar is on screen and says which providers do not report", async () => {
    const api = MockApi.withSampleData();
    await api.addAccount({ name: "Gemini", provider: "gemini", apiKey: false, defaultFolder: false, color: null });
    const refresh = vi.spyOn(api, "refreshPlanUsage");
    const { user } = renderApp(api);
    await waitFor(() => expect(refresh).toHaveBeenCalledWith("claude-trabajo"));
    expect(refresh).toHaveBeenCalledWith("claude-personal");
    expect(refresh).not.toHaveBeenCalledWith("gemini");
    await user.click(await screen.findByRole("button", { name: "Cuentas y modelos" }));
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
    await user.click(await screen.findByRole("button", { name: "Cuentas y modelos" }));

    await user.click(await screen.findByRole("combobox", { name: "Cuenta de hobby" }));
    const options = optionLabels();
    expect(options).not.toContain("trabajo");
    expect(options).toContain("personal");
    await user.keyboard("{Escape}");
    expect(screen.getByRole("combobox", { name: "Cuenta de homming" })).toBeDisabled();
  });
});

describe("Models per phase", () => {
  it("assigns phases to other providers and summarises them", async () => {
    const { user } = renderApp();
    await user.click(await screen.findByRole("button", { name: "Cuentas y modelos" }));
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
    await user.click(await screen.findByRole("button", { name: "Cuentas y modelos" }));
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
