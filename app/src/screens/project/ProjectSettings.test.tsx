import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";
import type { Project } from "../../api/types";
import { changeStep } from "./ProjectSettings";
import { chooseOption, optionLabels } from "../../test/select";
import { accountPalette } from "../../theme/tokens";

// xterm needs a real canvas; no terminal is opened here.
vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

const HEDY = "/code/hedy";
const TIENDA = "/code/tienda";
const sheet = (project: string) => `#ps=${encodeURIComponent(project)}`;

/** The app over the sample data, on `hash`. */
async function setup(hash = sheet(HEDY), api = MockApi.withSampleData()) {
  api.delayMs = 0;
  history.replaceState(null, "", hash);
  render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
  return { api, user: userEvent.setup() };
}

/** The card whose heading is `name`. */
async function card(name: string | RegExp) {
  const heading = await screen.findByRole("heading", { level: 2, name }, { timeout: 5000 });
  return heading.closest("section") as HTMLElement;
}

async function projectOf(api: MockApi, path: string) {
  return (await api.overview()).projects.find((p) => p.path === path)!;
}

afterEach(() => {
  history.replaceState(null, "", "#");
  sessionStorage.clear();
  localStorage.clear();
});

describe("A project's settings", () => {
  it("has the project's tabs and a way back to it", async () => {
    const { api, user } = await setup();
    const editor = vi.spyOn(api, "openInEditor");
    const heading = await screen.findByRole("heading", { level: 1, name: "Ajustes de hedy" }, { timeout: 5000 });
    const head = heading.closest("header") as HTMLElement;
    const tabs = within(head).getByRole("navigation", { name: "Secciones del proyecto" });
    expect(within(tabs).getByRole("button", { name: "Ajustes" })).toHaveAttribute("aria-current", "page");
    await user.click(within(head).getByRole("button", { name: "Abrir en VS Code" }));
    expect(editor).toHaveBeenCalledWith(HEDY);

    await user.click(within(tabs).getByRole("button", { name: "Resumen" }));
    expect(window.location.hash).toBe(`#s=${encodeURIComponent(HEDY)}`);
    history.back();
    await screen.findByRole("heading", { level: 1, name: "Ajustes de hedy" });

    await user.click(screen.getByRole("button", { name: "Volver a hedy" }));
    expect(window.location.hash).toBe(`#p=${encodeURIComponent(HEDY)}`);
    expect(await screen.findByRole("heading", { level: 1, name: "hedy" })).toBeInTheDocument();
  });

  it("opens from each project's name in Cuentas", async () => {
    const { user } = await setup("#accounts");
    const table = await screen.findByRole("table", { name: "Proyectos y cuentas" }, { timeout: 5000 });
    await user.click(within(table).getByRole("link", { name: "tienda" }));
    expect(await screen.findByRole("heading", { level: 1, name: "Ajustes de tienda" })).toBeInTheDocument();
    expect(window.location.hash).toBe(sheet(TIENDA));
  });

  it("changes the project's account and protects it", async () => {
    const { api, user } = await setup();
    const use = vi.spyOn(api, "useAccount");
    const account = await card("Cuenta");
    const select = within(account).getByRole("combobox", { name: "Cuenta del proyecto" });
    expect(select).toHaveTextContent("Claude personal");

    // tienda reserves its account: it is not offered here.
    await user.click(select);
    expect(optionLabels()).toEqual(["Claude personal", "ChatGPT", "Kimi"]);
    await user.keyboard("{Escape}");

    await chooseOption(user, select, "ChatGPT");
    expect(use).toHaveBeenLastCalledWith(HEDY, "chatgpt", undefined);
    await waitFor(() => expect(select).toHaveTextContent("ChatGPT"));

    const protect = within(account).getByRole("switch", { name: "Proteger" });
    expect(protect).toHaveAttribute("aria-checked", "false");
    expect(within(account).getByText(/no se ofrece en los demás proyectos/)).toBeInTheDocument();
    await user.click(protect);
    expect(use).toHaveBeenLastCalledWith(HEDY, "chatgpt", true);
    await waitFor(() => expect(protect).toHaveAttribute("aria-checked", "true"));
    expect(select).toBeDisabled();
    expect(within(account).getByText("Quita la protección para cambiar la cuenta.")).toBeInTheDocument();
    expect((await projectOf(api, HEDY)).protected).toBe(true);
  });

  it("in a protected project, every step uses its account", async () => {
    const { api, user } = await setup(sheet(TIENDA));
    const account = await card("Cuenta");
    expect(within(account).getByRole("combobox", { name: "Cuenta del proyecto" })).toBeDisabled();
    expect(within(account).getByRole("switch", { name: "Proteger" })).toHaveAttribute("aria-checked", "true");

    const steps = await card("Quién hace cada paso");
    expect(within(steps).getByText(/Protegido: todos los pasos usan su cuenta/)).toBeInTheDocument();
    expect(within(steps).getByRole("combobox", { name: "Quién hace «Planificar»" })).toBeDisabled();
    expect(within(steps).getByRole("combobox", { name: "Quién hace «Implementar»" })).toBeDisabled();
    await user.click(within(steps).getByRole("combobox", { name: "Quién hace «Escribir tests»" }));
    expect(optionLabels()).toEqual(["No usar este paso", "Claude trabajo"]);
    await user.keyboard("{Escape}");

    // Unprotected, the other accounts come back.
    await user.click(within(account).getByRole("switch", { name: "Proteger" }));
    await waitFor(() => expect(within(account).getByRole("combobox", { name: "Cuenta del proyecto" })).toBeEnabled());
    expect((await projectOf(api, TIENDA)).protected).toBe(false);
    await user.click(within(steps).getByRole("combobox", { name: "Quién hace «Implementar»" }));
    expect(optionLabels()).toContain("ChatGPT");
  });

  it("says who does each step, inline, with a model from a list", async () => {
    const { api, user } = await setup();
    const save = vi.spyOn(api, "setProjectPhases");
    const steps = await card("Quién hace cada paso");
    const accountFor = (step: string) => within(steps).getByRole("combobox", { name: `Quién hace «${step}»` });
    const modelFor = (step: string) => within(steps).getByRole("combobox", { name: `Modelo para «${step}»` });

    expect(accountFor("Planificar")).toHaveTextContent("Cuenta del proyecto (Claude personal)");
    expect(accountFor("Verificar")).toHaveTextContent("No usar este paso");
    expect(modelFor("Verificar")).toBeDisabled();
    expect(modelFor("Planificar")).toHaveTextContent("Automático");

    // A model for the plan, from Claude's list; the step stays on the project's account.
    await user.click(modelFor("Planificar"));
    expect(optionLabels()).toEqual(["Automático", "Fable", "Opus", "Sonnet", "Haiku"]);
    await user.click(within(screen.getByRole("listbox")).getByRole("option", { name: /^Opus/ }));
    expect(save).toHaveBeenLastCalledWith(HEDY, { plan: { account: "claude-personal", model: "opus" } });
    await waitFor(() => expect(modelFor("Planificar")).toHaveTextContent("Opus"));
    expect(accountFor("Planificar")).toHaveTextContent("Cuenta del proyecto (Claude personal)");

    // Another account implements.
    await chooseOption(user, accountFor("Implementar"), "ChatGPT");
    expect(save).toHaveBeenLastCalledWith(HEDY, {
      plan: { account: "claude-personal", model: "opus" },
      implement: { account: "chatgpt", model: null },
    });
    await waitFor(() => expect(accountFor("Implementar")).toHaveTextContent("ChatGPT"));

    // An optional step: on with Kimi and one of its models, then off again (its model goes too).
    await chooseOption(user, accountFor("Verificar"), "Kimi");
    await waitFor(() => expect(modelFor("Verificar")).toBeEnabled());
    await user.click(modelFor("Verificar"));
    expect(optionLabels()).toEqual(["Automático", "kimi-k3.1-pro", "kimi-k3.1-pro-highspeed", "kimi-k3"]);
    await user.click(within(screen.getByRole("listbox")).getByRole("option", { name: /^kimi-k3$/ }));
    await waitFor(async () => expect((await projectOf(api, HEDY)).phases.review).toEqual({ account: "kimi", model: "kimi-k3" }));
    await waitFor(() => expect(modelFor("Verificar")).toHaveTextContent("kimi-k3"));

    await chooseOption(user, accountFor("Verificar"), "No usar este paso");
    await waitFor(() => expect(modelFor("Verificar")).toBeDisabled());
    expect((await projectOf(api, HEDY)).phases).toEqual({
      plan: { account: "claude-personal", model: "opus" },
      implement: { account: "chatgpt", model: null },
    });

    // Back to "Automático" on the project's account: the step goes back to its default.
    await chooseOption(user, modelFor("Planificar"), /^Automático/);
    await waitFor(async () => expect((await projectOf(api, HEDY)).phases).toEqual({ implement: { account: "chatgpt", model: null } }));
  });

  it("puts a step back on Automático when its account is another provider's", async () => {
    const { api, user } = await setup();
    const save = vi.spyOn(api, "setProjectPhases");
    const steps = await card("Quién hace cada paso");
    const accountFor = (step: string) => within(steps).getByRole("combobox", { name: `Quién hace «${step}»` });
    const modelFor = (step: string) => within(steps).getByRole("combobox", { name: `Modelo para «${step}»` });

    await chooseOption(user, modelFor("Planificar"), /^Opus/);
    await waitFor(() => expect(modelFor("Planificar")).toHaveTextContent("Opus"));
    // Codex would get `--model opus`: the model goes with the old provider.
    await chooseOption(user, accountFor("Planificar"), "ChatGPT");
    expect(save).toHaveBeenLastCalledWith(HEDY, { plan: { account: "chatgpt", model: null } });
    await waitFor(() => expect(modelFor("Planificar")).toHaveTextContent("Automático"));

    // The same for an optional step moved from Kimi to Claude.
    await chooseOption(user, accountFor("Escribir tests"), "Kimi");
    await chooseOption(user, modelFor("Escribir tests"), /^kimi-k3$/);
    await waitFor(async () => expect((await projectOf(api, HEDY)).phases.tests).toEqual({ account: "kimi", model: "kimi-k3" }));
    await chooseOption(user, accountFor("Escribir tests"), "Claude personal");
    await waitFor(async () =>
      expect((await projectOf(api, HEDY)).phases.tests).toEqual({ account: "claude-personal", model: null }),
    );
  });

  it("keeps a step on «Cuenta del proyecto» following the project's account", async () => {
    const sample = MockApi.withSampleData();
    await sample.addAccount({ name: "Claude extra", provider: "claude", apiKey: false, defaultFolder: false, color: null });
    const { api, user } = await setup(sheet(HEDY), sample);
    const steps = await card("Quién hace cada paso");
    const account = await card("Cuenta");
    const projectAccount = within(account).getByRole("combobox", { name: "Cuenta del proyecto" });
    const accountFor = (step: string) => within(steps).getByRole("combobox", { name: `Quién hace «${step}»` });
    const modelFor = (step: string) => within(steps).getByRole("combobox", { name: `Modelo para «${step}»` });

    // Opus for the plan on the project's account, Kimi verifying.
    await chooseOption(user, modelFor("Planificar"), /^Opus/);
    await chooseOption(user, accountFor("Verificar"), "Kimi");
    await waitFor(async () =>
      expect((await projectOf(api, HEDY)).phases).toEqual({
        plan: { account: "claude-personal", model: "opus" },
        review: { account: "kimi", model: null },
      }),
    );

    // Another Claude account: the plan moves with the project and keeps Opus.
    await chooseOption(user, projectAccount, "Claude extra");
    await waitFor(async () =>
      expect((await projectOf(api, HEDY)).phases).toEqual({
        plan: { account: "claude-extra", model: "opus" },
        review: { account: "kimi", model: null },
      }),
    );
    await waitFor(() => expect(accountFor("Planificar")).toHaveTextContent("Cuenta del proyecto (Claude extra)"));
    expect(modelFor("Planificar")).toHaveTextContent("Opus");

    // Codex: Opus would not run there, so the plan goes back to its default.
    await chooseOption(user, projectAccount, "ChatGPT");
    await waitFor(async () => expect((await projectOf(api, HEDY)).phases).toEqual({ review: { account: "kimi", model: null } }));
    await waitFor(() => expect(accountFor("Planificar")).toHaveTextContent("Cuenta del proyecto (ChatGPT)"));
    expect(modelFor("Planificar")).toHaveTextContent("Automático");
  });

  it("saves the client on blur and the color, and shows the folder", async () => {
    const { api, user } = await setup();
    const setClient = vi.spyOn(api, "setProjectClient");
    const setColor = vi.spyOn(api, "setProjectColor");
    const general = await card("General");

    const client = within(general).getByLabelText("Cliente");
    expect(client).toHaveValue("Estudio Norte");
    await user.click(client);
    expect(setClient).not.toHaveBeenCalled();
    await user.clear(client);
    await user.type(client, "Acme");
    expect(setClient).not.toHaveBeenCalled();
    await user.tab();
    expect(setClient).toHaveBeenCalledWith(HEDY, "Acme");
    await waitFor(async () => expect((await projectOf(api, HEDY)).client).toBe("Acme"));

    const colors = within(general).getByRole("radiogroup", { name: "Color de hedy" });
    const [first] = accountPalette;
    const swatch = within(colors).getByRole("radio", { name: "Azul" });
    expect(swatch).toHaveAttribute("aria-checked", "false");
    await user.click(swatch);
    expect(setColor).toHaveBeenLastCalledWith(HEDY, first?.hex);
    await waitFor(() => expect(swatch).toHaveAttribute("aria-checked", "true"));

    const none = within(general).getByRole("button", { name: "Sin color" });
    expect(none).toHaveAttribute("aria-pressed", "false");
    await user.click(none);
    expect(setColor).toHaveBeenLastCalledWith(HEDY, null);
    await waitFor(() => expect(none).toHaveAttribute("aria-pressed", "true"));
    expect(within(colors).getAllByRole("radio").every((r) => r.getAttribute("aria-checked") === "false")).toBe(true);
    expect(within(general).getByText("Sin color, usa el de su cuenta.")).toBeInTheDocument();

    // The folder, without a second "Abrir en VS Code" (the header has it).
    expect(within(general).getByText("/code/hedy")).toBeInTheDocument();
    expect(within(general).queryByRole("button", { name: "Abrir en VS Code" })).not.toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "Abrir en VS Code" })).toHaveLength(1);

    // Where its memory lives, as in Memoria.
    expect(await card("Memoria de este proyecto")).toBeInTheDocument();
  });

  it("removes the project from Hedy only after confirming, then goes to Inicio", async () => {
    const { api, user } = await setup();
    const remove = vi.spyOn(api, "removeProject");
    const danger = await card("Zona de peligro");
    expect(within(danger).getByText(/No se borra nada del disco/)).toBeInTheDocument();

    await user.click(within(danger).getByRole("button", { name: "Quitar de Hedy…" }));
    expect(remove).not.toHaveBeenCalled();
    await user.click(within(danger).getByRole("button", { name: "¿Quitar hedy de Hedy?" }));
    expect(remove).toHaveBeenCalledWith(HEDY);
    await waitFor(() => expect(window.location.hash).toBe("#welcome"));
    expect((await api.overview()).projects.map((p) => p.path)).not.toContain(HEDY);
  });
});

describe("The steps' plan", () => {
  const accounts = async () => (await MockApi.withSampleData().overview()).accounts;
  const project = (phases: Project["phases"], account = "claude-personal") =>
    ({ path: HEDY, account, phases }) as Project;

  it("keeps a step's model while its provider stays, and drops it when it changes", async () => {
    const all = [...(await accounts()), { ...(await accounts())[0]!, id: "claude-extra" }];
    const opus = project({ plan: { account: "claude-personal", model: "opus" } });
    expect(changeStep(opus, all, "plan", false, { account: "claude-extra" })).toEqual({
      plan: { account: "claude-extra", model: "opus" },
    });
    expect(changeStep(opus, all, "plan", false, { account: "chatgpt" })).toEqual({ plan: { account: "chatgpt", model: null } });
    // Back on "Cuenta del proyecto" (same provider): it keeps the model, stored with the project's account.
    const away = project({ plan: { account: "claude-extra", model: "opus" } });
    expect(changeStep(away, all, "plan", false, { account: "" })).toEqual({ plan: { account: "claude-personal", model: "opus" } });
  });
});
