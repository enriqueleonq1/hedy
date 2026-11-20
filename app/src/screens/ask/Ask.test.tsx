import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";
import { answerParts, revealHidden } from "./citations";
import { buildSim, extent, nodeOf, remember, settle, SETTLE_TICKS } from "./graphLayout";

vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

function renderAt(hash: string, api = MockApi.withSampleData()) {
  api.delayMs = 0;
  history.replaceState(null, "", hash);
  render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
  return { api, user: userEvent.setup() };
}

const HEDY = `#m=${encodeURIComponent("/code/hedy")}`;

/** Memoria's bar: types the question and asks it with Enter or "Preguntar". */
async function ask(user: ReturnType<typeof userEvent.setup>, question: string, how: "enter" | "button" = "button") {
  const bar = await screen.findByRole("searchbox", { name: "Pregunta o busca en tu memoria" });
  await user.clear(bar);
  await user.type(bar, question);
  if (how === "enter") await user.keyboard("{Enter}");
  else await user.click(screen.getByRole("button", { name: "Preguntar" }));
  return screen.findByRole("region", { name: "Pregunta a tu memoria" });
}

/** A source's number in the list (the answer's chips share its name). */
const sourceButton = (panel: HTMLElement, n: number) =>
  within(panel)
    .getAllByRole("button", { name: `Nota ${n}` })
    .find((b) => b.hasAttribute("aria-expanded")) as HTMLElement;

