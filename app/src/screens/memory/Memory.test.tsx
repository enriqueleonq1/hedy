import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";
import type { Note } from "../../api/types";
import { translator } from "../../i18n";
import { keepLabel } from "./notes";

vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

const HEDY = `#m=${encodeURIComponent("/code/hedy")}`;

function renderAt(hash: string, api = MockApi.withSampleData()) {
  api.delayMs = 0;
  history.replaceState(null, "", hash);
  render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
  return { api, user: userEvent.setup() };
}

/** Memoria for hedy, from the project's own sections. */
async function setup() {
  const { api, user } = renderAt("#");
  await user.click(await screen.findByRole("button", { name: /^hedy/ }));
  const sections = await screen.findByRole("navigation", { name: "Secciones del proyecto" });
  await user.click(within(sections).getByRole("button", { name: "Memoria" }));
  await screen.findByRole("heading", { level: 1, name: "Memoria de hedy" });
  await screen.findByRole("region", { name: "Decisiones" });
  return { api, user };
}

/** Memoria's "Por aprobar" tab for hedy. */
async function pending() {
  const out = renderAt(`${HEDY}&pending`);
  const tab = await screen.findByRole("tab", { name: /^Por aprobar/ });
  expect(tab).toHaveAttribute("aria-selected", "true");
  await screen.findByText("Los formularios validan con zod");
  return out;
}

