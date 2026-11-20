import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";

vi.mock("../Terminal/TerminalView", () => ({ TerminalView: () => null }));

function setup(api = MockApi.withSampleData().withSampleActivity(), language?: "en") {
  api.delayMs = 0;
  if (language) void api.setSetting("ui.language", language);
  render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
  return { api, user: userEvent.setup() };
}

const SLOW = { timeout: 8000 };

/** The palette's own field: Inicio's project filter is a combobox too. */
function field(name = "Paleta de comandos") {
  return screen.getByRole("combobox", { name });
}

/** The palette's options (not those of a select on screen), in order. */
function optionRows(): HTMLElement[] {
  return within(screen.getByRole("listbox")).getAllByRole("option");
}

/** The labels of the palette's options, in order. */
function options(): string[] {
  return optionRows().map((o) => o.querySelector(".palette__label")?.textContent ?? "");
}

describe("Command palette", () => {
  afterEach(() => {
    history.replaceState(null, "", "#");
    localStorage.clear();
  });

  it("opens with Ctrl K and goes to a project typed loosely", async () => {
    const { user } = setup();
    await screen.findByRole("heading", { name: /¡Hola, Alex!/ });
    await user.keyboard("{Control>}k{/Control}");
    const input = field();
    expect(input).toHaveFocus();
    expect(screen.getByRole("dialog", { name: "Paleta de comandos" })).toBeInTheDocument();
    await user.type(input, "tnda");
    expect(options()[0]).toBe("tienda");
    expect(input).toHaveAttribute("aria-activedescendant", optionRows()[0]!.id);
    await user.keyboard("{Enter}");
    expect(screen.queryByRole("dialog", { name: "Paleta de comandos" })).not.toBeInTheDocument();
    expect(within(screen.getByRole("complementary", { name: "Proyectos y cuentas" })).getByRole("button", { name: /^tienda/ })).toHaveAttribute("aria-current", "page");
  });

  it("moves with the arrows, wraps around and lists the groups with no query", async () => {
    const { user } = setup();
    await screen.findByRole("heading", { name: /¡Hola, Alex!/ });
    await user.click(screen.getByRole("button", { name: "Buscar y ejecutar comandos (Ctrl K)" }));
    const input = field();
    const list = screen.getByRole("listbox", { name: "Comandos" });
    // What needs you first (the inbox), then where to go.
    expect(await within(list).findByRole("group", { name: "Te necesitan" }, SLOW)).toBeInTheDocument();
    for (const name of ["Ir a", "Proyectos", "Cuentas", "Acciones"]) {
      expect(within(list).getByRole("group", { name })).toBeInTheDocument();
    }
    const all = optionRows();
    expect(all[0]).toHaveAttribute("aria-selected", "true");
    await user.keyboard("{ArrowDown}");
    expect(all[1]).toHaveAttribute("aria-selected", "true");
    expect(input).toHaveAttribute("aria-activedescendant", all[1]!.id);
    await user.keyboard("{ArrowUp}{ArrowUp}");
    expect(optionRows().at(-1)).toHaveAttribute("aria-selected", "true");
  });

  it("jumps to what waits for an answer", async () => {
    const { user } = setup();
    await screen.findByRole("heading", { name: /¡Hola, Alex!/ });
    await user.keyboard("{Control>}k{/Control}");
    await user.type(field(), "responder migra");
    await waitFor(() => expect(options()[0]).toBe("Responder: Migra el reporte de comisiones a la API nueva"));
    await user.keyboard("{Enter}");
    expect(await screen.findByRole("heading", { name: "Migra el reporte de comisiones a la API nueva" })).toBeInTheDocument();
  });

  it("lists sync conflicts and team suggestions with what waits, and goes to them", async () => {
    const api = MockApi.withSampleData().withSampleActivity();
    const conflict = api.syncConflictList[0]!;
    api.syncConflictList.push({ ...conflict, id: 2 });
    const { user } = setup(api);
    await screen.findByRole("heading", { name: /¡Hola, Alex!/ });
    await user.keyboard("{Control>}k{/Control}");
    const list = screen.getByRole("listbox", { name: "Comandos" });
    const pending = await within(list).findByRole("group", { name: "Te necesitan" }, SLOW);
    await waitFor(() =>
      expect(within(pending).getByText("Resolver 2 cambios de sincronización")).toBeInTheDocument(),
    );
    expect(within(pending).getByText("Revisar 2 sugerencias del equipo Plataforma")).toBeInTheDocument();
    // Actividad lives in Inicio now: one place to go, not two.
    const go = within(list).getByRole("group", { name: "Ir a" });
    expect(within(go).getByText("Inicio")).toBeInTheDocument();
    expect(within(go).queryByText("Actividad")).not.toBeInTheDocument();

    await user.type(field(), "resolver sincronizacion");
    await waitFor(() => expect(options()[0]).toBe("Resolver 2 cambios de sincronización"));
    await user.keyboard("{Enter}");
    await waitFor(() => expect(window.location.hash).toBe("#sync"));

    await user.keyboard("{Control>}k{/Control}");
    await user.type(field(), "sugerencias plataforma");
    await waitFor(() => expect(options()[0]).toBe("Revisar 2 sugerencias del equipo Plataforma"));
    await user.keyboard("{Enter}");
    await waitFor(() => expect(window.location.hash).toBe("#teams"));
  });

  it("starts a new task in a project", async () => {
    const { user } = setup();
    await screen.findByRole("heading", { name: /¡Hola, Alex!/ });
    await user.keyboard("{Control>}k{/Control}");
    await user.type(field(), "nueva tarea hedy");
    expect(options()[0]).toBe("Nueva tarea en hedy");
    await user.keyboard("{Enter}");
    // hedy has open tasks, but this opens an empty chat.
    expect(await screen.findByRole("heading", { name: "hedy", level: 1 })).toBeInTheDocument();
  });

  it("opens an account's card", async () => {
    const { user } = setup();
    await screen.findByRole("heading", { name: /¡Hola, Alex!/ });
    await user.keyboard("{Control>}k{/Control}");
    await user.type(field(), "cuenta trabajo");
    expect(options()[0]).toBe("Cuenta Claude trabajo");
    await user.keyboard("{Enter}");
    expect(await screen.findByRole("article", { name: "Claude trabajo" })).toHaveClass("account-card--focus");
    expect(window.location.hash).toBe("#a=claude-trabajo");
  });

  it("changes a project's account only when asked, and never a protected one", async () => {
    const { api, user } = setup();
    const assign = vi.spyOn(api, "useAccount");
    await screen.findByRole("heading", { name: /¡Hola, Alex!/ });
    await user.keyboard("{Control>}k{/Control}");
    await user.type(field(), "cambiar cuenta tienda");
    // Nothing matches; what was typed can still be asked outside any project.
    expect(options()).toEqual(["Preguntar «cambiar cuenta tienda»"]);
    expect(screen.getByText("Nada coincide con «cambiar cuenta tienda».")).toBeInTheDocument();

    await user.clear(field());
    await user.type(field(), "cambiar cuenta hedy");
    expect(options()[0]).toBe("Cambiar la cuenta de hedy…");
    await user.keyboard("{Enter}");
    const page = screen.getByRole("dialog", { name: "¿Con qué cuenta trabaja hedy?" });
    expect(within(page).getByText(/^Ahora: Claude personal\. Hedy nunca cambia de cuenta solo/)).toBeInTheDocument();
    // The protected project's account is not offered; the current one neither.
    expect(options()).toEqual(["ChatGPT", "Kimi"]);
    expect(assign).not.toHaveBeenCalled();
    await user.keyboard("{Enter}");
    expect(assign).toHaveBeenCalledWith("/code/hedy", "chatgpt");
    expect(await screen.findByText("hedy ahora trabaja con ChatGPT")).toBeInTheDocument();
  });

  it("reads every plan's usage again on demand", async () => {
    const { api, user } = setup();
    await screen.findByRole("heading", { name: /¡Hola, Alex!/ });
    const refresh = vi.spyOn(api, "refreshPlanUsage");
    await user.keyboard("{Control>}k{/Control}");
    await user.type(field(), "actualizar uso");
    await waitFor(() => expect(options()[0]).toBe("Actualizar uso de los planes"));
    await user.keyboard("{Enter}");
    expect(refresh).toHaveBeenCalledWith("claude-personal");
    expect(refresh).toHaveBeenCalledWith("claude-trabajo");
    expect(await screen.findByText("Uso de los planes actualizado")).toBeInTheDocument();
  });

  it("goes back from a page with Escape, then closes and gives the focus back", async () => {
    const { user } = setup();
    await screen.findByRole("heading", { name: /¡Hola, Alex!/ });
    const hint = screen.getByRole("button", { name: "Buscar y ejecutar comandos (Ctrl K)" });
    await user.click(hint);
    await user.type(field(), "cambiar cuenta hedy");
    await user.keyboard("{Enter}");
    expect(screen.getByRole("dialog", { name: "¿Con qué cuenta trabaja hedy?" })).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.getByRole("dialog", { name: "Paleta de comandos" })).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(hint).toHaveFocus();
  });

  it("puts recent commands first", async () => {
    const { user } = setup();
    await screen.findByRole("heading", { name: /¡Hola, Alex!/ });
    await user.keyboard("{Control>}k{/Control}");
    await user.type(field(), "ajustes");
    await user.keyboard("{Enter}");
    expect(await screen.findByRole("heading", { name: "Ajustes" })).toBeInTheDocument();
    await user.keyboard("{Control>}k{/Control}");
    const recent = screen.getByRole("group", { name: "Recientes" });
    expect(within(recent).getAllByRole("option").map((o) => o.textContent)).toEqual(["Ajustes"]);
    expect(optionRows()[0]).toHaveTextContent("Ajustes");
  });

  it("speaks English", async () => {
    const { user } = setup(undefined, "en");
    await screen.findByRole("heading", { name: /Hi, Alex!/ });
    await user.click(screen.getByRole("button", { name: "Search and run commands (Ctrl K)" }));
    expect(field("Command palette")).toHaveAttribute(
      "placeholder",
      "Search a project, a task or an action",
    );
    expect(await screen.findByRole("group", { name: "Needs you" }, SLOW)).toBeInTheDocument();
    await user.type(field("Command palette"), "new task hedy");
    expect(options()[0]).toBe("New task in hedy");
    await user.clear(field("Command palette"));
    await user.type(field("Command palette"), "change hedy");
    await user.keyboard("{Enter}");
    expect(screen.getByRole("dialog", { name: "Which account does hedy work with?" })).toBeInTheDocument();
  });
});
