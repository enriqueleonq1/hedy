import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";
import { optionLabels } from "../../test/select";

vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

function setup(api = MockApi.withSampleData().withSampleActivity()) {
  api.delayMs = 0;
  render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
  return { api, user: userEvent.setup() };
}

const SLOW = { timeout: 8000 };

describe("Starting something new", () => {
  afterEach(() => {
    history.replaceState(null, "", "#");
    localStorage.clear();
  });

  it("offers a new project or an existing folder from the sidebar's +, and a quick question from Chats", async () => {
    const { user } = setup();
    await screen.findByRole("heading", { name: /¡Hola, Alex!/ });
    // Home no longer has its own "Nuevo" menu: the sidebar is where things start.
    expect(screen.queryByRole("button", { name: "Empezar algo nuevo" })).not.toBeInTheDocument();
    const sidebar = screen.getByRole("complementary", { name: "Navegación" });
    const trigger = within(sidebar).getByRole("button", { name: "Agregar un proyecto" });
    expect(trigger).toHaveAttribute("aria-haspopup", "menu");
    await user.click(trigger);
    const menu = screen.getByRole("menu", { name: "Agregar un proyecto" });
    const items = within(menu).getAllByRole("menuitem");
    expect(items.map((i) => i.querySelector(".menu__label")?.textContent)).toEqual(["Nuevo proyecto", "Abrir carpeta…"]);
    expect(items[0]).toHaveFocus();
    expect(within(menu).getByText("Una carpeta nueva, lista para trabajar")).toBeInTheDocument();
    expect(within(menu).getByText("Un proyecto que ya tienes en tu equipo")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    // The quick question has its own "+", next to the chats.
    expect(within(sidebar).getByRole("heading", { name: "Chats" })).toBeInTheDocument();
    expect(within(sidebar).getByRole("button", { name: "Pregunta rápida" })).toBeInTheDocument();
  });

  it("creates a project from a name and lands in its composer, ready to type", async () => {
    const { api, user } = setup();
    const create = vi.spyOn(api, "newProject");
    await screen.findByRole("heading", { name: /¡Hola, Alex!/ });
    await user.click(screen.getByRole("button", { name: "Agregar un proyecto" }));
    await user.click(screen.getByRole("menuitem", { name: /Nuevo proyecto/ }));

    const dialog = screen.getByRole("dialog", { name: "Nuevo proyecto" });
    const name = within(dialog).getByRole("textbox", { name: "Nombre" });
    expect(name).toHaveFocus();
    // Where most projects live, changeable.
    expect(within(dialog).getByText("/code")).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Cambiar la carpeta donde se crea" })).toBeInTheDocument();
    const createButton = within(dialog).getByRole("button", { name: "Crear proyecto" });
    expect(createButton).toBeDisabled();

    // An existing folder is never reused.
    await user.type(name, "Notas");
    expect(await within(dialog).findByText(/^ya existe \/code\/notas/)).toBeInTheDocument();
    expect(name).toHaveAttribute("aria-invalid", "true");
    expect(createButton).toBeDisabled();

    await user.clear(name);
    await user.type(name, "Mi tienda");
    expect(await within(dialog).findByText("/code/mi-tienda")).toBeInTheDocument();
    // The account is preselected; the rest waits behind "Más opciones".
    const more = within(dialog).getByRole("button", { name: /Más opciones/ });
    expect(more).toHaveAttribute("aria-expanded", "false");
    expect(within(dialog).queryByRole("combobox", { name: "Cuenta" })).not.toBeInTheDocument();
    await user.click(more);
    expect(within(dialog).getByRole("combobox", { name: "Cuenta" })).toHaveTextContent("Claude personal");
    // The client is set later, from Cuentas: not asked when creating.
    expect(within(dialog).queryByRole("textbox", { name: "Cliente" })).not.toBeInTheDocument();

    await user.click(createButton);
    expect(create).toHaveBeenCalledWith({
      name: "Mi tienda",
      parent: "/code",
      account: "claude-personal",
      protect: false,
    });
    const composer = await screen.findByRole("textbox", { name: "Pídele algo a Mi tienda" });
    await waitFor(() => expect(composer).toHaveFocus());
    expect(window.location.hash).toBe(`#p=${encodeURIComponent("/code/mi-tienda")}&new`);
    expect(within(screen.getByRole("complementary", { name: "Navegación" })).getByRole("button", { name: /^Mi tienda/ })).toHaveAttribute("aria-current", "page");
  });

  it("asks a quick question outside any project: type, Enter, done", async () => {
    const { api, user } = setup();
    const start = vi.spyOn(api, "startConversation");
    await screen.findByRole("heading", { name: /¡Hola, Alex!/ });
    await user.click(screen.getByRole("button", { name: "Pregunta rápida" }));

    expect(await screen.findByRole("heading", { name: "Pregunta rápida" })).toBeInTheDocument();
    expect(window.location.hash).toBe("#c");
    const composer = screen.getByRole("textbox", { name: "¿Qué quieres saber?" });
    await waitFor(() => expect(composer).toHaveFocus());
    // Only the modes that never change files; the default account, preselected.
    await user.click(screen.getByRole("combobox", { name: "Modo" }));
    expect(optionLabels()).toEqual(["Solo preguntar", "Investigar"]);
    await user.keyboard("{Escape}");
    await waitFor(() => expect(composer).not.toHaveFocus());
    expect(screen.getByRole("combobox", { name: "Cuenta de la pregunta" })).toHaveTextContent("Claude personal");
    await user.click(composer);
    expect(screen.queryByRole("tab", { name: "Cambios" })).not.toBeInTheDocument();

    await user.type(composer, "¿Qué es un monad?{Enter}");
    expect(start).toHaveBeenCalledWith({ prompt: "¿Qué es un monad?", mode: "ask", account: "claude-personal", model: null });
    expect(await screen.findByRole("heading", { name: "¿Qué es un monad?" })).toBeInTheDocument();
    expect(window.location.hash).toMatch(/^#c=t\d+$/);
    // It shows up under "Chats" in the sidebar.
    const sidebar = screen.getByRole("complementary");
    expect(await within(sidebar).findByRole("button", { name: "¿Qué es un monad?" })).toHaveAttribute("aria-current", "page");
  });

  it("continues and deletes a conversation", async () => {
    history.replaceState(null, "", "#c=c1");
    const { api, user } = setup();
    const remove = vi.spyOn(api, "deleteConversation");
    const follow = vi.spyOn(api, "queueMessage");
    await screen.findByRole("heading", { name: "¿Qué diferencia hay entre OAuth y OpenID Connect?" }, SLOW);
    expect(await screen.findByText(/OpenID Connect va encima/)).toBeInTheDocument();
    expect(screen.getByText(/responde/)).toBeInTheDocument();

    await user.type(screen.getByRole("textbox", { name: "Responde o pide algo más" }), "¿Y en móvil?{Enter}");
    expect(follow).toHaveBeenCalledWith("c1", "¿Y en móvil?");
    expect(await screen.findByText("Hecho: ¿Y en móvil?")).toBeInTheDocument();

    // In the ⋯ menu, confirmed in a dialog.
    await user.click(await screen.findByRole("button", { name: "Más acciones" }));
    await user.click(await screen.findByRole("menuitem", { name: "Quitar esta conversación" }));
    const confirm = screen.getByRole("dialog", { name: "¿Quitar esta conversación?" });
    expect(within(confirm).getByText(/No se puede deshacer/)).toBeInTheDocument();
    await user.click(within(confirm).getByRole("button", { name: "Quitar" }));
    expect(remove).toHaveBeenCalledWith("c1");
    expect(await screen.findByText("Conversación borrada")).toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: "Pregunta rápida" })).toBeInTheDocument();
    expect(await screen.findByText("Pregunta algo sin abrir un proyecto.")).toBeInTheDocument();
  });

  it("opens a conversation from Inicio, not a project", async () => {
    const { user } = setup();
    // An answered conversation waits among what is ready.
    const ready = await screen.findByRole("region", { name: /^Listas para revisar/ }, SLOW);
    await user.click(within(ready).getByRole("button", { name: /OAuth y OpenID Connect/ }));
    expect(await screen.findByRole("heading", { name: "¿Qué diferencia hay entre OAuth y OpenID Connect?" })).toBeInTheDocument();
    expect(window.location.hash).toBe("#c=c1");
  });

  it("turns what was typed in the palette into a question", async () => {
    const { user } = setup();
    await screen.findByRole("heading", { name: /¡Hola, Alex!/ });
    await user.keyboard("{Control>}k{/Control}");
    await user.type(screen.getByRole("combobox", { name: "Paleta de comandos" }), "cómo funciona OIDC");
    const ask = screen.getAllByRole("option").at(-1)!;
    expect(ask).toHaveTextContent("Preguntar «cómo funciona OIDC»");
    await user.click(ask);
    const composer = await screen.findByRole("textbox", { name: "¿Qué quieres saber?" });
    expect(composer).toHaveValue("cómo funciona OIDC");
    await waitFor(() => expect(composer).toHaveFocus());
  });

  it("guides an empty home towards a new project or an existing folder", async () => {
    const api = MockApi.withSampleData();
    for (const p of (await api.overview()).projects.map((p) => p.path)) await api.removeProject(p);
    const { user } = setup(api);
    await screen.findByRole("heading", { name: /¡Hola, Alex!/ });
    expect(within(screen.getByRole("complementary", { name: "Navegación" })).getByText("Sin proyectos todavía.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /^Nuevo proyecto.*Una carpeta nueva/ }));
    expect(screen.getByRole("dialog", { name: "Nuevo proyecto" })).toBeInTheDocument();
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());

    const pick = vi.spyOn(api, "pickFolder");
    await user.click(screen.getByRole("button", { name: /^Abrir carpeta….*Un proyecto que ya tienes/ }));
    expect(pick).toHaveBeenCalled();
    expect(await screen.findByRole("dialog", { name: "Abrir carpeta" })).toBeInTheDocument();
  });
});
