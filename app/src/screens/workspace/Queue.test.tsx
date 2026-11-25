import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";
import { chooseMode } from "../../test/select";

// xterm needs a real canvas; the Terminal tab is not exercised here.
vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

function setup(api = MockApi.withSampleData()) {
  api.delayMs = 0;
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<App api={api} queryClient={queryClient} />);
  return { api, user: userEvent.setup() };
}

/** A new task in hedy, without a plan: it stops at a permission card. */
async function taskAtPermission(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole("button", { name: /^hedy/ }));
  await user.type(await screen.findByLabelText("Pídele algo a hedy"), "Recordatorios");
  await chooseMode(user, "Enseguida");
  await user.click(screen.getByRole("button", { name: "Enviar" }));
  return screen.findByRole("region", { name: "Claude quiere correr un comando" });
}

describe("Message queue", () => {
  it("shows, edits and removes queued messages while the agent works", async () => {
    const { user } = setup(MockApi.withSampleData().withSampleActivity());
    await user.click(await screen.findByRole("button", { name: /^tienda/ }));
    // tienda opens on its home: the running task (s4) is one click away.
    await user.click(await within(screen.getByRole("main")).findByRole("button", { name: /^Revisa los tests del carrito/ }));

    const queue = await screen.findByRole("region", { name: "Mensajes en cola" });
    expect(within(queue).getAllByText("En cola")).toHaveLength(2);
    expect(within(queue).getByText(/se envía como un solo mensaje cuando Claude Opus termine su turno/)).toBeInTheDocument();
    const composer = screen.getByLabelText(/^Escribe mientras Claude Opus trabaja/);

    await user.type(composer, "Usa la rama nueva");
    await user.click(screen.getByRole("button", { name: "Poner en cola" }));
    await waitFor(() => expect(within(queue).getAllByText("En cola")).toHaveLength(3));
    expect(composer).toHaveValue("");

    await user.click(within(queue).getAllByRole("button", { name: "Editar el mensaje en cola" })[2]!);
    const editor = within(queue).getByRole("textbox", { name: "Editar el mensaje en cola" });
    await user.clear(editor);
    await user.type(editor, "Usa la rama de pruebas");
    await user.click(within(queue).getByRole("button", { name: "Guardar" }));
    expect(await within(queue).findByText("Usa la rama de pruebas")).toBeInTheDocument();

    await user.click(within(queue).getAllByRole("button", { name: "Quitar el mensaje de la cola" })[0]!);
    await waitFor(() => expect(within(queue).getAllByText("En cola")).toHaveLength(2));
    expect(within(queue).queryByText(/corre también los tests del checkout/)).not.toBeInTheDocument();
  });

  it("delivers the queue after the card is answered and the turn ends", async () => {
    const { user } = setup();
    const card = await taskAtPermission(user);

    await user.type(screen.getByLabelText(/^Primero responde al permiso de arriba/), "Luego actualiza el README");
    await user.click(screen.getByRole("button", { name: "Enviar después" }));
    const queue = await screen.findByRole("region", { name: "Mensajes en cola" });
    expect(within(queue).getByText("Primero responde la tarjeta de arriba; después se envía la cola.")).toBeInTheDocument();

    await user.click(within(card).getByRole("button", { name: "Permitir" }));
    expect(await screen.findByText("Hecho: Luego actualiza el README")).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Mensajes en cola" })).not.toBeInTheDocument();
  });

  it("keeps the queue on hold after a limit until the user sends it", async () => {
    const { api, user } = setup();
    await taskAtPermission(user);
    await user.type(screen.getByLabelText(/^Primero responde al permiso de arriba/), "Sigue con los correos");
    await user.click(screen.getByRole("button", { name: "Enviar después" }));

    const task = (await api.tasks("/code/hedy"))[0]!;
    api.simulateLimit(task.id, "reached");
    const queue = await screen.findByRole("region", { name: "Mensajes en cola" });
    expect(await within(queue).findByText("En pausa")).toBeInTheDocument();
    expect(within(queue).getByText(/la cuenta llegó a su límite. Hedy no lo envía solo/)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Detener" }));
    await user.click(await within(queue).findByRole("button", { name: "Enviar ahora" }));
    expect(await screen.findByText("Hecho: Sigue con los correos")).toBeInTheDocument();
  });

  it("counts queued messages on Inicio's board", async () => {
    // Actividad's old address lands on Inicio.
    window.history.replaceState(null, "", "#activity");
    const { user } = setup(MockApi.withSampleData().withSampleActivity());
    await user.click(await screen.findByRole("radio", { name: /^En marcha/ }, { timeout: 8000 }));
    const running = await screen.findByRole("region", { name: "En marcha (1)" }, { timeout: 8000 });
    expect(within(running).getByText("2 en cola")).toBeInTheDocument();
    window.history.replaceState(null, "", "#");
  });
});
