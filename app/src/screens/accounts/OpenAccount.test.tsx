import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";

vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

const sidebar = () => screen.getByRole("complementary", { name: "Proyectos y cuentas" });

describe("Opening an account from the shell", () => {
  afterEach(() => history.replaceState(null, "", "#"));

  it("opens the account in use's card from the sidebar", async () => {
    render(<App api={MockApi.withSampleData()} queryClient={new QueryClient()} />);
    const user = userEvent.setup();
    await screen.findByRole("heading", { name: /¡Hola, Alex!/ });

    // Outside a project, the sidebar names the default account; a click opens its card.
    await user.click(within(sidebar()).getByRole("button", { name: "Claude personal: Semana · 83%. Abrir la cuenta" }));
    const personal = await screen.findByRole("article", { name: "Claude personal" });
    expect(personal).toHaveClass("account-card--focus");
    expect(personal).toHaveFocus();
    expect(screen.getByRole("article", { name: "Claude trabajo" })).not.toHaveClass("account-card--focus");
    expect(window.location.hash).toBe("#a=claude-personal");
  });

  it("names the project's account in its screens, not the default one", async () => {
    history.replaceState(null, "", "#p=%2Fcode%2Ftienda");
    render(<App api={MockApi.withSampleData()} queryClient={new QueryClient()} />);
    const user = userEvent.setup();
    const side = await screen.findByRole("complementary", { name: "Proyectos y cuentas" });
    const account = await within(side).findByRole("button", { name: "Claude trabajo: Sesión 5 h · 34%. Abrir la cuenta" });
    await user.click(account);
    expect(await screen.findByRole("article", { name: "Claude trabajo" })).toHaveClass("account-card--focus");
    expect(window.location.hash).toBe("#a=claude-trabajo");
  });
});
