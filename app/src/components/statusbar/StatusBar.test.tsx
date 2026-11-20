import type { ReactNode } from "react";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { App } from "../../App";
import { ApiProvider } from "../../api/context";
import { MockApi } from "../../api/MockApi";
import type { Overview } from "../../api/types";
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
                    <StatusBar view={view} onNavigate={onNavigate} />
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

describe("Status bar", () => {
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
});

describe("Status bar in the app", () => {
  afterEach(() => history.replaceState(null, "", "#"));

  it("stays out of home and settings, and shows in a project's workspace", async () => {
    const api = MockApi.withSampleData();
    api.delayMs = 0;
    render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
    const user = userEvent.setup();
    await screen.findByRole("heading", { name: /¡Hola, Alex!/ });
    expect(screen.queryByRole("contentinfo", { name: "Barra de estado" })).toBeNull();
    const sidebar = screen.getByRole("complementary", { name: "Proyectos y cuentas" });
    await user.click(within(sidebar).getByRole("button", { name: "Ajustes" }));
    expect(await screen.findByRole("heading", { level: 1, name: "Ajustes" })).toBeInTheDocument();
    expect(screen.queryByRole("contentinfo", { name: "Barra de estado" })).toBeNull();
    await user.click(within(sidebar).getByRole("button", { name: /^hedy/ }));
    expect(await screen.findByRole("contentinfo", { name: "Barra de estado" })).toBeInTheDocument();
  });

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
    // The account in use lives in the sidebar, not in the bar; nor do sync or teams.
    expect(within(bar).queryByRole("button", { name: /Abrir la cuenta|computadoras|equipo/ })).toBeNull();
    expect(within(bar).getByRole("button", { name: "Atajos de teclado (?)" })).toBeInTheDocument();

    await user.keyboard("{Control>}\\{/Control}");
    expect(bar.closest(".app")).toHaveClass("app--focus");
    expect(screen.getByRole("contentinfo", { name: "Barra de estado" })).toBe(bar);
  });
});
