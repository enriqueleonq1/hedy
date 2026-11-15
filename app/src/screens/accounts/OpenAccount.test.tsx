import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";

vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

describe("Opening an account from the shell", () => {
  afterEach(() => history.replaceState(null, "", "#"));

  it("opens that account's card from the sidebar and from the top bar", async () => {
    render(<App api={MockApi.withSampleData()} queryClient={new QueryClient()} />);
    const user = userEvent.setup();
    await screen.findByRole("heading", { name: /¡Hola, Enrique!/ });

    const sidebar = screen.getByRole("complementary");
    await user.click(within(sidebar).getByRole("button", { name: /^Claude trabajo: .*Abrir la cuenta$/ }));
    const work = await screen.findByRole("article", { name: "Claude trabajo" });
    expect(work).toHaveClass("account-card--focus");
    expect(work).toHaveFocus();
    expect(window.location.hash).toBe("#a=claude-trabajo");

    await user.click(within(screen.getByRole("group", { name: "Consumo de la semana por proveedor" })).getByRole("button", { name: /^Claude personal: .*Abrir la cuenta$/ }));
    expect(await screen.findByRole("article", { name: "Claude personal" })).toHaveClass("account-card--focus");
    expect(screen.getByRole("article", { name: "Claude trabajo" })).not.toHaveClass("account-card--focus");
  });
});
