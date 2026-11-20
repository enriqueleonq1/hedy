import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../App";
import { MockApi } from "../api/MockApi";
import type { PlanState } from "../api/types";

vi.mock("./Terminal/TerminalView", () => ({ TerminalView: () => null }));

const PERSONAL = "Claude personal: Semana · 83%. Abrir la cuenta";
const WORK = "Claude trabajo: Sesión 5 h · 34%. Abrir la cuenta";
const nowSecs = () => Math.floor(Date.now() / 1000);

function setup() {
  const api = MockApi.withSampleData();
  api.delayMs = 0;
  render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
  return { api, user: userEvent.setup() };
}

const sidebar = () => screen.getByRole("complementary", { name: "Proyectos y cuentas" });

describe("Sidebar account", () => {
  afterEach(() => history.replaceState(null, "", "#"));

  // Opening the project's account from the sidebar: screens/accounts/OpenAccount.test.tsx.
  it("names the default account at home, not the one closest to its limit", async () => {
    const { api } = setup();
    await api.setDefaultAccount("claude-trabajo");
    await screen.findByRole("heading", { name: /¡Hola, Alex!/ });
    expect(await within(sidebar()).findByRole("button", { name: WORK })).toBeInTheDocument();
    expect(within(sidebar()).queryByRole("button", { name: PERSONAL })).toBeNull();
  });

  it("says how old the reading is in its tooltip", async () => {
    setup();
    await screen.findByRole("heading", { name: /¡Hola, Alex!/ });
    const account = within(sidebar()).getByRole("button", { name: PERSONAL });
    expect(account.getAttribute("title")).toBe("Claude personal · Semana · 83% · actualizado hace 2 min");
  });

  it("keeps the last figure, muted, while the palette reads the plans again", async () => {
    const { api, user } = setup();
    await screen.findByRole("heading", { name: /¡Hola, Alex!/ });
    const account = within(sidebar()).getByRole("button", { name: PERSONAL });
    // Wait for the reading asked on mount, then hold the next one.
    await waitFor(() => expect(account).not.toHaveAttribute("aria-busy"));
    const plan = (await api.overview()).accounts.find((a) => a.id === "claude-personal")!.usage.plan;
    let answer!: (plan: PlanState) => void;
    const reading = new Promise<PlanState>((resolve) => (answer = resolve));
    const refresh = vi.spyOn(api, "refreshPlanUsage").mockImplementation(async (id) => (id === "claude-personal" ? reading : plan));

    await user.keyboard("{Control>}k{/Control}");
    await user.type(screen.getByRole("combobox", { name: "Paleta de comandos" }), "actualizar uso");
    await waitFor(() => expect(within(screen.getByRole("dialog", { name: "Paleta de comandos" })).getAllByRole("option")[0]).toHaveTextContent("Actualizar uso de los planes"));
    await user.keyboard("{Enter}");
    expect(refresh).toHaveBeenCalledWith("claude-personal");
    await waitFor(() => expect(account).toHaveAttribute("aria-busy", "true"));
    expect(within(account).getByText("83%")).toHaveClass("plan-figure--busy");
    expect(account.getAttribute("title")).toMatch(/ · actualizando…$/);

    answer({ ...plan, attemptedAt: nowSecs() + 5 });
    await waitFor(() => expect(account).not.toHaveAttribute("aria-busy"));
    expect(within(account).getByText("83%")).not.toHaveClass("plan-figure--busy");
  });
});
