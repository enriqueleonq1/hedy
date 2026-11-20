import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";
import type { ThemeSetting } from "../../api/types";

// xterm needs a real canvas; the terminal is not exercised here.
vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

const ONE_DARK_LIKE = `{
  // A VS Code theme file, comments and trailing commas included
  "name": "One Dark Pro",
  "type": "dark",
  "colors": {
    "editor.background": "#282c34",
    "editor.foreground": "#abb2bf",
    "sideBar.background": "#21252b",
    "textLink.foreground": "#61afef",
    "button.background": "#4d78cc",
  },
  "tokenColors": [
    { "scope": ["keyword.control", "storage.type"], "settings": { "foreground": "#c678dd" } },
    { "scope": "string", "settings": { "foreground": "#98c379" } },
  ],
}`;

async function setup(mode: ThemeSetting = "dark") {
  const api = MockApi.withSampleData();
  api.delayMs = 0;
  await api.setSetting("ui.theme", mode);
  render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
  const user = userEvent.setup();
  await openAppearance(user);
  return { api, user };
}

/** Ajustes, then Apariencia in its index: where the gallery lives. */
async function openAppearance(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole("button", { name: "Ajustes" }));
  const index = await screen.findByRole("navigation", { name: "Secciones de Ajustes" });
  await user.click(within(index).getByRole("button", { name: "Apariencia" }));
  await screen.findByRole("heading", { name: "Temas" });
}

const cssVar = (name: string) => document.documentElement.style.getPropertyValue(name);
const card = (name: string) => screen.getByRole("button", { name: new RegExp(`^${name}`) });
const bar = () => screen.queryByRole("region", { name: "Vista previa del tema" });

