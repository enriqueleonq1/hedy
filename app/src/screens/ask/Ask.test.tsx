import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";
import { buildSim, extent, nodeOf, settle } from "./graphLayout";

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
    await user.click(await screen.findByRole("button", { name: "Preguntar a la memoria" }));
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

  it("starts from a project's memory, and offers to write down what is not written", async () => {
    const { user } = renderAt("#m=%2Fcode%2Fhedy");
    await user.click(await screen.findByRole("button", { name: "Preguntar" }));
    expect(window.location.hash).toBe("#ask=%2Fcode%2Fhedy");
    const panel = screen.getByRole("region", { name: "Pregunta a tu memoria" });
    expect(within(panel).getByLabelText("Ámbito")).toHaveDisplayValue(/^Proyecto · hedy$/);
    await user.type(within(panel).getByLabelText("Pregunta"), "¿Qué tipografía usan los correos?");
    await user.click(within(panel).getByRole("button", { name: "Preguntar" }));
    await user.click(await within(panel).findByRole("button", { name: "Anotarlo" }));
    expect(window.location.hash).toBe("#m=%2Fcode%2Fhedy");
  });

  it("refuses an empty question", async () => {
    renderAt("#ask");
    const panel = await screen.findByRole("region", { name: "Pregunta a tu memoria" });
    expect(within(panel).getByRole("button", { name: "Preguntar" })).toBeDisabled();
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
});
