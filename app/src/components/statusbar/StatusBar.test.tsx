import type { ReactNode } from "react";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { App } from "../../App";
import { ApiProvider } from "../../api/context";
import { MockApi } from "../../api/MockApi";
import type { Overview, PlanState } from "../../api/types";
import { FileNavigatorProvider, useFileNavigator, type NavigatorTarget } from "../../files/navigator";
import { I18nProvider } from "../../i18n";
import type { View } from "../Layout";
import { PlanUsageProvider } from "../planUsage";
import { ShortcutsProvider } from "../shortcuts";
import { TerminalProvider } from "../Terminal/TerminalProvider";
import { ToastProvider } from "../ui/Toast";
import { StatusBar } from "./StatusBar";

vi.mock("../Terminal/TerminalView", () => ({ TerminalView: () => null }));

const HEDY = "/code/hedy";
const TAX_TASK = "Arregla el cálculo de impuestos en las facturas";

/** Opens a file in the navigator, as the explorer or a search result would. */
function OpenFile({ target }: { target: NavigatorTarget }) {
  const nav = useFileNavigator();
  return (
    <button type="button" onClick={() => nav.open(target)}>
      test: open file
    </button>
  );
}

async function renderBar(view: View, extra?: ReactNode) {
  const api = MockApi.withSampleData().withSampleActivity();
  api.delayMs = 0;
  const overview: Overview = await api.overview();
  const onNavigate = vi.fn();
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <ApiProvider api={api}>
        <I18nProvider setting="es">
          <ToastProvider>
            <FileNavigatorProvider>
              <TerminalProvider>
                <ShortcutsProvider>
                  <PlanUsageProvider accounts={overview.accounts} screen={view.screen}>
                    <StatusBar overview={overview} view={view} onNavigate={onNavigate} />
                    {extra}
                  </PlanUsageProvider>
                </ShortcutsProvider>
              </TerminalProvider>
            </FileNavigatorProvider>
          </ToastProvider>
        </I18nProvider>
      </ApiProvider>
    </QueryClientProvider>,
  );
  const bar = screen.getByRole("contentinfo", { name: "Barra de estado" });
  return { api, overview, onNavigate, bar, user: userEvent.setup() };
}

const PERSONAL = "Claude personal: Semana · 83%. Abrir la cuenta";
const nowSecs = () => Math.floor(Date.now() / 1000);