const tab = (name: RegExp) => screen.getByRole("tab", { name });
const badge = () => tab(/^Por aprobar/).querySelector(".memory__badge");
const card = (text: string) => {
  const li = screen.getByText(text).closest("li.note");
  if (!(li instanceof HTMLElement)) throw new Error(`no card for ${text}`);
  return li;
};

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

  it("groups notes by kind with a hint, and shows what the last task used and who reads it", async () => {
    await setup();
    expect(window.location.hash).toBe(HEDY);
    expect(screen.getByRole("combobox", { name: "Para" })).toHaveDisplayValue("hedy");
    expect(tab(/^Notas/)).toHaveAttribute("aria-selected", "true");
    const decisions = screen.getByRole("region", { name: "Decisiones" });
    expect(within(decisions).getByText("Algo que elegimos.")).toBeInTheDocument();
    expect(within(screen.getByRole("region", { name: "Reglas" })).getByText("Obligatorio.")).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Tuyas · en todos los proyectos" })).toBeInTheDocument();
    // Proposals wait in "Por aprobar", not among the notes.
    expect(screen.queryByRole("region", { name: "Propuestas por tus IAs · para aprobar" })).toBeNull();
    expect(screen.queryByText("Los formularios validan con zod")).toBeNull();
    const used = screen.getByRole("region", { name: "Usada en la última tarea" });
    expect(within(used).getByText(/Los precios se guardan en USD/)).toBeInTheDocument();
    const stale = screen.getByRole("region", { name: "Para revisar" });
    expect(within(stale).getByText("El deploy se hace los viernes")).toBeInTheDocument();

    const readers = screen.getByRole("region", { name: "Quién la lee" });
    await userEvent.click(within(readers).getByRole("switch", { name: "Claude personal lee la memoria" }));
    expect(await within(readers).findByText("apagada")).toBeInTheDocument();
  });

  it("puts the contradiction first and resolves it by keeping one note", async () => {
    const { user } = await setup();
    const contradiction = screen.getByRole("region", { name: "Estas dos notas se contradicen" });
    // Before the groups of notes.
    expect(
      contradiction.compareDocumentPosition(screen.getByRole("region", { name: "Decisiones" })) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    await user.click(within(contradiction).getByRole("button", { name: "Quedarme con 24 h" }));
    await waitFor(() => expect(screen.queryByRole("region", { name: "Estas dos notas se contradicen" })).toBeNull());
    expect(screen.queryByText("Los recordatorios se envían 2 h antes de la cita.")).toBeNull();
  });

  it("lists every proposal of the project in Por aprobar, with its count", async () => {
    await pending();
    expect(window.location.hash).toBe(`${HEDY}&pending`);
    // Three notes and two skills, as cards with their context.
    expect(badge()).toHaveTextContent("5");
    expect(screen.getByText("Los formularios validan con zod")).toBeInTheDocument();
    expect(screen.getByText("Cambio de una nota con el mismo tema")).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Parecida a" })).toBeInTheDocument();
    expect(screen.getByRole("listitem", { name: "agregar-endpoint" })).toBeInTheDocument();
    expect(screen.getByRole("listitem", { name: "correr-tests" })).toBeInTheDocument();
    // A proposed card is drawn like any other, solid.
    expect(card("Los formularios validan con zod")).toHaveClass("note--proposed");
  });

  it("says when nothing waits, for a project without proposals", async () => {
    renderAt(`#m=${encodeURIComponent("/code/tienda")}&pending`);
    expect(await screen.findByText("Nada por aprobar.")).toBeInTheDocument();
    // The count is the chosen project's: none.
    expect(badge()).toBeNull();
  });

  it("approves what an AI proposed and adds notes of the user", async () => {
    const { user } = await pending();
    await user.click(within(card("Los formularios validan con zod")).getByRole("button", { name: "Aprobar" }));
    await waitFor(() => expect(screen.queryByText("Los formularios validan con zod")).toBeNull());
    expect(badge()).toHaveTextContent("4");

    await user.click(tab(/^Notas/));
    expect(window.location.hash).toBe(HEDY);
    const conventions = await screen.findByRole("region", { name: "Convenciones" });
    expect(within(conventions).getByText("Los formularios validan con zod")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "+ Nueva nota" }));
    const dialog = screen.getByRole("dialog", { name: "Nueva nota" });
    await user.type(within(dialog).getByLabelText("Nota"), "Nunca subir secretos al repo");
    await user.click(within(dialog).getByRole("radio", { name: "Regla" }));
    await user.click(within(dialog).getByRole("button", { name: "Guardar nota" }));
    expect(await within(screen.getByRole("region", { name: "Reglas" })).findByText("Nunca subir secretos al repo")).toBeInTheDocument();
  });

  it("shows a proposal under a known topic as a change to that note", async () => {
    const { user } = await pending();
    const change = screen.getByText("Cambio de una nota con el mismo tema").closest("li");
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
    await waitFor(() => expect(screen.queryByText("Cambio de una nota con el mismo tema")).toBeNull());
    await user.click(tab(/^Notas/));
    const decisions = await screen.findByRole("region", { name: "Decisiones" });
    expect(within(decisions).getByText(proposal)).toBeInTheDocument();
    expect(within(decisions).queryByText(old)).toBeNull();
    expect(within(decisions).getByText(/actualizada/)).toBeInTheDocument();
  });

  it("rejecting a change leaves the note as it was", async () => {
    const { api, user } = await pending();
    const change = screen.getByText("Cambio de una nota con el mismo tema").closest("li");
    if (!change) throw new Error("no change card");
    await user.click(within(change).getByRole("button", { name: "Rechazar" }));
    await waitFor(() => expect(screen.queryByText("Cambio de una nota con el mismo tema")).toBeNull());
    const view = await api.memory("/code/hedy");
    expect(view.notes.some((n) => n.text === "Las citas usan la zona horaria del salón, no la del servidor")).toBe(true);
    expect(view.proposed.some((p) => p.note.topic === "citas/zona-horaria")).toBe(false);
  });

  it("offers to replace a similar note, keep both or discard the proposal", async () => {
    const { api, user } = await pending();
    const similar = screen.getByRole("region", { name: "Parecida a" });
    const long = "Los precios se guardan en USD y se muestran también en Bs a tasa BCV";
    expect(within(similar).getByText(long)).toBeInTheDocument();
    const proposal = similar.closest("li.note");
    if (!(proposal instanceof HTMLElement)) throw new Error("no proposal card");
    expect(within(proposal).getByRole("button", { name: "Conservar las dos" })).toBeInTheDocument();
    expect(within(proposal).getByRole("button", { name: "Descartar la propuesta" })).toBeInTheDocument();

    await user.click(within(similar).getByRole("button", { name: "Reemplazar esa" }));
    await waitFor(() => expect(screen.queryByRole("region", { name: "Parecida a" })).toBeNull());
    const view = await api.memory("/code/hedy");
    expect(view.notes.find((n) => n.text === "Los precios se guardan en USD")?.uses).toBe(23);
    expect(view.notes.some((n) => n.text === long)).toBe(false);
  });

  it("keeps both notes when the user says so", async () => {
    const { user } = await pending();
    const proposal = screen.getByRole("region", { name: "Parecida a" }).closest("li.note");
    if (!(proposal instanceof HTMLElement)) throw new Error("no proposal card");
    await user.click(within(proposal).getByRole("button", { name: "Conservar las dos" }));
    await waitFor(() => expect(screen.queryByRole("region", { name: "Parecida a" })).toBeNull());
    await user.click(tab(/^Notas/));
    const decisions = await screen.findByRole("region", { name: "Decisiones" });
    expect(within(decisions).getByText("Los precios se guardan en USD")).toBeInTheDocument();
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

  it("searches the project's notes as the user types, and asks only on Enter", async () => {
    const api = MockApi.withSampleData();
    const asked = vi.spyOn(api, "askMemory");
    const { user } = renderAt(HEDY, api);
    await screen.findByRole("region", { name: "Decisiones" });
    const bar = screen.getByRole("searchbox", { name: "Pregunta o busca en tu memoria" });
    expect(bar).toHaveAttribute("placeholder", "Pregunta o busca… (Enter pregunta)");
    await user.type(bar, "pytest");
    const results = await screen.findByRole("region", { name: "Resultados" });
    expect(within(results).getAllByRole("listitem")).toHaveLength(1);
    expect(screen.queryByRole("region", { name: "Decisiones" })).toBeNull();
    expect(asked).not.toHaveBeenCalled();
    expect(screen.queryByRole("region", { name: "Pregunta a tu memoria" })).toBeNull();

    await user.keyboard("{Enter}");
    expect(await screen.findByRole("region", { name: "Pregunta a tu memoria" })).toBeInTheDocument();
    expect(asked).toHaveBeenCalledWith("pytest", { kind: "project", project: "/code/hedy" });
    // The search stays under the answer.
    expect(screen.getByRole("region", { name: "Resultados" })).toBeInTheDocument();
  });

  it("installs the code map in the integrated terminal", async () => {
    const api = new MockApi({ codeMap: { enabled: null, found: null } });
    history.replaceState(null, "", "#settings/saving");
    render(<App api={api} queryClient={new QueryClient()} />);
    const user = userEvent.setup();
    const section = (await screen.findByRole("heading", { name: "Mapa del código" })).closest("section");
    if (!section) throw new Error("no section");
    await user.click(within(section).getByRole("button", { name: "Instalar" }));
    // The terminal panel opens with the official install script.
    expect(await screen.findByText("Instalar mapa del código")).toBeInTheDocument();
  });
});

