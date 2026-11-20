import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";

vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

async function openAccounts(api = MockApi.withSampleData()) {
  history.replaceState(null, "", "#accounts");
  render(<App api={api} queryClient={new QueryClient()} />);
  const user = userEvent.setup();
  await screen.findByRole("heading", { name: /Tus cuentas/ });
  return { api, user };
}

describe("Renaming an account", () => {
  afterEach(() => history.replaceState(null, "", "#"));

  it("renames in place, keeps the id and cancels with Escape", async () => {
    const api = MockApi.withSampleData();
    const first = (await api.overview()).accounts[0]!;
    const { user } = await openAccounts(api);

    const card = await screen.findByRole("article", { name: first.name });
    const pencil = within(card).getByRole("button", { name: `Cambiar el nombre de ${first.name}` });
    await user.click(pencil);
    let input = within(card).getByRole("textbox", { name: "Nombre de la cuenta" });
    await user.clear(input);
    await user.type(input, "Mi cuenta{Escape}");
    expect(within(card).getByRole("heading", { name: first.name })).toBeInTheDocument();
    // The focus goes back to the pencil, not to the page.
    expect(within(card).getByRole("button", { name: `Cambiar el nombre de ${first.name}` })).toHaveFocus();

    await user.click(within(card).getByRole("button", { name: `Cambiar el nombre de ${first.name}` }));
    input = within(card).getByRole("textbox", { name: "Nombre de la cuenta" });
    await user.clear(input);
    await user.type(input, "  Mi cuenta {Enter}");
    await screen.findByRole("article", { name: "Mi cuenta" });
    const renamed = (await api.overview()).accounts.find((a) => a.id === first.id);
    expect(renamed?.name).toBe("Mi cuenta");
  });

  it("starts renaming from a double-click on the name and from the card's menu", async () => {
    const { user } = await openAccounts();
    const card = await screen.findByRole("article", { name: "Claude trabajo" });

    await user.dblClick(within(card).getByRole("heading", { name: "Claude trabajo" }));
    expect(within(card).getByRole("textbox", { name: "Nombre de la cuenta" })).toHaveFocus();
    await user.keyboard("{Escape}");

    await user.click(within(card).getByRole("button", { name: "Acciones de Claude trabajo" }));
    await user.click(screen.getByRole("menuitem", { name: "Renombrar" }));
    expect(within(card).getByRole("textbox", { name: "Nombre de la cuenta" })).toHaveFocus();
  });
});

describe("An account card", () => {
  afterEach(() => history.replaceState(null, "", "#"));

  it("says once, for every card, where the figures come from", async () => {
    await openAccounts();
    expect(screen.getAllByText(/El límite de cada plan lo informa su CLI/)).toHaveLength(1);
    const card = screen.getByRole("article", { name: "Claude trabajo" });
    expect(within(card).queryByText(/lo que corrió Hedy/)).not.toBeInTheDocument();
  });

  it("keeps the secondary actions in its menu, deleting last and only after confirming", async () => {
    const { api, user } = await openAccounts();
    const remove = vi.spyOn(api, "removeAccount");
    const makeDefault = vi.spyOn(api, "setDefaultAccount");
    const card = screen.getByRole("article", { name: "Claude trabajo" });
    // Signed in: no button on the card itself, everything is in the menu.
    expect(within(card).queryByRole("button", { name: "Reconectar" })).not.toBeInTheDocument();
    expect(within(card).queryByRole("button", { name: "Quitar de Hedy" })).not.toBeInTheDocument();

    await user.click(within(card).getByRole("button", { name: "Acciones de Claude trabajo" }));
    const menu = screen.getByRole("menu", { name: "Acciones de Claude trabajo" });
    expect(within(menu).getAllByRole("menuitem").map((i) => i.textContent)).toEqual([
      "Renombrar",
      "Por defecto",
      "Reconectar",
      "Quitar de Hedy",
    ]);
    expect(within(menu).getByRole("separator")).toBeInTheDocument();
    expect(within(menu).getByRole("menuitem", { name: "Quitar de Hedy" })).toHaveClass("menu__item--danger");
    await user.click(within(menu).getByRole("menuitem", { name: "Por defecto" }));
    expect(makeDefault).toHaveBeenCalledWith("claude-trabajo");

    // Deleting asks first; cancelling keeps the account.
    const codex = screen.getByRole("article", { name: "ChatGPT" });
    await user.click(within(codex).getByRole("button", { name: "Acciones de ChatGPT" }));
    await user.click(screen.getByRole("menuitem", { name: "Quitar de Hedy" }));
    let dialog = screen.getByRole("dialog", { name: "¿Quitar ChatGPT de Hedy?" });
    expect(within(dialog).getByText(/su carpeta con el login no se borra/)).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Cancelar" }));
    expect(remove).not.toHaveBeenCalled();

    await user.click(within(codex).getByRole("button", { name: "Acciones de ChatGPT" }));
    await user.click(screen.getByRole("menuitem", { name: "Quitar de Hedy" }));
    dialog = screen.getByRole("dialog", { name: "¿Quitar ChatGPT de Hedy?" });
    await user.click(within(dialog).getByRole("button", { name: "Quitar de Hedy" }));
    expect(remove).toHaveBeenCalledWith("chatgpt");
    await waitFor(() => expect(screen.queryByRole("article", { name: "ChatGPT" })).not.toBeInTheDocument());
  });

  it("shows sign-in as its one visible action when signed out", async () => {
    const { user } = await openAccounts();
    const codex = screen.getByRole("article", { name: "ChatGPT" });
    expect(within(codex).getByText("Sin sesión")).toBeInTheDocument();
    expect(within(codex).getByRole("button", { name: "Iniciar sesión" })).toBeInTheDocument();
    await user.click(within(codex).getByRole("button", { name: "Acciones de ChatGPT" }));
    const items = within(screen.getByRole("menu")).getAllByRole("menuitem").map((i) => i.textContent);
    expect(items).not.toContain("Reconectar");
    // The default account is not offered to become the default again.
    await user.keyboard("{Escape}");
    await user.click(within(screen.getByRole("article", { name: "Claude personal" })).getByRole("button", { name: "Acciones de Claude personal" }));
    expect(within(screen.getByRole("menu")).queryByRole("menuitem", { name: "Por defecto" })).not.toBeInTheDocument();
  });
});