describe("Theme gallery", () => {
  it("previews a theme across the app, applies it and undoes it", async () => {
    const { api, user } = await setup();
    const setSetting = vi.spyOn(api, "setSetting");
    expect(card("Hedy Dark")).toHaveAttribute("aria-pressed", "true");
    expect(within(card("Hedy Dark")).getByText("En uso")).toBeInTheDocument();

    await user.click(card("Dracula"));
    expect(within(bar()!).getByText("Vista previa: Dracula")).toBeInTheDocument();
    expect(within(bar()!).getByText(/Contraste revisado: todo se lee/)).toBeInTheDocument();
    expect(card("Dracula")).toHaveAttribute("aria-pressed", "true");
    expect(within(card("Dracula")).getByText("Viendo")).toBeInTheDocument();
    expect(cssVar("--bg")).toBe("#282A36");
    expect(cssVar("--syn-keyword")).toBe("#FF79C6");
    // Nothing saved while previewing.
    expect(setSetting).not.toHaveBeenCalled();

    // The bar stays while the user looks around the app.
    await user.click(screen.getByRole("button", { name: /^Inicio/ }));
    expect(bar()).toBeInTheDocument();
    expect(cssVar("--bg")).toBe("#282A36");
    await openAppearance(user);

    await user.click(within(bar()!).getByRole("button", { name: "Aplicar tema" }));
    await waitFor(() => expect(bar()).toBeNull());
    expect(setSetting).toHaveBeenCalledWith("ui.theme.dark", "dracula");
    expect(setSetting).not.toHaveBeenCalledWith("ui.theme", expect.anything());
    expect(await screen.findByText("Tema aplicado: Dracula")).toBeInTheDocument();
    expect(cssVar("--bg")).toBe("#282A36");
    await waitFor(() => expect(within(card("Dracula")).getByText("En uso")).toBeInTheDocument());

    await user.click(screen.getByRole("button", { name: "Deshacer" }));
    await waitFor(() => expect(cssVar("--bg")).toBe("#0A0A0C"));
    expect(setSetting).toHaveBeenLastCalledWith("ui.theme.dark", "hedy");
  });

  it("cancels a preview with Escape or Cancelar, restoring the theme", async () => {
    const { user } = await setup();
    await user.click(card("Nord"));
    expect(cssVar("--bg")).toBe("#2E3440");
    await user.keyboard("{Escape}");
    expect(bar()).toBeNull();
    expect(cssVar("--bg")).toBe("#0A0A0C");

    await user.click(card("GitHub Light"));
    expect(document.documentElement.dataset.theme).toBe("light");
    await user.click(within(bar()!).getByRole("button", { name: "Cancelar" }));
    expect(bar()).toBeNull();
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(cssVar("--bg")).toBe("#0A0A0C");
  });

  it("switches a fixed mode to the applied theme's", async () => {
    const { api, user } = await setup("dark");
    const setSetting = vi.spyOn(api, "setSetting");
    await user.click(card("Solarized Light"));
    await user.click(within(bar()!).getByRole("button", { name: "Aplicar tema" }));
    await waitFor(() => expect(setSetting).toHaveBeenCalledWith("ui.theme", "light"));
    expect(setSetting).toHaveBeenCalledWith("ui.theme.light", "solarized-light");
    await waitFor(() => expect(cssVar("--bg")).toBe("#FDF6E3"));
    expect(screen.getByRole("radio", { name: "Claro" })).toHaveAttribute("aria-checked", "true");
  });

  it("marks as in use only the theme of the mode chosen", async () => {
    await setup("light");
    expect(within(card("Hedy Light")).getByText("En uso")).toBeInTheDocument();
    expect(within(card("Hedy Dark")).queryByText("En uso")).toBeNull();
  });

  it("in system mode, marks the theme of each mode as in use", async () => {
    await setup("system");
    expect(within(card("Hedy Dark")).getByText("En uso")).toBeInTheDocument();
    expect(within(card("Hedy Light")).getByText("En uso")).toBeInTheDocument();
  });

  it("in system mode, keeps a theme for the other mode and offers to always use it", async () => {
    const { api, user } = await setup("system");
    const setSetting = vi.spyOn(api, "setSetting");
    await user.click(card("Catppuccin Latte"));
    await user.click(within(bar()!).getByRole("button", { name: "Aplicar tema" }));
    expect(
      await screen.findByText("Tema aplicado: Catppuccin Latte. Se verá cuando el sistema esté en modo claro."),
    ).toBeInTheDocument();
    expect(setSetting).toHaveBeenCalledWith("ui.theme.light", "catppuccin-latte");
    // The OS is dark here: the page stays dark until the user chooses.
    expect(document.documentElement.dataset.theme).toBe("dark");
    await user.click(screen.getByRole("button", { name: "Usar siempre claro" }));
    await waitFor(() => expect(cssVar("--bg")).toBe("#EFF1F5"));
    expect(setSetting).toHaveBeenLastCalledWith("ui.theme", "light");
  });

  it("imports a VS Code theme, previews it and saves it on apply", async () => {
    const { api, user } = await setup();
    const saveTheme = vi.spyOn(api, "saveTheme");
    await user.click(screen.getByRole("button", { name: "Importar de VS Code" }));
    const dialog = screen.getByRole("dialog", { name: "Importar de VS Code" });

    await user.click(within(dialog).getByLabelText("O pega el JSON del tema"));
    await user.paste("not a theme");
    await user.click(within(dialog).getByRole("button", { name: "Ver en Hedy" }));
    expect(within(dialog).getByRole("alert")).toHaveTextContent("No es un JSON válido");

    await user.clear(within(dialog).getByLabelText("O pega el JSON del tema"));
    await user.paste(ONE_DARK_LIKE);
    expect(within(dialog).getByLabelText("Nombre")).toHaveValue("One Dark Pro");
    await user.click(within(dialog).getByRole("button", { name: "Ver en Hedy" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(within(bar()!).getByText("Vista previa: One Dark Pro")).toBeInTheDocument();
    expect(cssVar("--bg")).toBe("#282C34");
    expect(saveTheme).not.toHaveBeenCalled();

    // "Ajustar" goes back to the dialog as it was.
    await user.click(within(bar()!).getByRole("button", { name: "Ajustar" }));
    const again = screen.getByRole("dialog", { name: "Importar de VS Code" });
    expect(within(again).getByLabelText("O pega el JSON del tema")).toHaveValue(ONE_DARK_LIKE);
    await user.click(within(again).getByRole("button", { name: "Cancelar" }));
    expect(bar()).toBeInTheDocument();

    await user.click(within(bar()!).getByRole("button", { name: "Guardar y aplicar" }));
    await waitFor(() => expect(bar()).toBeNull());
    expect(saveTheme).toHaveBeenCalledWith(
      expect.objectContaining({ id: null, name: "One Dark Pro", base: "dark", origin: "vscode" }),
    );
    const saved = (await api.listThemes()).find((t) => t.name === "One Dark Pro");
    expect(saved?.colors.bg).toBe("#282C34");
    expect((await api.overview()).themeDark).toBe(`custom:${saved?.id}`);
    expect(await screen.findByRole("button", { name: /^One Dark Pro.*De VS Code/ })).toHaveAttribute("aria-pressed", "true");
  });

  it("describes a theme with the chosen account and previews it", async () => {
    const { api, user } = await setup();
    const describe = vi.spyOn(api, "describeTheme");
    await user.click(screen.getByRole("button", { name: "Describir con IA" }));
    const dialog = screen.getByRole("dialog", { name: "Describir con IA" });
    expect(within(dialog).getByText("Una pregunta corta a tu propia cuenta (unos pocos tokens).")).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Cálido, tipo café" }));
    expect(within(dialog).getByLabelText("¿Cómo quieres que se vea Hedy?")).toHaveValue("Cálido, tipo café");
    await user.click(within(dialog).getByRole("button", { name: "Generar" }));

    expect(await screen.findByText("Vista previa: Cálido, tipo café")).toBeInTheDocument();
    expect(describe).toHaveBeenCalledWith("claude-personal", "Cálido, tipo café", null);
    expect(cssVar("--bg")).toBe("#2A211C");
    expect(within(bar()!).getByRole("button", { name: "Guardar y aplicar" })).toBeInTheDocument();
  });

  it("says when the account hit its limit and never tries another one", async () => {
    const { api, user } = await setup();
    api.describeFailure = "Claude personal llegó a su límite de uso";
    const describe = vi.spyOn(api, "describeTheme");
    await user.click(screen.getByRole("button", { name: "Describir con IA" }));
    const dialog = screen.getByRole("dialog", { name: "Describir con IA" });
    await user.type(within(dialog).getByLabelText("¿Cómo quieres que se vea Hedy?"), "Oscuro y azul");
    await user.click(within(dialog).getByRole("button", { name: "Generar" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "Claude personal llegó a su límite. Elige otra cuenta o espera a que se renueve.",
    );
    expect(describe).toHaveBeenCalledTimes(1);
    expect(bar()).toBeNull();
  });

  it("renames and deletes a theme of the user's own", async () => {
    const { user } = await setup();
    await user.click(screen.getByRole("button", { name: "Opciones de Café de la tarde" }));
    await user.click(screen.getByRole("menuitem", { name: "Renombrar" }));
    const rename = screen.getByRole("dialog", { name: "Renombrar tema" });
    await user.clear(within(rename).getByLabelText("Nombre"));
    await user.type(within(rename).getByLabelText("Nombre"), "Café de noche");
    await user.click(within(rename).getByRole("button", { name: "Guardar" }));
    expect(await screen.findByRole("button", { name: /^Café de noche/ })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Opciones de Café de noche" }));
    await user.click(screen.getByRole("menuitem", { name: "Quitar" }));
    const confirm = screen.getByRole("dialog", { name: "¿Quitar «Café de noche»?" });
    await user.click(within(confirm).getByRole("button", { name: "Quitar" }));
    await waitFor(() => expect(screen.queryByRole("button", { name: /^Café de noche/ })).toBeNull());
    expect(await screen.findByText("Tema quitado: Café de noche")).toBeInTheDocument();
  });

  it("falls back to Hedy's theme when the theme in use is deleted", async () => {
    const { api, user } = await setup();
    const [cafe] = await api.listThemes();
    await user.click(card("Café de la tarde"));
    await user.click(within(bar()!).getByRole("button", { name: "Aplicar tema" }));
    await waitFor(() => expect(cssVar("--bg")).toBe(cafe?.colors.bg));
    await user.click(screen.getByRole("button", { name: "Opciones de Café de la tarde" }));
    await user.click(screen.getByRole("menuitem", { name: "Quitar" }));
    const confirm = screen.getByRole("dialog", { name: "¿Quitar «Café de la tarde»?" });
    expect(within(confirm).getByText("Ahora está en uso: se volverá a Hedy Dark.")).toBeInTheDocument();
    await user.click(within(confirm).getByRole("button", { name: "Quitar" }));
    await waitFor(() => expect(cssVar("--bg")).toBe("#0A0A0C"));
    expect((await api.overview()).themeDark).toBe("hedy");
  });

  it("opens the gallery from the command palette", async () => {
    const api = MockApi.withSampleData();
    api.delayMs = 0;
    render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
    const user = userEvent.setup();
    await screen.findByRole("heading", { name: /¡Hola, Alex!/ });
    await user.keyboard("{Control>}k{/Control}");
    await user.type(screen.getByRole("combobox", { name: "Paleta de comandos" }), "cambiar tema");
    await user.keyboard("{Enter}");
    expect(await screen.findByRole("heading", { name: "Temas" })).toHaveFocus();
  });
});
