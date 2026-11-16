import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";

vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

const bar = () => screen.getByRole("contentinfo", { name: "Barra de estado" });

describe("Opening an account from the shell", () => {
  afterEach(() => history.replaceState(null, "", "#"));

  it("opens that account's card from the sidebar and from the status bar", async () => {
    render(<App api={MockApi.withSampleData()} queryClient={new QueryClient()} />);
    const user = userEvent.setup();
    await screen.findByRole("heading", { name: /¡Hola, Enrique!/ });

    const sidebar = screen.getByRole("complementary");
    await user.click(within(sidebar).getByRole("button", { name: /^Claude trabajo: .*Abrir la cuenta$/ }));
    const work = await screen.findByRole("article", { name: "Claude trabajo" });
    expect(work).toHaveClass("account-card--focus");
    expect(work).toHaveFocus();
    expect(window.location.hash).toBe("#a=claude-trabajo");

    // Outside a project, the bar names the account closest to its limit; a click opens its card.
    await user.click(within(bar()).getByRole("button", { name: "Claude personal: Semana · 83%. Abrir la cuenta" }));
    expect(await screen.findByRole("article", { name: "Claude personal" })).toHaveClass("account-card--focus");
    expect(screen.getByRole("article", { name: "Claude trabajo" })).not.toHaveClass("account-card--focus");
    expect(window.location.hash).toBe("#a=claude-personal");
  });

  it("names the project's account in its screens, not the one closest to its limit", async () => {
    history.replaceState(null, "", "#p=%2Fcode%2Ftienda");
    render(<App api={MockApi.withSampleData()} queryClient={new QueryClient()} />);
    const user = userEvent.setup();
    const account = await within(await screen.findByRole("contentinfo", { name: "Barra de estado" })).findByRole(
      "button",
      { name: "Claude trabajo: Sesión 5 h · 34%. Abrir la cuenta" },
    );
    await user.click(account);
    expect(await screen.findByRole("article", { name: "Claude trabajo" })).toHaveClass("account-card--focus");
  });
});
