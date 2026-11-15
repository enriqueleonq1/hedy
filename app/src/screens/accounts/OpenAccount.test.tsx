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

    // The top bar's indicator opens every account's windows; a row opens that card.
    const indicator = screen.getByRole("button", { name: /^Uso de los planes: / });
    expect(indicator).toHaveAttribute("aria-expanded", "false");
    await user.click(indicator);
    const popover = screen.getByRole("dialog", { name: "Uso de los planes" });
    expect(within(popover).getAllByRole("button", { name: /Abrir la cuenta$/ })).toHaveLength(4);
    // Every account, not only one per provider, with each window and when it renews.
    const workRow = within(popover).getByRole("button", { name: "Claude trabajo: Sesión 5 h · 34%, Semana · 29%. Abrir la cuenta" });
    expect(within(workRow).getAllByText(/^Se renueva /)).toHaveLength(2);
    await user.click(within(popover).getByRole("button", { name: /^Claude personal: .*Abrir la cuenta$/ }));
    expect(screen.queryByRole("dialog", { name: "Uso de los planes" })).not.toBeInTheDocument();
    expect(await screen.findByRole("article", { name: "Claude personal" })).toHaveClass("account-card--focus");
    expect(screen.getByRole("article", { name: "Claude trabajo" })).not.toHaveClass("account-card--focus");
  });

  it("closes the usage popover with Escape and gives the focus back", async () => {
    render(<App api={MockApi.withSampleData()} queryClient={new QueryClient()} />);
    const user = userEvent.setup();
    await screen.findByRole("heading", { name: /¡Hola, Enrique!/ });
    const indicator = screen.getByRole("button", { name: /^Uso de los planes: / });
    await user.click(indicator);
    expect(indicator).toHaveAttribute("aria-expanded", "true");
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "Uso de los planes" })).not.toBeInTheDocument();
    expect(indicator).toHaveFocus();
  });
});
