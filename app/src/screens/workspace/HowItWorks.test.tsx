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
  it("plans first with one AI by default, and a flow can be picked in the mode menu", async () => {
    const { api, user } = setup();
    const start = vi.spyOn(api, "startTask");
    const composer = await openHedy(user);
    const modes = screen.getByRole("combobox", { name: "Modo" });
    await waitFor(() => expect(modes).toHaveTextContent("Cambiar código · tras tu OK"));
    await user.click(modes);
    const when = await screen.findByRole("group", { name: "¿Cuándo empieza a tocar el código?" });
    expect(within(when).getByRole("option", { name: "Enseguida" })).toBeInTheDocument();
    expect(within(when).getByRole("option", { name: "Cuando apruebes el plan" })).toBeInTheDocument();
    const flows = screen.getByRole("group", { name: "O con un flujo de varias IAs" });
    // «Con tu aprobación» is retired: one AI after your OK does the same.
    expect(within(flows).queryByRole("option", { name: "Con tu aprobación" })).toBeNull();
    await user.click(within(flows).getByRole("option", { name: "Esencial" }));
    expect(modes).toHaveTextContent("Cambiar código · Esencial");
    await user.keyboard("{Escape}");

    await user.type(composer, "Corrige el 400 del controlador de citas{Enter}");
    await waitFor(() => expect(start).toHaveBeenCalled());
    expect(start.mock.calls[0]?.[0]).toMatchObject({ flow: "essential", planFirst: false, mode: "project" });
  });

  it("starts right away when asked, for this task only", async () => {
    const { api, user } = setup();
    const start = vi.spyOn(api, "startTask");
    const composer = await openHedy(user);
    const modes = screen.getByRole("combobox", { name: "Modo" });
    await user.click(modes);
    await user.click(await screen.findByRole("option", { name: "Enseguida" }));
    expect(modes).toHaveTextContent(/^Cambiar código$/);
    await user.type(composer, "Sube la versión{Enter}");
    await waitFor(() => expect(start).toHaveBeenCalled());
    expect(start.mock.calls[0]?.[0]).toMatchObject({ planFirst: false, mode: "project" });
    expect(start.mock.calls[0]?.[0]).not.toHaveProperty("flow");
  });

  it("follows the project's way of working", async () => {
    const api = MockApi.withSampleData();
    const project = (await api.overview()).projects.find((p) => p.path === "/code/hedy");
    if (!project) throw new Error("hedy is a sample project");
    await api.setProjectWorking("/code/hedy", { ...project.working, plan: false });
    const { user } = setup(api);
    await openHedy(user);
    const modes = screen.getByRole("combobox", { name: "Modo" });
    await waitFor(() => expect(modes).toHaveTextContent(/^Cambiar código$/));
  });

  it("starts with the project's default flow, which the flows tab sets", async () => {
    const api = MockApi.withSampleData();
    await api.setDefaultFlow("/code/hedy", "essential");
    const { user } = setup(api);
    await openHedy(user);
    const modes = screen.getByRole("combobox", { name: "Modo" });
    await waitFor(() => expect(modes).toHaveTextContent("Cambiar código · Esencial"));
    await user.click(modes);
    expect(screen.getByRole("option", { name: "Esencial" })).toHaveAccessibleDescription(/Por defecto en este proyecto/);
    await user.click(screen.getByRole("option", { name: "Cuando apruebes el plan" }));
    expect(modes).toHaveTextContent("Cambiar código · tras tu OK");
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
