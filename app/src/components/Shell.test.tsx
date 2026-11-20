import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../App";
import { MockApi } from "../api/MockApi";

vi.mock("./Terminal/TerminalView", () => ({ TerminalView: () => null }));

function setup() {
  const api = MockApi.withSampleData();
  api.delayMs = 0;
  render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
  return { api, user: userEvent.setup() };
}

const sidebar = () => screen.getByRole("complementary", { name: "Proyectos y cuentas" });

describe("Shell", () => {
  afterEach(() => {
    history.replaceState(null, "", "#");
    localStorage.clear();
  });

  it("navigates from the sidebar alone: no top bar, no project tabs, no tip card", async () => {
    const { user } = setup();
    await screen.findByRole("heading", { name: /¡Hola, Alex!/ });
    expect(document.querySelector(".topbar")).toBeNull();
    expect(screen.queryByRole("navigation", { name: "Proyectos abiertos" })).not.toBeInTheDocument();
    expect(within(sidebar()).queryByRole("note")).not.toBeInTheDocument();
    expect(screen.queryByText(/lo leo de la CLI/)).not.toBeInTheDocument();
    // Accounts are reached from the account in use, at the foot: no list of them.
    expect(within(sidebar()).queryByRole("button", { name: "Cuentas y modelos" })).not.toBeInTheDocument();
    expect(within(sidebar()).getByRole("heading", { name: "Chats" })).toBeInTheDocument();

    // Inicio carries what waits for the user, as a badge and in its name.
    const nav = within(sidebar()).getByRole("navigation", { name: "Secciones" });
    const home = await within(nav).findByRole("button", { name: "Inicio: 7 cosas te esperan" });
    expect(home).toHaveAttribute("aria-current", "page");
    expect(within(home).getByText("7")).toHaveClass("nav-badge");

    await user.click(within(nav).getByRole("button", { name: "Tu memoria" }));
    expect(window.location.hash).toBe("#ask");
    expect(within(nav).getByRole("button", { name: "Tu memoria" })).toHaveAttribute("aria-current", "page");
    expect(home).not.toHaveAttribute("aria-current");
    await user.click(within(nav).getByRole("button", { name: "Habilidades" }));
    expect(window.location.hash).toBe("#skills");
    // The brand goes home too.
    await user.click(within(sidebar()).getByRole("button", { name: "Hedy" }));
    expect(await screen.findByRole("heading", { name: /¡Hola, Alex!/ })).toBeInTheDocument();

    // The search opens the palette (also Ctrl K).
    await user.click(within(sidebar()).getByRole("button", { name: "Buscar y ejecutar comandos (Ctrl K)" }));
    expect(screen.getByRole("dialog", { name: "Paleta de comandos" })).toBeInTheDocument();
    await user.keyboard("{Escape}");

    await user.click(within(sidebar()).getByRole("button", { name: "Ajustes" }));
    expect(window.location.hash).toBe("#settings");
  });

  it("shows the account in use at the foot, with its figure and a meter, and opens it", async () => {
    const { user } = setup();
    await screen.findByRole("heading", { name: /¡Hola, Alex!/ });
    // Outside a project: the default account.
    const row = within(sidebar()).getByRole("button", { name: "Claude personal: Semana · 83%. Abrir la cuenta" });
    expect(within(row).getByText("Claude personal")).toBeInTheDocument();
    expect(within(row).getByRole("meter", { name: "Semana · 83%" })).toHaveClass("meter--warn");
    // In a project: that project's account.
    await user.click(within(sidebar()).getByRole("button", { name: /^tienda/ }));
    const work = await within(sidebar()).findByRole("button", { name: /^Claude trabajo: .*Abrir la cuenta$/ });
    expect(within(work).getByText("34%")).not.toHaveClass("level--warning");
    await user.click(work);
    expect(await screen.findByRole("article", { name: "Claude trabajo" })).toBeInTheDocument();
    expect(window.location.hash).toBe("#a=claude-trabajo");
  });

  it("lists each project with what waits there, a lock when protected and a + for a new task", async () => {
    const { user } = setup();
    await screen.findByRole("heading", { name: /¡Hola, Alex!/ });
    const hedy = within(sidebar()).getByRole("button", { name: /^hedy/ });
    // The five proposals of hedy's memory and skills.
    expect(await within(hedy).findByTitle("5 cosas te esperan aquí")).toHaveTextContent("5");
    expect(within(hedy).queryByText("Proyecto protegido: solo usa su cuenta")).not.toBeInTheDocument();
    const tienda = within(sidebar()).getByRole("button", { name: /^tienda/ });
    expect(within(tienda).getByText("Proyecto protegido: solo usa su cuenta")).toHaveClass("sr-only");

    await user.click(within(sidebar()).getByRole("button", { name: "Nueva tarea en tienda" }));
    expect(await screen.findByRole("textbox", { name: "Pídele algo a tienda" })).toBeInTheDocument();
    expect(window.location.hash).toBe(`#p=${encodeURIComponent("/code/tienda")}&new`);
    expect(tienda).toHaveAttribute("aria-current", "page");
  });

  it("shows the status bar only in a project or a conversation, without sync, team or account items", async () => {
    const { user } = setup();
    await screen.findByRole("heading", { name: /¡Hola, Alex!/ });
    expect(screen.queryByRole("contentinfo", { name: "Barra de estado" })).not.toBeInTheDocument();

    await user.click(within(sidebar()).getByRole("button", { name: /^hedy/ }));
    const bar = await screen.findByRole("contentinfo", { name: "Barra de estado" });
    expect(within(bar).getByRole("button", { name: "Atajos de teclado (?)" })).toBeInTheDocument();
    expect(within(bar).queryByRole("button", { name: /Abrir la cuenta$/ })).not.toBeInTheDocument();
    expect(within(bar).queryByText(/Sincroniz/)).not.toBeInTheDocument();
    expect(within(bar).queryByText(/Plataforma/)).not.toBeInTheDocument();

    await user.click(within(sidebar()).getByRole("button", { name: "Pregunta rápida" }));
    expect(await screen.findByRole("contentinfo", { name: "Barra de estado" })).toBeInTheDocument();

    await user.click(within(sidebar()).getByRole("button", { name: "Ajustes" }));
    expect(screen.queryByRole("contentinfo", { name: "Barra de estado" })).not.toBeInTheDocument();
  });

  it("opens a terminal in the project on screen with Ctrl+J or the Terminal button, and hides it again", async () => {
    const { user } = setup();
    await screen.findByRole("heading", { name: /¡Hola, Alex!/ });
    await user.click(within(sidebar()).getByRole("button", { name: /^hedy/ }));
    await screen.findByRole("textbox", { name: /hedy/ });
    const button = within(sidebar()).getByRole("button", { name: /^Terminal/ });
    expect(button).toHaveAttribute("aria-keyshortcuts", "Control+J");
    expect(button).toHaveAttribute("aria-pressed", "false");

    await user.keyboard("{Control>}j{/Control}");
    const tabs = screen.getByRole("tablist", { name: "Terminales" });
    expect(within(tabs).getByRole("tab", { name: "Terminal · hedy" })).toBeInTheDocument();
    expect(button).toHaveAttribute("aria-pressed", "true");
    expect(button).toHaveTextContent("Terminal · 1");

    // With one open, the button (or Ctrl+J) shows and hides the panel instead of opening another.
    await user.click(button);
    expect(button).toHaveAttribute("aria-pressed", "false");
    expect(screen.queryByRole("tablist", { name: "Terminales" })).not.toBeInTheDocument();
    await user.keyboard("{Control>}j{/Control}");
    expect(button).toHaveAttribute("aria-pressed", "true");
    expect(within(screen.getByRole("tablist", { name: "Terminales" })).getAllByRole("tab")).toHaveLength(1);
  });

  it("jumps to the Nth sidebar project with Alt+digit, and the palette still switches projects", async () => {
    const { user } = setup();
    await screen.findByRole("heading", { name: /¡Hola, Alex!/ });
    const projects = within(sidebar()).getAllByRole("button").filter((b) => b.closest("section") && /^(hedy|tienda)/.test(b.textContent ?? ""));
    expect(projects.length).toBeGreaterThan(1);
    await user.keyboard("{Alt>}2{/Alt}");
    expect(projects[1]).toHaveAttribute("aria-current", "page");
    await user.keyboard("{Alt>}1{/Alt}");
    expect(projects[0]).toHaveAttribute("aria-current", "page");
    // A digit beyond the list does nothing.
    await user.keyboard("{Alt>}9{/Alt}");
    expect(projects[0]).toHaveAttribute("aria-current", "page");

    await user.keyboard("{Control>}k{/Control}");
    await user.type(screen.getByRole("combobox", { name: "Paleta de comandos" }), "tnda");
    await user.keyboard("{Enter}");
    expect(within(sidebar()).getByRole("button", { name: /^tienda/ })).toHaveAttribute("aria-current", "page");
  });

  it("hides the sidebar in focus mode with Ctrl+\\ or from the palette, and leaves it from its own button", async () => {
    const { user } = setup();
    await screen.findByRole("heading", { name: /¡Hola, Alex!/ });
    const app = document.querySelector(".app")!;
    expect(app).not.toHaveClass("app--focus");

    await user.keyboard("{Control>}\\{/Control}");
    expect(app).toHaveClass("app--focus");
    await user.click(screen.getByRole("button", { name: "Salir del modo enfoque" }));
    expect(app).not.toHaveClass("app--focus");

    await user.keyboard("{Control>}k{/Control}");
    await user.type(screen.getByRole("combobox", { name: "Paleta de comandos" }), "modo enfoque");
    await user.keyboard("{Enter}");
    expect(app).toHaveClass("app--focus");
    await user.keyboard("{Control>}\\{/Control}");
    expect(app).not.toHaveClass("app--focus");
  });

  it("lists the shortcuts with ?, outside text fields, and from the palette", async () => {
    const { user } = setup();
    await screen.findByRole("heading", { name: /¡Hola, Alex!/ });

    await user.keyboard("?");
    const dialog = screen.getByRole("dialog", { name: "Atajos de teclado" });
    expect(within(dialog).getByText("Abrir la paleta de comandos")).toBeInTheDocument();
    expect(within(dialog).getByText("Ir al proyecto 1–9 de la barra lateral")).toBeInTheDocument();
    expect(within(dialog).getByText("Aprobar")).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Cerrar" })).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "Atajos de teclado" })).not.toBeInTheDocument();

    // In a text field, "?" is just a character.
    await user.keyboard("{Control>}k{/Control}");
    const field = screen.getByRole("combobox", { name: "Paleta de comandos" });
    await user.type(field, "?");
    expect(screen.queryByRole("dialog", { name: "Atajos de teclado" })).not.toBeInTheDocument();
    await user.clear(field);
    await user.type(field, "atajos");
    await user.keyboard("{Enter}");
    expect(screen.getByRole("dialog", { name: "Atajos de teclado" })).toBeInTheDocument();
  });
});
