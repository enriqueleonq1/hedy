import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../App";
import { MockApi } from "../api/MockApi";

vi.mock("./Terminal/TerminalView", () => ({ TerminalView: () => null }));

function setup(api = MockApi.withSampleData().withSampleActivity()) {
  api.delayMs = 0;
  render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
  return { api, user: userEvent.setup() };
}

const sidebar = () => screen.getByRole("complementary", { name: "Navegación" });
const ready = () => screen.findByRole("complementary", { name: "Navegación" }, { timeout: 5000 });

describe("Sidebar quick actions", () => {
  afterEach(() => history.replaceState(null, "", "#"));

  it("offers a project's actions on right click and goes where each says", async () => {
    const { user } = setup();
    const hedy = await within(await ready()).findByRole("button", { name: "hedy" });
    await user.pointer({ keys: "[MouseRight]", target: hedy });
    const menu = screen.getByRole("menu", { name: "Acciones de hedy" });
    expect(within(menu).getAllByRole("menuitem").map((i) => i.textContent)).toEqual([
      "Nueva tarea",
      "Abrir una terminal aquí",
      "Abrir en VS Code",
      "Memoria",
      "Resumen",
      "Ajustes del proyecto",
      "Copiar la ruta",
      "Quitar de Hedy…",
    ]);
    await user.click(within(menu).getByRole("menuitem", { name: "Ajustes del proyecto" }));
    expect(window.location.hash).toBe(`#ps=${encodeURIComponent("/code/hedy")}`);
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("removes a project from Hedy only after confirming, and leaves its screens", async () => {
    // On the project's own home: removing it leaves for Inicio.
    history.replaceState(null, "", `#p=${encodeURIComponent("/code/tienda")}`);
    const { api, user } = setup();
    const remove = vi.spyOn(api, "removeProject");
    const tienda = await within(await ready()).findByRole("button", { name: /^tienda/ });
    tienda.focus();
    // From the keyboard too: Shift+F10.
    await user.keyboard("{Shift>}{F10}{/Shift}");
    await user.click(screen.getByRole("menuitem", { name: "Quitar de Hedy…" }));
    const dialog = screen.getByRole("dialog", { name: "¿Quitar tienda de Hedy?" });
    expect(within(dialog).getByText(/Su carpeta, sus archivos y su repositorio no se tocan/)).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Cancelar" }));
    expect(remove).not.toHaveBeenCalled();

    await user.pointer({ keys: "[MouseRight]", target: within(sidebar()).getByRole("button", { name: /^tienda/ }) });
    await user.click(screen.getByRole("menuitem", { name: "Quitar de Hedy…" }));
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Quitar de Hedy" }));
    expect(remove).toHaveBeenCalledWith("/code/tienda");
    await waitFor(() => expect(within(sidebar()).queryByRole("button", { name: /^tienda/ })).not.toBeInTheDocument());
    expect(window.location.hash).toBe("#welcome");
  });

  it("offers a chat's actions, asks again with the same question and removes it after confirming", async () => {
    const { api, user } = setup();
    const remove = vi.spyOn(api, "deleteConversation");
    const chat = await within(await ready()).findByRole("button", { name: /OAuth y OpenID/ });
    await user.pointer({ keys: "[MouseRight]", target: chat });
    const menu = screen.getByRole("menu", { name: /^Acciones de «/ });
    expect(within(menu).getAllByRole("menuitem").map((i) => i.textContent?.split("Un chat nuevo")[0])).toEqual([
      "Abrir",
      "Preguntar algo parecido",
      "Copiar la pregunta",
      "Quitar…",
    ]);
    await user.click(within(menu).getByRole("menuitem", { name: /Preguntar algo parecido/ }));
    expect(await screen.findByDisplayValue("¿Qué diferencia hay entre OAuth y OpenID Connect?")).toBeInTheDocument();

    await user.pointer({ keys: "[MouseRight]", target: within(sidebar()).getByRole("button", { name: /OAuth y OpenID/ }) });
    await user.click(screen.getByRole("menuitem", { name: "Quitar…" }));
    await user.click(within(screen.getByRole("dialog", { name: "¿Quitar esta conversación?" })).getByRole("button", { name: "Quitar" }));
    expect(remove).toHaveBeenCalledWith("c1");
    await waitFor(() => expect(within(sidebar()).queryByRole("button", { name: /OAuth y OpenID/ })).not.toBeInTheDocument());
  });
});