describe("Asking the memory", () => {
  afterEach(() => history.replaceState(null, "", "#"));

  it("answers with cited sources, from the sidebar, and says when nothing is written", async () => {
    const api = MockApi.withSampleData();
    const asked = vi.spyOn(api, "askMemory");
    const { user } = renderAt("#activity", api);
    await user.click(await screen.findByRole("button", { name: "Memoria" }));
    expect(window.location.hash).toBe("#memory");
    expect(screen.getByRole("combobox", { name: "Para" })).toHaveDisplayValue("Todo");
    expect(screen.queryByRole("region", { name: "Pregunta a tu memoria" })).toBeNull();

    const panel = await ask(user, "¿En qué moneda se guardan los precios?", "enter");
    expect(asked).toHaveBeenLastCalledWith("¿En qué moneda se guardan los precios?", { kind: "mine" });
    const cite = await within(panel).findAllByRole("button", { name: "Nota 1" });
    expect(cite.length).toBeGreaterThan(0);
    expect(within(panel).getByRole("heading", { name: "Fuentes" })).toBeInTheDocument();
    const source = within(panel).getByRole("list");
    expect(within(source).getByText(/Los precios se guardan en USD/)).toBeInTheDocument();
    expect(within(source).getByText(/Tuya · proyecto hedy/)).toBeInTheDocument();
    expect(within(panel).getByText(/leyó \d+ notas de tu memoria/)).toBeInTheDocument();

    await ask(user, "¿Qué tipografía usan los correos?");
    expect(await within(panel).findByText("Nada escrito en tu memoria responde eso.")).toBeInTheDocument();
    // Asked about everything: no project to look in or to write it down in.
    expect(within(panel).queryByRole("button", { name: "Anotarlo" })).toBeNull();
    expect(within(panel).queryByRole("button", { name: "Buscar en el código" })).toBeNull();

    await user.click(within(panel).getByRole("button", { name: "Cerrar" }));
    expect(screen.queryByRole("region", { name: "Pregunta a tu memoria" })).toBeNull();
  });

  it("opens a source in place, with the way to its project and to remove it", async () => {
    const api = MockApi.withSampleData();
    const forget = vi.spyOn(api, "forgetNote");
    const { user } = renderAt("#memory", api);
    const panel = await ask(user, "¿En qué moneda se guardan los precios?");
    await within(panel).findByRole("heading", { name: "Fuentes" });
    const first = sourceButton(panel, 1);
    expect(first).toHaveAttribute("aria-expanded", "false");
    expect(within(panel).queryByRole("button", { name: "Abrir en su proyecto" })).toBeNull();
    await user.click(first);
    expect(first).toHaveAttribute("aria-expanded", "true");
    expect(within(panel).getByRole("button", { name: "Abrir en su proyecto" })).toBeInTheDocument();

    // "Quitar" asks first, then forgets that note.
    await user.click(within(panel).getByRole("button", { name: "Quitar" }));
    await user.click(within(panel).getByRole("button", { name: "¿Quitarla?" }));
    await waitFor(() => expect(forget).toHaveBeenCalledTimes(1));

    // A second click folds it again; a chip in the answer opens it too.
    await user.click(first);
    expect(first).toHaveAttribute("aria-expanded", "false");
    const chip = within(panel.querySelector(".ask-answer__text") as HTMLElement).getAllByRole("button", { name: "Nota 1" })[0]!;
    await user.click(chip);
    expect(first).toHaveAttribute("aria-expanded", "true");

    await user.click(within(panel).getByRole("button", { name: "Abrir en su proyecto" }));
    expect(window.location.hash).toBe(HEDY);
    expect(await screen.findByRole("combobox", { name: "Para" })).toHaveDisplayValue("hedy");
    expect(screen.getByRole("tab", { name: /^Notas/ })).toHaveAttribute("aria-selected", "true");
    expect(await screen.findByRole("region", { name: "Decisiones" })).toBeInTheDocument();
  });

  it("asks a team's notes, with their author", async () => {
    const api = MockApi.withSampleData();
    const asked = vi.spyOn(api, "askMemory");
    const { user } = renderAt("#memory", api);
    const para = await screen.findByRole("combobox", { name: "Para" });
    await waitFor(() => expect(within(para).getByRole("option", { name: "Equipo · Plataforma" })).toBeInTheDocument());
    await user.selectOptions(para, "Equipo · Plataforma");
    const panel = await ask(user, "¿Cómo son los commits?");
    expect(asked).toHaveBeenLastCalledWith("¿Cómo son los commits?", { kind: "team", team: "team-1" });
    const sources = await within(panel).findByRole("list");
    expect(within(sources).getByText(/Conventional Commits/)).toBeInTheDocument();
    expect(within(sources).getByText(/Equipo Plataforma · capa del equipo · \w+/)).toBeInTheDocument();
  });

  it("asks a project's memory, looks in the code for what is not written, or writes it down", async () => {
    const api = MockApi.withSampleData();
    const asked = vi.spyOn(api, "askMemory");
    const { user } = renderAt(HEDY, api);
    await screen.findByRole("region", { name: "Decisiones" });
    const panel = await ask(user, "¿Qué tipografía usan los correos?");
    expect(asked).toHaveBeenLastCalledWith("¿Qué tipografía usan los correos?", { kind: "project", project: "/code/hedy" });
    // Not written: the project's code answers, and its note waits for approval.
    await user.click(await within(panel).findByRole("button", { name: "Buscar en el código" }));
    expect(await within(panel).findByText(/Inter a 15 px, definida en/)).toBeInTheDocument();
    expect(within(panel).getByText("«Los correos usan Inter a 15 px (emails/base.css).»")).toBeInTheDocument();
    expect((await api.memory("/code/hedy")).proposed.some((p) => p.note.text.includes("Inter"))).toBe(true);
    await user.click(within(panel).getByRole("button", { name: "Guardar en la memoria" }));
    expect(await within(panel).findByRole("status")).toHaveTextContent("Guardada");
    expect((await api.memory("/code/hedy")).notes.some((n) => n.text.includes("Inter"))).toBe(true);

    await ask(user, "¿Qué tipografía usan los boletines?");
    await user.click(await within(panel).findByRole("button", { name: "Anotarlo" }));
    const dialog = screen.getByRole("dialog", { name: "Nueva nota" });
    expect(within(dialog).getByLabelText("Nota")).toHaveValue("¿Qué tipografía usan los boletines?");
    await user.clear(within(dialog).getByLabelText("Nota"));
    await user.type(within(dialog).getByLabelText("Nota"), "Los boletines usan Georgia.");
    await user.click(within(dialog).getByRole("button", { name: "Guardar nota" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Nueva nota" })).toBeNull());
    expect((await api.memory("/code/hedy")).notes.some((n) => n.text === "Los boletines usan Georgia." && n.project === "/code/hedy")).toBe(true);
  });

  it("makes chips only of the core's citations", async () => {
    const api = MockApi.withSampleData();
    const real = api.askMemory.bind(api);
    api.askMemory = async (_q, scope) => ({
      ...(await real("¿En qué moneda se guardan los precios?", scope)),
      text: "Desde [2024] usamos `items[2]` y args[0] [1].",
      cited: [1],
    });
    const { user } = renderAt("#memory", api);
    const panel = await ask(user, "¿precios?");
    await within(panel).findByRole("heading", { name: "Fuentes" });
    const text = panel.querySelector(".ask-answer__text") as HTMLElement;
    expect(within(text).getAllByRole("button").map((b) => b.textContent)).toEqual(["1"]);
    expect(text).toHaveTextContent("Desde [2024] usamos `items[2]` y args[0]");
  });

  it("keeps the asked project for writing it down, whatever Para says now", async () => {
    const api = MockApi.withSampleData();
    const { user } = renderAt(HEDY, api);
    const panel = await ask(user, "¿Qué tipografía usan los boletines?");
    await within(panel).findByText("Nada escrito en tu memoria responde eso.");
    const para = screen.getByRole("combobox", { name: "Para" });
    await waitFor(() => expect(within(para).getByRole("option", { name: "Equipo · Plataforma" })).toBeInTheDocument());
    await user.selectOptions(para, "Equipo · Plataforma");
    await user.click(within(panel).getByRole("button", { name: "Anotarlo" }));
    const dialog = screen.getByRole("dialog", { name: "Nueva nota" });
    await user.click(within(dialog).getByRole("button", { name: "Guardar nota" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Nueva nota" })).toBeNull());
    expect((await api.memory("/code/hedy")).notes.some((n) => n.text === "¿Qué tipografía usan los boletines?" && n.project === "/code/hedy")).toBe(true);
  });

  it("never asks about a project that is not on the list: its old address lands in Inicio", async () => {
    const api = MockApi.withSampleData();
    const asked = vi.spyOn(api, "askMemory");
    renderAt("#ask=%2Fno%2Fexiste", api);
    await screen.findByRole("button", { name: /^hedy/ });
    expect(screen.queryByRole("heading", { level: 1, name: "Memoria" })).toBeNull();
    expect(screen.queryByRole("combobox", { name: "Para" })).toBeNull();
    expect(asked).not.toHaveBeenCalled();
  });

  it("drops a code lookup that comes back after a newer question was asked", async () => {
    const api = MockApi.withSampleData();
    let release: (() => void) | undefined;
    const real = api.lookInCode.bind(api);
    api.lookInCode = async (q, p) => {
      await new Promise<void>((resolve) => (release = resolve));
      return real(q, p);
    };
    const { user } = renderAt(HEDY, api);
    const panel = await ask(user, "¿Qué tipografía usan los correos?");
    await user.click(await within(panel).findByRole("button", { name: "Buscar en el código" }));
    await ask(user, "¿En qué moneda se guardan los precios?");
    await within(panel).findByRole("heading", { name: "Fuentes" });
    release?.();
    await new Promise((r) => setTimeout(r, 50));
    expect(within(panel).queryByText(/Según el código del proyecto/)).toBeNull();
  });

  it("refuses an empty question", async () => {
    const { user } = renderAt("#memory");
    const button = await screen.findByRole("button", { name: "Preguntar" });
    expect(button).toBeDisabled();
    await user.type(screen.getByRole("searchbox", { name: "Pregunta o busca en tu memoria" }), "   ");
    expect(button).toBeDisabled();
    await user.keyboard("{Enter}");
    expect(screen.queryByRole("region", { name: "Pregunta a tu memoria" })).toBeNull();
  });

  it("keeps the map under the answer, in the Mapa tab", async () => {
    const { user } = renderAt("#memory=map");
    expect(await screen.findByRole("tab", { name: /^Mapa/ })).toHaveAttribute("aria-selected", "true");
    const map = await within(screen.getByRole("tabpanel")).findByRole("img", { name: /notas/ });
    const panel = await ask(user, "¿En qué moneda se guardan los precios?");
    await within(panel).findByRole("heading", { name: "Fuentes" });
    // Asking does not leave the map (its cited notes light up on the canvas).
    expect(map.isConnected).toBe(true);
    expect(screen.getByRole("tab", { name: /^Mapa/ })).toHaveAttribute("aria-selected", "true");
  });

  it("does not offer a global note when writing down a protected project's answer", async () => {
    const { user } = renderAt(`#m=${encodeURIComponent("/code/tienda")}`);
    const panel = await ask(user, "¿Qué tipografía usan los boletines?");
    await user.click(await within(panel).findByRole("button", { name: "Anotarlo" }));
    const dialog = screen.getByRole("dialog", { name: "Nueva nota" });
    expect(within(dialog).queryByRole("radio", { name: "Todos los proyectos" })).toBeNull();
  });
});

describe("Citations and hidden characters", () => {
  it("reads citations as the core does", () => {
    expect(answerParts("A [1]. B [2, 3] y `x[1]` y v[1] y [9].", [1, 2, 3])).toEqual([
      { text: "A " },
      { cite: [1] },
      { text: ". B " },
      { cite: [2, 3] },
      { text: " y `x[1]` y v[1] y [9]." },
    ]);
    expect(answerParts("Tecla ` y luego [1].", [1])).toEqual([{ text: "Tecla ` y luego " }, { cite: [1] }, { text: "." }]);
  });

  it("shows hidden characters", () => {
    expect(revealHidden("rm\u202e -rf")).toBe("rm⟦U+202E⟧ -rf");
    expect(revealHidden("sin nada")).toBe("sin nada");
  });
});

describe("The graph's layout", () => {
  it("settles linked notes near their hub and loose ones further out, the same every time", async () => {
    const graph = await MockApi.withSampleData().memoryGraph();
    graph.nodes.push(
      ...Array.from({ length: 12 }, (_, i) => ({
        id: `n:${1000 + i}`,
        kind: "note" as const,
        label: `Suelta ${i}`,
        group: "you",
        source: { kind: "local" as const, id: 1000 + i },
        layer: "mine" as const,
      })),
    );
    const a = buildSim(graph);
    settle(a);
    const b = buildSim(graph);
    settle(b);
    expect(a.nodes.map((n) => [Math.round(n.x), Math.round(n.y)])).toEqual(b.nodes.map((n) => [Math.round(n.x), Math.round(n.y)]));
    for (const n of a.nodes) expect(Number.isFinite(n.x) && Number.isFinite(n.y)).toBe(true);
    const centre = extent(a);
    const dist = (n: { x: number; y: number }) => Math.hypot(n.x - centre.x, n.y - centre.y);
    const loose = a.nodes.filter((n) => n.nb.length === 0);
    const linked = a.nodes.filter((n) => n.nb.length > 0 && !n.hub);
    const median = (xs: number[]) => xs.sort((x, y) => x - y)[Math.floor(xs.length / 2)] ?? 0;
    expect(median(loose.map(dist))).toBeGreaterThan(median(linked.map(dist)));
    expect(nodeOf(a, { kind: "local", id: 1000 })?.node.label).toBe("Suelta 0");
  });

  it("starts a memory that changed a little from where it was, and one that did not as it was", async () => {
    const graph = await MockApi.withSampleData().memoryGraph();
    const first = buildSim(graph);
    settle(first);
    remember(first);
    const same = buildSim(graph);
    expect(same.ticks).toBe(SETTLE_TICKS);
    const grown = { ...graph, nodes: [...graph.nodes, { id: "n:5000", kind: "note" as const, label: "Nueva", group: "you", source: { kind: "local" as const, id: 5000 }, layer: "mine" as const }], links: [...graph.links, { source: "n:5000", target: "you" }] };
    const warm = buildSim(grown);
    expect(warm.ticks).toBeGreaterThan(0);
    expect(warm.ticks).toBeLessThan(SETTLE_TICKS);
    const before = first.byId.get("you");
    expect([warm.byId.get("you")?.x, warm.byId.get("you")?.y]).toEqual([before?.x, before?.y]);
  });
});
