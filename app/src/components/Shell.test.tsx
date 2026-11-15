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

  it("navigates from the sidebar alone: no project tabs, no tip card", async () => {
    setup();
    await screen.findByRole("heading", { name: /¡Hola, Enrique!/ });
    expect(screen.queryByRole("navigation", { name: "Proyectos abiertos" })).not.toBeInTheDocument();
    expect(within(sidebar()).queryByRole("note")).not.toBeInTheDocument();
    expect(screen.queryByText(/lo leo de la CLI/)).not.toBeInTheDocument();
    // Accounts are quiet rows: a dot, the name and the figure, no bar.
    const row = within(sidebar()).getByRole("button", { name: /^Claude trabajo: .*Abrir la cuenta$/ });
    expect(row.querySelector(".bar")).toBeNull();
    expect(within(row).getByText("34%")).toHaveClass("sidebar__meta");
  });

  it("explains where the figures come from once, in the usage popover", async () => {
    const { user } = setup();
    await user.click(await screen.findByRole("button", { name: /^Uso de los planes:/ }));
    const popover = screen.getByRole("dialog", { name: "Uso de los planes" });
    expect(within(popover).getByText(/El % de cada plan lo leo de su CLI/)).toBeInTheDocument();
  });

  it("jumps to the Nth sidebar project with Alt+digit, and the palette still switches projects", async () => {
    const { user } = setup();
    await screen.findByRole("heading", { name: /¡Hola, Enrique!/ });
    const projects = within(sidebar()).getAllByRole("button").filter((b) => b.closest("section") && /^(hedy|homming)/.test(b.textContent ?? ""));
    expect(projects.length).toBeGreaterThan(1);
    await user.keyboard("{Alt>}2{/Alt}");
    expect(projects[1]).toHaveAttribute("aria-current", "page");
    await user.keyboard("{Alt>}1{/Alt}");
    expect(projects[0]).toHaveAttribute("aria-current", "page");
    // A digit beyond the list does nothing.
    await user.keyboard("{Alt>}9{/Alt}");
    expect(projects[0]).toHaveAttribute("aria-current", "page");

    await user.keyboard("{Control>}k{/Control}");
    await user.type(screen.getByRole("combobox", { name: "Paleta de comandos" }), "hmng");
    await user.keyboard("{Enter}");
    expect(within(sidebar()).getByRole("button", { name: /^homming/ })).toHaveAttribute("aria-current", "page");
  });

  it("hides the sidebar in focus mode with Ctrl+\\ or from the palette, and leaves it from the top bar", async () => {
    const { user } = setup();
    await screen.findByRole("heading", { name: /¡Hola, Enrique!/ });
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
    await screen.findByRole("heading", { name: /¡Hola, Enrique!/ });

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
