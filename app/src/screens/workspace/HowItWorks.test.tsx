import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";

// xterm needs a real canvas; the Terminal tab is not exercised here.
vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

function setup(api = MockApi.withSampleData()) {
  api.delayMs = 0;
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<App api={api} queryClient={queryClient} />);
  return { api, user: userEvent.setup() };
}

async function openHedy(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole("button", { name: /^hedy/ }));
  return screen.findByLabelText("Pídele algo a hedy");
}

describe("How a task that changes code works", () => {
  it("is one AI by default, and a flow can be picked in the mode menu", async () => {
    const { api, user } = setup();
    const start = vi.spyOn(api, "startTask");
    const composer = await openHedy(user);
    const modes = screen.getByRole("combobox", { name: "Modo" });
    await user.click(modes);
    const how = await screen.findByRole("group", { name: "Cómo trabaja" });
    expect(within(how).getByRole("option", { name: "Una IA" })).toBeInTheDocument();
    await user.click(within(how).getByRole("option", { name: "Esencial" }));
    expect(modes).toHaveTextContent("Cambiar código · Esencial");
    // A flow has its own steps: no "plan first" under it.
    await user.click(modes);
    expect(screen.queryByRole("option", { name: "Enséñame el plan primero" })).toBeNull();
    await user.keyboard("{Escape}");

    await user.type(composer, "Corrige el 400 del controlador de citas{Enter}");
    await waitFor(() => expect(start).toHaveBeenCalled());
    expect(start.mock.calls[0]?.[0]).toMatchObject({ flow: "essential", planFirst: false, mode: "project" });
  });

  it("starts with the project's default flow, which the flows tab sets", async () => {
    const api = MockApi.withSampleData();
    await api.setDefaultFlow("/code/hedy", "approval");
    const { user } = setup(api);
    await openHedy(user);
    const modes = screen.getByRole("combobox", { name: "Modo" });
    await waitFor(() => expect(modes).toHaveTextContent("Cambiar código · Con tu aprobación"));
    await user.click(modes);
    expect(screen.getByRole("option", { name: "Con tu aprobación" })).toHaveAccessibleDescription(
      /Por defecto en este proyecto/,
    );
    await user.click(screen.getByRole("option", { name: "Una IA" }));
    expect(modes).toHaveTextContent(/^Cambiar código/);
    expect(modes).not.toHaveTextContent("Con tu aprobación");
  });

  it("says before sending that a flow takes the task on when it ends", async () => {
    const api = MockApi.withSampleData();
    const [entry] = (await api.flows("/code/tienda")).filter((e) => e.scope === "project");
    if (!entry) throw new Error("tienda has a flow");
    await api.saveFlow("/code/tienda", entry.key, { ...entry.flow, triggers: ["manual", "taskEnd"] });
    const { user } = setup(api);
    await user.click(await screen.findByRole("button", { name: /^tienda/ }));
    expect(await screen.findByText(`Al terminar, la retoma el flujo «${entry.flow.name}».`)).toBeInTheDocument();
  });

  it("changes the mode with Shift+Tab", async () => {
    const { user } = setup();
    const composer = await openHedy(user);
    const modes = screen.getByRole("combobox", { name: "Modo" });
    composer.focus();
    await user.keyboard("{Shift>}{Tab}{/Shift}");
    expect(modes).toHaveTextContent("Solo preguntar");
    expect(composer).toHaveFocus();
    await user.keyboard("{Shift>}{Tab}{/Shift}");
    expect(modes).toHaveTextContent("Investigar");
  });
});
