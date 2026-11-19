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

describe("Asking the memory", () => {
  afterEach(() => history.replaceState(null, "", "#"));

  it("answers with cited sources, from the sidebar, and says when nothing is written", async () => {
    const { user } = renderAt("#activity");
    await user.click(await screen.findByRole("button", { name: "Tu memoria" }));
    expect(window.location.hash).toBe("#ask");
    const panel = screen.getByRole("region", { name: "Pregunta a tu memoria" });
    expect(within(panel).getByText(/Pregunta lo que saben tus notas/)).toBeInTheDocument();

    await user.type(within(panel).getByLabelText("Pregunta"), "¿En qué moneda se guardan los precios?");
    await user.click(within(panel).getByRole("button", { name: "Preguntar" }));
    const cite = await within(panel).findAllByRole("button", { name: "Nota 1" });
    expect(cite.length).toBeGreaterThan(0);
    expect(within(panel).getByRole("heading", { name: "Fuentes" })).toBeInTheDocument();
    const source = within(panel).getByRole("list");
    expect(within(source).getByText(/Los precios se guardan en USD/)).toBeInTheDocument();
    expect(within(source).getByText(/Tuya · proyecto hedy/)).toBeInTheDocument();
    expect(within(panel).getByText(/leyó \d+ notas de tu memoria/)).toBeInTheDocument();

    await user.clear(within(panel).getByLabelText("Pregunta"));
    await user.type(within(panel).getByLabelText("Pregunta"), "¿Qué tipografía usan los correos?");
    await user.click(within(panel).getByRole("button", { name: "Preguntar" }));
    expect(await within(panel).findByText("Nada escrito en tu memoria responde eso.")).toBeInTheDocument();
  });

  it("asks a team's notes, with their author, and opens a project's memory to write one down", async () => {
    const { user } = renderAt("#ask");
    const panel = await screen.findByRole("region", { name: "Pregunta a tu memoria" });
    const scope = within(panel).getByLabelText("Ámbito");
    await waitFor(() => expect(within(scope).getByRole("option", { name: "Equipo · Plataforma" })).toBeInTheDocument());
    await user.selectOptions(scope, "Equipo · Plataforma");
    await user.type(within(panel).getByLabelText("Pregunta"), "¿Cómo son los commits?");
    await user.click(within(panel).getByRole("button", { name: "Preguntar" }));
    const sources = await within(panel).findByRole("list");
    expect(within(sources).getByText(/Conventional Commits/)).toBeInTheDocument();
    expect(within(sources).getByText(/Equipo Plataforma · capa del equipo · \w+/)).toBeInTheDocument();
  });

  it("starts from a project's memory, looks in the code for what is not written, or opens the memory", async () => {
    const { api, user } = renderAt("#m=%2Fcode%2Fhedy");
    await user.click(await screen.findByRole("button", { name: "Preguntar" }));
    expect(window.location.hash).toBe("#ask=%2Fcode%2Fhedy");
    const panel = screen.getByRole("region", { name: "Pregunta a tu memoria" });
    expect(within(panel).getByLabelText("Ámbito")).toHaveDisplayValue(/^Proyecto · hedy$/);
    await user.type(within(panel).getByLabelText("Pregunta"), "¿Qué tipografía usan los correos?");
    await user.click(within(panel).getByRole("button", { name: "Preguntar" }));
    // Not written: the project's code answers, and its note waits for approval.
    await user.click(await within(panel).findByRole("button", { name: "Buscar en el código" }));
    expect(await within(panel).findByText(/Inter a 15 px, definida en/)).toBeInTheDocument();
    expect(within(panel).getByText("«Los correos usan Inter a 15 px (emails/base.css).»")).toBeInTheDocument();
    expect((await api.memory("/code/hedy")).proposed.some((p) => p.note.text.includes("Inter"))).toBe(true);
    await user.click(within(panel).getByRole("button", { name: "Guardar en la memoria" }));
    expect(await within(panel).findByRole("status")).toHaveTextContent("Guardada");
    expect((await api.memory("/code/hedy")).notes.some((n) => n.text.includes("Inter"))).toBe(true);

    await user.clear(within(panel).getByLabelText("Pregunta"));
    await user.type(within(panel).getByLabelText("Pregunta"), "¿Qué tipografía usan los boletines?");
    await user.click(within(panel).getByRole("button", { name: "Preguntar" }));
    await user.click(await within(panel).findByRole("button", { name: "Anotarlo" }));
    expect(window.location.hash).toBe("#m=%2Fcode%2Fhedy");
  });

  it("makes chips only of the core's citations", async () => {
    const api = MockApi.withSampleData();
    const real = api.askMemory.bind(api);
    api.askMemory = async (_q, scope) => ({
      ...(await real("¿En qué moneda se guardan los precios?", scope)),
      text: "Desde [2024] usamos `items[2]` y args[0] [1].",
      cited: [1],
    });
    const { user } = renderAt("#ask", api);
    const panel = await screen.findByRole("region", { name: "Pregunta a tu memoria" });
    await user.type(within(panel).getByLabelText("Pregunta"), "¿precios?");
    await user.click(within(panel).getByRole("button", { name: "Preguntar" }));
    await within(panel).findByRole("heading", { name: "Fuentes" });
    const text = panel.querySelector(".ask-answer__text") as HTMLElement;
    expect(within(text).getAllByRole("button").map((b) => b.textContent)).toEqual(["1"]);
    expect(text).toHaveTextContent("Desde [2024] usamos `items[2]` y args[0]");
  });

  it("keeps the asked project for writing it down, whatever the select says now", async () => {
    const { user } = renderAt("#ask=%2Fcode%2Fhedy");
    const panel = await screen.findByRole("region", { name: "Pregunta a tu memoria" });
    await user.type(within(panel).getByLabelText("Pregunta"), "¿Qué tipografía usan los boletines?");
    await user.click(within(panel).getByRole("button", { name: "Preguntar" }));
    await within(panel).findByText("Nada escrito en tu memoria responde eso.");
    await user.selectOptions(within(panel).getByLabelText("Ámbito"), "Todo lo mío");
    await user.click(within(panel).getByRole("button", { name: "Anotarlo" }));
    expect(window.location.hash).toBe("#m=%2Fcode%2Fhedy");
  });

  it("asks all of mine for a project that is not on the list, as the select shows", async () => {
    const api = MockApi.withSampleData();
    const asked = vi.spyOn(api, "askMemory");
    const { user } = renderAt("#ask=%2Fno%2Fexiste", api);
    const panel = await screen.findByRole("region", { name: "Pregunta a tu memoria" });
    expect(within(panel).getByLabelText("Ámbito")).toHaveDisplayValue("Todo lo mío");
    await user.type(within(panel).getByLabelText("Pregunta"), "¿Precios?");
    await user.click(within(panel).getByRole("button", { name: "Preguntar" }));
    await waitFor(() => expect(asked).toHaveBeenCalledWith("¿Precios?", { kind: "mine" }));
  });

  it("refuses an empty question", async () => {
    renderAt("#ask");
    const panel = await screen.findByRole("region", { name: "Pregunta a tu memoria" });
    expect(within(panel).getByRole("button", { name: "Preguntar" })).toBeDisabled();
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