describe("Status bar", () => {
  it("at home shows only the account and the shortcuts", async () => {
    const { bar, onNavigate, user } = await renderBar({ screen: "welcome" });
    const account = within(bar).getByRole("button", { name: "Claude personal: Semana · 83%. Abrir la cuenta" });
    expect(within(bar).getAllByRole("button")).toHaveLength(2);
    expect(within(bar).queryByRole("group", { name: "Tu carpeta" })).toBeNull();
    await user.click(account);
    expect(onNavigate).toHaveBeenCalledWith({ screen: "accounts", account: "claude-personal" });

    await user.click(within(bar).getByRole("button", { name: "Atajos de teclado (?)" }));
    expect(screen.getByRole("dialog", { name: "Atajos de teclado" })).toBeInTheDocument();
  });

  it("in a task says which copy the code views show and switches it from its menu", async () => {
    const { bar, onNavigate, user } = await renderBar({ screen: "workspace", project: HEDY, task: "s5" });
    const copy = await within(bar).findByRole("button", { name: `Las vistas de código muestran Tarea: ${TAX_TASK}. Cambiar de copia` });
    expect(within(copy).getByText("Tarea")).toBeInTheDocument();
    expect(within(copy).getByText(TAX_TASK)).toBeInTheDocument();

    await user.click(copy);
    const menu = screen.getByRole("menu");
    // The copy on screen cannot be picked again; the open tasks of the project can.
    expect(within(menu).getByRole("menuitem", { name: new RegExp(`^Tarea: ${TAX_TASK}`) })).toBeDisabled();
    expect(within(menu).queryByRole("menuitem", { name: /Revisa los tests del carrito/ })).toBeNull();
    await user.click(within(menu).getByRole("menuitem", { name: "Tu carpeta" }));
    expect(onNavigate).toHaveBeenCalledWith({ screen: "workspace", project: HEDY, task: null });

    await user.click(copy);
    await user.click(within(screen.getByRole("menu")).getByRole("menuitem", { name: /^Tarea: Agrega recordatorios/ }));
    expect(onNavigate).toHaveBeenLastCalledWith({ screen: "workspace", project: HEDY, task: "s1" });
  });

  it("shows the folder's branch and counters in every project screen, never the copy or the changes outside the workspace", async () => {
    const { bar } = await renderBar({ screen: "status", project: HEDY });
    const folder = await within(bar).findByRole("group", { name: "Tu carpeta" });
    expect(within(folder).getByRole("button", { name: "Tu carpeta: rama main" })).toBeInTheDocument();
    expect(within(folder).getByRole("button", { name: /Commits por traer: 2/ })).toBeInTheDocument();
    expect(within(bar).queryByRole("button", { name: /Cambiar de copia/ })).toBeNull();
    expect(within(bar).queryByRole("button", { name: /Revisar/ })).toBeNull();
  });

  it("counts the copy's changes and opens the review with Ctrl+Shift+G", async () => {
    const { bar, user } = await renderBar({ screen: "workspace", project: HEDY, task: null });
    const changes = await within(bar).findByRole("button", { name: "3 cambios sin commit en tu carpeta. Revisar (Ctrl Shift G)" });
    expect(changes).toHaveTextContent("3");

    const pressed: KeyboardEvent[] = [];
    const listen = (e: KeyboardEvent) => {
      if (e.key !== "G" || !e.shiftKey || !e.ctrlKey) return;
      pressed.push(e);
      e.preventDefault();
    };
    window.addEventListener("keydown", listen);
    await user.click(changes);
    window.removeEventListener("keydown", listen);
    expect(pressed).toHaveLength(1);
  });

  it("without a panel to take the shortcut, opens the first changed file of the task, then copies its path", async () => {
    const { bar, user } = await renderBar({ screen: "workspace", project: HEDY, task: "s5" });
    const changes = await within(bar).findByRole("button", { name: /^\d+ archivos? cambiados? por la tarea\. Revisar/ });
    await user.click(changes);
    const file = await within(bar).findByRole("button", { name: /: copiar la ruta$/ });
    const path = file.getAttribute("aria-label")?.replace(/: copiar la ruta$/, "") ?? "";
    expect(path).not.toBe("");
    await user.click(file);
    await expect(navigator.clipboard.readText()).resolves.toBe(path);
    expect(await screen.findByText(`Ruta copiada: ${path}`)).toBeInTheDocument();
  });

  it("shows the open file's name and line", async () => {
    const target: NavigatorTarget = { copy: { kind: "folder", project: HEDY }, path: "facturas/impuestos.py", line: 16 };
    const { bar, user } = await renderBar({ screen: "workspace", project: HEDY, task: null }, <OpenFile target={target} />);
    expect(within(bar).queryByRole("button", { name: /copiar la ruta/ })).toBeNull();
    await user.click(screen.getByRole("button", { name: "test: open file" }));
    const file = within(bar).getByRole("button", { name: "facturas/impuestos.py: copiar la ruta" });
    expect(file).toHaveTextContent("impuestos.pyLín 16");
  });

  it("shows the project's last tests and opens its status", async () => {
    const { bar, onNavigate, user } = await renderBar({ screen: "workspace", project: HEDY, task: null });
    const tests = await within(bar).findByRole("button", { name: /^Últimos tests \(pytest -q\): pasaron hace 2 h\./ });
    expect(tests).toHaveTextContent("✓Tests");
    await user.click(tests);
    expect(onNavigate).toHaveBeenCalledWith({ screen: "status", project: HEDY });
  });

  it("says the agent is working and goes to the composer", async () => {
    const composer = <textarea id="composer-input" aria-label="composer" />;
    const { bar, user } = await renderBar({ screen: "workspace", project: "/code/tienda", task: "s4" }, composer);
    const agent = await within(bar).findByRole("button", { name: "Claude Opus está trabajando. Ir al cuadro de texto" });
    expect(agent).toHaveTextContent("Claude Opus trabajando…");
    expect(agent.querySelector(".statusbar__dot--working")).not.toBeNull();
    await user.click(agent);
    expect(screen.getByRole("textbox", { name: "composer" })).toHaveFocus();
  });

  it("says when the agent waits for the user and goes to the card", async () => {
    const card = (
      <section className="permission-card" aria-label="card" tabIndex={-1}>
        <button type="button">Permitir</button>
      </section>
    );
    const { bar, user } = await renderBar({ screen: "workspace", project: "/code/tienda", task: "s2" }, card);
    const agent = await within(bar).findByRole("button", { name: "Claude Opus espera tu respuesta. Ir a la tarjeta" });
    expect(agent).toHaveTextContent("Te espera");
    expect(agent).toHaveClass("statusbar__agent--waiting");
    await user.click(agent);
    expect(screen.getByRole("region", { name: "card" })).toHaveFocus();
  });

  it("names the open task's account, else the project's", async () => {
    const { bar } = await renderBar({ screen: "memory", project: "/code/tienda" });
    expect(within(bar).getByRole("button", { name: /^Claude trabajo: .*Abrir la cuenta$/ })).toBeInTheDocument();
    // No agent, copy or file outside the workspace.
    await waitFor(() => expect(within(bar).getByRole("group", { name: "Tu carpeta" })).toBeInTheDocument());
    expect(within(bar).queryByRole("button", { name: /trabajando|Te espera|copiar la ruta/ })).toBeNull();
  });
});

