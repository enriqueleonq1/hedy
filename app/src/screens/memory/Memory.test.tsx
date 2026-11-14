import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";
import type { Note } from "../../api/types";
import { keepLabel } from "./notes";

vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

async function setup() {
  const api = MockApi.withSampleData();
  api.delayMs = 0;
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<App api={api} queryClient={queryClient} />);
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: /^hedy/ }));
  await user.click(await screen.findByRole("button", { name: "Memoria" }));
  await screen.findByRole("heading", { name: "Memoria de hedy" });
  return { api, user };
}

const note = (text: string): Note => ({
  id: 1,
  project: null,
  text,
  category: "decision",
  author: "user",
  status: "approved",
  createdAt: 0,
  uses: 0,
  lastUsed: null,
});

describe("Memory", () => {
  afterEach(() => history.replaceState(null, "", "#"));

  it("names the difference between contradicting notes", () => {
    const a = note("Los recordatorios se envían 24 h antes de la cita.");
    const b = note("Los recordatorios se envían 2 h antes de la cita.");
    expect(keepLabel(a, b, "primera")).toBe("Quedarme con 24 h");
    expect(keepLabel(note("Usar Twilio"), note("No usar Twilio"), "primera")).toBe("Quedarme con la primera");
  });

  it("groups notes and shows what the last task used and who reads it", async () => {
    await setup();
    expect(screen.getByRole("region", { name: "Decisiones" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Tuyas · en todos los proyectos" })).toBeInTheDocument();
    const used = screen.getByRole("region", { name: "Usada en la última tarea" });
    expect(within(used).getByText(/Los precios se guardan en USD/)).toBeInTheDocument();
    const stale = screen.getByRole("region", { name: "Para revisar" });
    expect(within(stale).getByText("El deploy se hace los viernes")).toBeInTheDocument();

    const readers = screen.getByRole("region", { name: "Quién la lee" });
    await userEvent.click(within(readers).getByRole("switch", { name: "Claude personal lee la memoria" }));
    expect(await within(readers).findByText("apagada")).toBeInTheDocument();
  });

  it("resolves a contradiction by keeping one note", async () => {
    const { user } = await setup();
    const card = screen.getByRole("region", { name: "Estas dos notas se contradicen" });
    await user.click(within(card).getByRole("button", { name: "Quedarme con 24 h" }));
    await waitFor(() => expect(screen.queryByRole("region", { name: "Estas dos notas se contradicen" })).toBeNull());
    expect(screen.queryByText("Los recordatorios se envían 2 h antes de la cita.")).toBeNull();
  });

  it("approves what an AI proposed and adds notes of the user", async () => {
    const { user } = await setup();
    const proposed = screen.getByRole("region", { name: "Propuestas por tus IAs · para aprobar" });
    expect(within(proposed).getByText("Codex")).toBeInTheDocument();
    await user.click(within(proposed).getByRole("button", { name: "Aprobar" }));
    await waitFor(() => expect(screen.queryByRole("region", { name: /Propuestas/ })).toBeNull());
    expect(within(screen.getByRole("region", { name: "Convenciones" })).getByText("Los formularios validan con zod")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "+ Nueva nota" }));
    const dialog = screen.getByRole("dialog", { name: "Nueva nota" });
    await user.type(within(dialog).getByLabelText("Nota"), "Nunca subir secretos al repo");
    await user.click(within(dialog).getByRole("radio", { name: "Regla" }));
    await user.click(within(dialog).getByRole("button", { name: "Guardar nota" }));
    expect(await within(screen.getByRole("region", { name: "Reglas" })).findByText("Nunca subir secretos al repo")).toBeInTheDocument();
  });

  it("drops an abandoned edit", async () => {
    const { user } = await setup();
    const rules = screen.getByRole("region", { name: "Reglas" });
    await user.click(within(rules).getByRole("button", { name: "Editar" }));
    await user.type(within(rules).getByLabelText("Texto de la nota"), " y nunca borrar");
    await user.click(within(rules).getByRole("button", { name: "Cancelar" }));
    await user.click(within(rules).getByRole("button", { name: "Editar" }));
    expect(within(rules).getByLabelText("Texto de la nota")).toHaveValue("Nunca tocar .env ni correr migraciones en producción");
  });

  it("searches the memory", async () => {
    const { user } = await setup();
    await user.type(screen.getByRole("searchbox", { name: "Buscar en la memoria" }), "pytest");
    const results = await screen.findByRole("region", { name: "Resultados" });
    expect(within(results).getAllByRole("listitem")).toHaveLength(1);
  });

  it("installs the code map in the integrated terminal", async () => {
    const api = new MockApi({ codeMap: { enabled: null, found: null } });
    history.replaceState(null, "", "#settings");
    render(<App api={api} queryClient={new QueryClient()} />);
    const user = userEvent.setup();
    const section = (await screen.findByRole("heading", { name: "Mapa del código" })).closest("section");
    if (!section) throw new Error("no section");
    await user.click(within(section).getByRole("button", { name: "Instalar" }));
    // The terminal panel opens with the official install script.
    expect(await screen.findByText("Instalar mapa del código")).toBeInTheDocument();
  });
});