describe("Memoria's hub", () => {
  afterEach(() => history.replaceState(null, "", "#"));

  it("opens for everything at #memory, with a project to pick for the notes and how it works", async () => {
    const { user } = renderAt("#memory");
    expect(await screen.findByRole("heading", { level: 1, name: "Memoria" })).toBeInTheDocument();
    expect(screen.getByText(/^Notas: hechos y reglas\. Habilidades: pasos para hacer algo\./)).toBeInTheDocument();
    const para = screen.getByRole("combobox", { name: "Para" });
    expect(para).toHaveDisplayValue("Todo");
    expect(within(para).getByRole("option", { name: "hedy" })).toBeInTheDocument();
    await waitFor(() => expect(within(para).getByRole("option", { name: "Equipo · Plataforma" })).toBeInTheDocument());
    const tabs = screen.getByRole("tablist", { name: "Secciones de la memoria" });
    expect(within(tabs).getAllByRole("tab").map((t) => t.textContent?.replace(/\d+$/, ""))).toEqual([
      "Notas",
      "Habilidades",
      "Por aprobar",
      "Mapa",
    ]);
    expect(screen.getByText("Las notas viven en cada proyecto. Elige uno:")).toBeInTheDocument();
    // With everything, the count is every project's.
    await waitFor(() => expect(badge()).toHaveTextContent("5"));
    expect(screen.getByText("Cómo funciona").closest("details")).not.toHaveAttribute("open");
    // Typing with no project does not search anyone's notes.
    await user.type(screen.getByRole("searchbox", { name: "Pregunta o busca en tu memoria" }), "pytest");
    expect(screen.queryByRole("region", { name: "Resultados" })).toBeNull();

    await user.click(screen.getByRole("button", { name: "hedy" }));
    expect(window.location.hash).toBe(HEDY);
    expect(await screen.findByRole("region", { name: "Decisiones" })).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Para" })).toHaveDisplayValue("hedy");
  });

  it.each([
    ["#memory=pending", /^Por aprobar/, "Todo"],
    ["#memory=map", /^Mapa/, "Todo"],
    [`${HEDY}&skills`, /^Habilidades/, "hedy"],
    [`${HEDY}&pending`, /^Por aprobar/, "hedy"],
    [`${HEDY}&map`, /^Mapa/, "hedy"],
    // The addresses Memoria's parts had before.
    ["#skills", /^Habilidades/, "Todo"],
    ["#ask", /^Notas/, "Todo"],
    [`#ask=${encodeURIComponent("/code/hedy")}`, /^Notas/, "hedy"],
    [`#k=${encodeURIComponent("/code/hedy")}`, /^Habilidades/, "hedy"],
  ])("lands %s in Memoria's right section", async (hash, selected, para) => {
    renderAt(hash);
    // In a project the heading names it, as Resumen and Ajustes do.
    expect(await screen.findByRole("heading", { level: 1, name: para === "Todo" ? "Memoria" : "Memoria de hedy" })).toBeInTheDocument();
    await waitFor(() => expect(tab(selected)).toHaveAttribute("aria-selected", "true"));
    expect(screen.getByRole("combobox", { name: "Para" })).toHaveDisplayValue(para);
  });

  it("writes each section's address as the user moves between tabs", async () => {
    const { user } = renderAt(HEDY);
    await screen.findByRole("region", { name: "Decisiones" });
    await user.click(tab(/^Habilidades/));
    expect(window.location.hash).toBe(`${HEDY}&skills`);
    await user.click(tab(/^Por aprobar/));
    expect(window.location.hash).toBe(`${HEDY}&pending`);
    await user.click(tab(/^Mapa/));
    expect(window.location.hash).toBe(`${HEDY}&map`);
    await user.click(tab(/^Notas/));
    expect(window.location.hash).toBe(HEDY);
  });

  it("switches Para between everything, a project and a team, and keeps the tab", async () => {
    const { user } = renderAt("#memory=skills");
    const para = await screen.findByRole("combobox", { name: "Para" });
    // Everything: the whole library.
    expect(await screen.findByRole("list", { name: "Habilidades" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { level: 1, name: "Habilidades" })).toBeNull();

    await user.selectOptions(para, "hedy");
    expect(window.location.hash).toBe(`${HEDY}&skills`);
    // A project: its skills, a new one and the way back to all of them.
    expect(await screen.findByRole("region", { name: "De este proyecto" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "+ Nueva habilidad" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Ver todas las habilidades" }));
    expect(window.location.hash).toBe("#memory=skills");
    expect(await screen.findByRole("list", { name: "Habilidades" })).toBeInTheDocument();

    await user.click(tab(/^Notas/));
    await waitFor(() => expect(within(screen.getByRole("combobox", { name: "Para" })).getByRole("option", { name: "Equipo · Plataforma" })).toBeInTheDocument());
    await user.selectOptions(screen.getByRole("combobox", { name: "Para" }), "Equipo · Plataforma");
    expect(screen.getByRole("combobox", { name: "Para" })).toHaveDisplayValue("Equipo · Plataforma");
    expect(screen.getByText(/Las notas del equipo se ven dentro de cada proyecto enlazado/)).toBeInTheDocument();

    await user.selectOptions(screen.getByRole("combobox", { name: "Para" }), "Todo");
    expect(screen.getByText("Las notas viven en cada proyecto. Elige uno:")).toBeInTheDocument();
    expect(window.location.hash).toBe("#memory");
  });

  it("opens from the sidebar for the project on screen, else for everything", async () => {
    const { user } = renderAt(`#s=${encodeURIComponent("/code/hedy")}`);
    await screen.findByRole("region", { name: "Tests" });
    const nav = () => screen.getAllByRole("button", { name: /^Memoria/ }).find((b) => b.classList.contains("sidebar__nav"));
    expect(nav()).toBeDefined();
    await user.click(nav() as HTMLElement);
    expect(window.location.hash).toBe(HEDY);
    expect(await screen.findByRole("combobox", { name: "Para" })).toHaveDisplayValue("hedy");
    expect(nav()).toHaveAttribute("aria-current", "page");

    window.location.hash = "#settings";
    await screen.findByRole("heading", { level: 1, name: "General" });
    await user.click(nav() as HTMLElement);
    expect(window.location.hash).toBe("#memory");
    expect(await screen.findByRole("combobox", { name: "Para" })).toHaveDisplayValue("Todo");
  });

  it("moves between its tabs with the arrows; Tab enters on the chosen one", async () => {
    const { user } = renderAt(HEDY);
    await screen.findByRole("region", { name: "Decisiones" });
    const tabs = screen.getByRole("tablist", { name: "Secciones de la memoria" });
    const notes = tab(/^Notas/);
    expect(notes).toHaveAttribute("tabIndex", "0");
    expect(notes).toHaveAttribute("aria-controls", "memory-panel-notes");
    for (const other of within(tabs).getAllByRole("tab").filter((t) => t !== notes)) {
      expect(other).toHaveAttribute("tabIndex", "-1");
      expect(other).not.toHaveAttribute("aria-controls");
    }
    notes.focus();
    await user.keyboard("{ArrowRight}");
    expect(window.location.hash).toBe(`${HEDY}&skills`);
    await waitFor(() => expect(tab(/^Habilidades/)).toHaveFocus());
    expect(tab(/^Habilidades/)).toHaveAttribute("aria-selected", "true");
    await user.keyboard("{End}");
    expect(window.location.hash).toBe(`${HEDY}&map`);
    await user.keyboard("{ArrowRight}");
    expect(window.location.hash).toBe(HEDY);
    await user.keyboard("{ArrowLeft}");
    expect(window.location.hash).toBe(`${HEDY}&map`);
  });

  it("shows the project's tabs for a project, with the way to its other sections", async () => {
    const { user } = renderAt(HEDY);
    const sections = await screen.findByRole("navigation", { name: "Secciones del proyecto" });
    expect(within(sections).getByRole("button", { name: "Memoria" })).toHaveAttribute("aria-current", "page");
    await user.click(within(sections).getByRole("button", { name: "Resumen" }));
    expect(await screen.findByRole("region", { name: "Tests" })).toBeInTheDocument();
    expect(window.location.hash).toBe(`#s=${encodeURIComponent("/code/hedy")}`);
  });

  it("keeps its own title for everything, without the project's tabs", async () => {
    renderAt("#memory");
    expect(await screen.findByRole("heading", { level: 1, name: "Memoria" })).toBeInTheDocument();
    expect(screen.queryByRole("navigation", { name: "Secciones del proyecto" })).toBeNull();
    // Each project to pick has its color.
    const pick = screen.getByText("Las notas viven en cada proyecto. Elige uno:").parentElement as HTMLElement;
    expect(within(pick).getByRole("button", { name: "hedy" }).querySelector(".dot")).not.toBeNull();
  });

  it("says when the notes or the map did not load and tries again", async () => {
    const api = MockApi.withSampleData();
    let failing = true;
    const memory = api.memory.bind(api);
    const graph = api.memoryGraph.bind(api);
    vi.spyOn(api, "memory").mockImplementation((path) => (failing ? Promise.reject("disco lleno") : memory(path)));
    vi.spyOn(api, "memoryGraph").mockImplementation(() => (failing ? Promise.reject("disco lleno") : graph()));
    const { user } = renderAt(HEDY, api);
    const notes = (await screen.findByText("No se pudo leer la memoria.")).closest("[role=alert]") as HTMLElement;
    expect(notes).toHaveTextContent("disco lleno");
    failing = false;
    await user.click(within(notes).getByRole("button", { name: "Reintentar" }));
    expect(await screen.findByRole("region", { name: "Decisiones" })).toBeInTheDocument();

    failing = true;
    await user.click(tab(/^Mapa/));
    const map = (await screen.findByText("No se pudo cargar el mapa.")).closest("[role=alert]") as HTMLElement;
    failing = false;
    await user.click(within(map).getByRole("button", { name: "Reintentar" }));
    await waitFor(() => expect(screen.queryByText("No se pudo cargar el mapa.")).toBeNull());
  });

  it("with no projects yet, offers to start one instead of an empty list", async () => {
    const { user } = renderAt("#memory", new MockApi());
    expect(await screen.findByText("Aún no hay proyectos")).toBeInTheDocument();
    expect(screen.queryByText("Las notas viven en cada proyecto. Elige uno:")).toBeNull();
    await user.click(screen.getByRole("button", { name: "Nuevo proyecto" }));
    expect(await screen.findByRole("dialog", { name: "Nuevo proyecto" })).toBeInTheDocument();
  });
});
