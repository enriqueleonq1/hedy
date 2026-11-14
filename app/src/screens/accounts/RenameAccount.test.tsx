import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";

vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

describe("Renaming an account", () => {
  afterEach(() => history.replaceState(null, "", "#"));

  it("renames in place, keeps the id and cancels with Escape", async () => {
    const api = MockApi.withSampleData();
    const first = (await api.overview()).accounts[0]!;
    history.replaceState(null, "", "#accounts");
    render(<App api={api} queryClient={new QueryClient()} />);
    const user = userEvent.setup();

    const card = await screen.findByRole("article", { name: first.name });
    await user.click(within(card).getByRole("button", { name: `Cambiar el nombre de ${first.name}` }));
    let input = within(card).getByRole("textbox", { name: "Nombre de la cuenta" });
    await user.clear(input);
    await user.type(input, "Mi cuenta{Escape}");
    expect(within(card).getByRole("heading", { name: first.name })).toBeInTheDocument();

    await user.click(within(card).getByRole("button", { name: `Cambiar el nombre de ${first.name}` }));
    input = within(card).getByRole("textbox", { name: "Nombre de la cuenta" });
    await user.clear(input);
    await user.type(input, "  Mi cuenta {Enter}");
    await screen.findByRole("article", { name: "Mi cuenta" });
    const renamed = (await api.overview()).accounts.find((a) => a.id === first.id);
    expect(renamed?.name).toBe("Mi cuenta");
  });
});
