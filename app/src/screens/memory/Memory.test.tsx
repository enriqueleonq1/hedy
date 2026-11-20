import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";
import type { Note } from "../../api/types";
import { translator } from "../../i18n";
import { keepLabel } from "./notes";

vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

async function setup() {
  const api = MockApi.withSampleData();
  api.delayMs = 0;
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<App api={api} queryClient={queryClient} />);
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: /^hedy/ }));
  const sections = await screen.findByRole("navigation", { name: "Secciones del proyecto" });
  await user.click(within(sections).getByRole("button", { name: "Memoria" }));
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
  topic: null,
  updatedAt: null,
});

describe("Memory", () => {
  afterEach(() => history.replaceState(null, "", "#"));

  it("names the difference between contradicting notes", () => {
    const a = note("Los recordatorios se envían 24 h antes de la cita.");
    const b = note("Los recordatorios se envían 2 h antes de la cita.");
    const t = translator("es");
    expect(keepLabel(a, b, "first", t)).toBe("Quedarme con 24 h");
    expect(keepLabel(note("Usar Twilio"), note("No usar Twilio"), "first", t)).toBe("Quedarme con la primera");
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
    expect(within(proposed).getAllByText("Codex")).toHaveLength(2);
    await user.click(within(proposed).getByRole("button", { name: "Aprobar" }));
    expect(
      await within(screen.getByRole("region", { name: "Convenciones" })).findByText("Los formularios validan con zod"),
    ).toBeInTheDocument();
    expect(within(proposed).queryByText("Los formularios validan con zod")).toBeNull();

    await user.click(screen.getByRole("button", { name: "+ Nueva nota" }));
    const dialog = screen.getByRole("dialog", { name: "Nueva nota" });
    await user.type(within(dialog).getByLabelText("Nota"), "Nunca subir secretos al repo");
    await user.click(within(dialog).getByRole("radio", { name: "Regla" }));
    await user.click(within(dialog).getByRole("button", { name: "Guardar nota" }));
    expect(await within(screen.getByRole("region", { name: "Reglas" })).findByText("Nunca subir secretos al repo")).toBeInTheDocument();
  });

  it("shows a proposal under a known topic as a change to that note", async () => {
    const { user } = await setup();
    const proposed = screen.getByRole("region", { name: "Propuestas por tus IAs · para aprobar" });
    const change = within(proposed).getByText("Cambio de una nota con el mismo tema").closest("li");
    if (!change) throw new Error("no change card");
    const old = "Las citas usan la zona horaria del salón, no la del servidor";
    const proposal = "Las citas usan la zona horaria del salón; el servidor guarda las fechas en UTC";
    expect(within(change).getByText("Ahora dice")).toBeInTheDocument();
    expect(within(change).getByText(old)).toBeInTheDocument();
    expect(within(change).getByText(proposal)).toBeInTheDocument();
    expect(within(change).getByText("citas/zona-horaria")).toBeInTheDocument();
    // The note it changes is not offered again as a similar one.
    expect(within(change).queryByRole("region", { name: "Parecida a" })).toBeNull();

    await user.click(within(change).getByRole("button", { name: "Aprobar cambio" }));
    const decisions = screen.getByRole("region", { name: "Decisiones" });
    expect(await within(decisions).findByText(proposal)).toBeInTheDocument();
    expect(within(decisions).queryByText(old)).toBeNull();
    expect(within(decisions).getByText(/actualizada/)).toBeInTheDocument();
    expect(screen.queryByText("Cambio de una nota con el mismo tema")).toBeNull();
  });

  it("rejecting a change leaves the note as it was", async () => {
    const { user } = await setup();
    const change = screen.getByText("Cambio de una nota con el mismo tema").closest("li");
    if (!change) throw new Error("no change card");
    await user.click(within(change).getByRole("button", { name: "Rechazar" }));
    await waitFor(() => expect(screen.queryByText("Cambio de una nota con el mismo tema")).toBeNull());
    const decisions = screen.getByRole("region", { name: "Decisiones" });
    expect(within(decisions).getByText("Las citas usan la zona horaria del salón, no la del servidor")).toBeInTheDocument();
  });

  it("offers to replace a similar note, keep both or discard the proposal", async () => {
    const { api, user } = await setup();
    const similar = screen.getByRole("region", { name: "Parecida a" });
    const long = "Los precios se guardan en USD y se muestran también en Bs a tasa BCV";
    expect(within(similar).getByText(long)).toBeInTheDocument();
    const card = similar.closest("li.note");
    if (!(card instanceof HTMLElement)) throw new Error("no proposal card");
    expect(within(card).getByRole("button", { name: "Conservar las dos" })).toBeInTheDocument();
    expect(within(card).getByRole("button", { name: "Descartar la propuesta" })).toBeInTheDocument();

    await user.click(within(similar).getByRole("button", { name: "Reemplazar esa" }));
    await waitFor(() => expect(screen.queryByRole("region", { name: "Parecida a" })).toBeNull());
    const decisions = screen.getByRole("region", { name: "Decisiones" });
    expect(within(decisions).getByText("Los precios se guardan en USD")).toBeInTheDocument();
    expect(within(decisions).queryByText(long)).toBeNull();
    const view = await api.memory("/code/hedy");
    expect(view.notes.find((n) => n.text === "Los precios se guardan en USD")?.uses).toBe(23);
  });

  it("keeps both notes when the user says so", async () => {
    const { user } = await setup();
    const card = screen.getByRole("region", { name: "Parecida a" }).closest("li.note");
    if (!(card instanceof HTMLElement)) throw new Error("no proposal card");
    await user.click(within(card).getByRole("button", { name: "Conservar las dos" }));
    const decisions = screen.getByRole("region", { name: "Decisiones" });
    expect(await within(decisions).findByText("Los precios se guardan en USD")).toBeInTheDocument();
    expect(within(decisions).getByText("Los precios se guardan en USD y se muestran también en Bs a tasa BCV")).toBeInTheDocument();
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