describe("Status bar account", () => {
  /** Waits for the reading asked on mount, then answers the next ones with `answer`. */
  async function withReading(api: MockApi, overview: Overview, answer: (plan: PlanState) => PlanState | Promise<PlanState>) {
    const plan = overview.accounts.find((a) => a.id === "claude-personal")!.usage.plan;
    const refresh = vi.spyOn(api, "refreshPlanUsage");
    await waitFor(() => expect(within(screen.getByRole("contentinfo")).getByRole("button", { name: PERSONAL })).not.toHaveAttribute("aria-busy"));
    refresh.mockClear();
    refresh.mockImplementation(async () => answer(plan));
    return refresh;
  }

  it("says how old the reading is in its tooltip", async () => {
    const { bar } = await renderBar({ screen: "welcome" });
    const title = within(bar).getByRole("button", { name: PERSONAL }).getAttribute("title")!;
    expect(title.split("\n")[0]).toBe("Claude personal · Semana · 83% · actualizado hace 2 min");
    expect(title).toContain("Clic derecho: actualizar ahora");
  });

  it("reads the plan again from its context menu and says the new figure", async () => {
    const { api, overview, bar, user } = await renderBar({ screen: "welcome" });
    const refresh = await withReading(api, overview, (plan) => ({ ...plan, attemptedAt: nowSecs() + 5 }));
    const account = within(bar).getByRole("button", { name: PERSONAL });
    await user.pointer({ keys: "[MouseRight]", target: account });
    const menu = screen.getByRole("menu", { name: "Uso de Claude personal" });
    expect(within(menu).getAllByRole("menuitem").map((i) => i.textContent)).toEqual(["Actualizar ahora", "Abrir la cuenta"]);
    await user.click(within(menu).getByRole("menuitem", { name: "Actualizar ahora" }));
    expect(refresh).toHaveBeenCalledWith("claude-personal");
    expect(await screen.findByText("Uso de Claude personal actualizado: Semana · 83%")).toBeInTheDocument();
  });

  it("does not pretend when the backend answers from its cache", async () => {
    const { api, overview, bar, user } = await renderBar({ screen: "welcome" });
    // Asked 30 s ago: the backend keeps the reading for three minutes.
    await withReading(api, overview, (plan) => ({ ...plan, attemptedAt: nowSecs() - 30 }));
    await user.pointer({ keys: "[MouseRight]", target: within(bar).getByRole("button", { name: PERSONAL }) });
    await user.click(screen.getByRole("menuitem", { name: "Actualizar ahora" }));
    expect(
      await screen.findByText("Actualizado hace 2 min; se puede volver a consultar en 2 min 30 s"),
    ).toBeInTheDocument();
    expect(screen.queryByText(/actualizado: Semana/)).toBeNull();
  });

  it("opens its menu from the keyboard and keeps the last figure, muted, while it reads", async () => {
    const { api, overview, bar, onNavigate, user } = await renderBar({ screen: "welcome" });
    let answer!: (plan: PlanState) => void;
    const reading = new Promise<PlanState>((resolve) => (answer = resolve));
    const plan = overview.accounts.find((a) => a.id === "claude-personal")!.usage.plan;
    await withReading(api, overview, () => reading);
    const account = within(bar).getByRole("button", { name: PERSONAL });

    account.focus();
    await user.keyboard("{Shift>}{F10}{/Shift}");
    await user.click(screen.getByRole("menuitem", { name: "Actualizar ahora" }));
    expect(account).toHaveAttribute("aria-busy", "true");
    expect(within(account).getByText("83%")).toHaveClass("plan-figure--busy");
    expect(account.getAttribute("title")).toMatch(/actualizando…/);
    answer({ ...plan, attemptedAt: nowSecs() + 5 });
    await waitFor(() => expect(account).not.toHaveAttribute("aria-busy"));
    expect(within(account).getByText("83%")).not.toHaveClass("plan-figure--busy");

    account.focus();
    await user.keyboard("{ContextMenu}");
    await user.click(screen.getByRole("menuitem", { name: "Abrir la cuenta" }));
    expect(onNavigate).toHaveBeenCalledWith({ screen: "accounts", account: "claude-personal" });
  });
});

describe("Status bar in the app", () => {
  afterEach(() => history.replaceState(null, "", "#"));

  it("takes the folder out of the task header and stays (dimmed) in focus mode", async () => {
    history.replaceState(null, "", `#p=${encodeURIComponent(HEDY)}&t=s5`);
    const api = MockApi.withSampleData().withSampleActivity();
    api.delayMs = 0;
    render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
    const user = userEvent.setup();
    const bar = await screen.findByRole("contentinfo", { name: "Barra de estado" });
    await within(bar).findByRole("group", { name: "Tu carpeta" });
    const header = screen.getByRole("heading", { level: 1, name: TAX_TASK }).closest("header") as HTMLElement;
    expect(within(header).queryByRole("group", { name: "Tu carpeta" })).toBeNull();
    // The top bar has no plan usage of its own any more.
    expect(within(document.querySelector(".topbar") as HTMLElement).queryByRole("button", { name: /Abrir la cuenta/ })).toBeNull();

    await user.keyboard("{Control>}\\{/Control}");
    expect(bar.closest(".app")).toHaveClass("app--focus");
    expect(screen.getByRole("contentinfo", { name: "Barra de estado" })).toBe(bar);
  });
});
