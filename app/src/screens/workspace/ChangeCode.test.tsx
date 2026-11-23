import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";
import { chooseMode } from "../../test/select";

// xterm needs a real canvas; the Terminal tab is not exercised here.
vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

function setup() {
  const api = MockApi.withSampleData();
  api.delayMs = 0;
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<App api={api} queryClient={queryClient} />);
  return { api, user: userEvent.setup() };
}

async function askInHedy(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole("button", { name: /^hedy/ }));
  const composer = await screen.findByLabelText("Pídele algo a hedy");
  await chooseMode(user, "Solo preguntar");
  await user.type(composer, "¿Dónde se crean las citas?{Enter}");
  return screen.findByRole("group", { name: "¿Y ahora?" });
}

describe("A question that goes on to change code", () => {
  it("changes code in the same task from the button under the answer", async () => {
    const { api, user } = setup();
    const next = await askInHedy(user);
    const [asked] = await api.tasks("/code/hedy");

    await user.click(within(next).getByRole("button", { name: "Pasar a cambiar código" }));
    expect(screen.getByText(/Tu próximo mensaje empieza a cambiar código/)).toBeInTheDocument();
    const composer = screen.getByPlaceholderText("¿Qué cambio hago? Ej.: arréglalo");
    await waitFor(() => expect(composer).toHaveFocus());
    expect(screen.getByRole("combobox", { name: "Modo" })).toHaveTextContent("Cambiar código · con plan");

    await user.type(composer, "Arréglalo{Enter}");
    expect(await screen.findByRole("separator", { name: /Desde aquí cambia código, en la rama hedy\// })).toBeInTheDocument();
    // What came before still reads as an answer; what follows as changing code.
    expect(document.querySelector(".phase-divider")).toHaveTextContent("responde");
    expect(await screen.findByText("planifica", { exact: false })).toBeInTheDocument();
    const [changed] = await api.tasks("/code/hedy");
    expect(changed?.id).toBe(asked?.id);
    expect(changed?.mode).toBe("project");
    // It started with the plan, as the composer said.
    expect(await screen.findByText(/Pasar el correo a minúsculas/)).toBeInTheDocument();
    // A task that changes code keeps its mode: no selector any more.
    expect(screen.queryByRole("combobox", { name: "Modo" })).toBeNull();
  });

  it("changes code from the mode menu, and can go back to asking", async () => {
    const { api, user } = setup();
    await askInHedy(user);
    const modes = screen.getByRole("combobox", { name: "Modo" });
    expect(modes).toHaveTextContent("Solo preguntar");
    await user.click(modes);
    // Its own mode and changing code: never the other question mode.
    expect(screen.queryByRole("option", { name: "Investigar" })).toBeNull();
    await user.click(screen.getByRole("option", { name: "Cambiar código" }));
    expect(screen.getByText(/Tu próximo mensaje empieza a cambiar código/)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Seguir preguntando" }));
    expect(screen.queryByText(/Tu próximo mensaje empieza a cambiar código/)).toBeNull();
    expect(modes).toHaveTextContent("Solo preguntar");

    await chooseMode(user, "Cambiar código");
    await chooseMode(user, "Enséñame el plan primero");
    await user.type(screen.getByPlaceholderText("¿Qué cambio hago? Ej.: arréglalo"), "Arréglalo{Enter}");
    await screen.findByRole("separator", { name: /Desde aquí cambia código/ });
    const [changed] = await api.tasks("/code/hedy");
    expect(changed?.stages.map((s) => s.phase)).toEqual(["implement"]);
  });
});
