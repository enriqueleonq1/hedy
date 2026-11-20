import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";
import { lineDiff } from "./skills";

vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

async function setup(api = MockApi.withSampleData()) {
  api.delayMs = 0;
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<App api={api} queryClient={queryClient} />);
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: /^hedy/ }));
  const sections = await screen.findByRole("navigation", { name: "Secciones del proyecto" });
  await user.click(within(sections).getByRole("button", { name: "Memoria" }));
  await screen.findByRole("heading", { name: "Memoria de hedy" });
  await user.click(screen.getByRole("tab", { name: /^Habilidades/ }));
  return { api, user };
}

describe("Skills", () => {
  afterEach(() => history.replaceState(null, "", "#"));

  it("diffs two bodies line by line", () => {
    expect(lineDiff("a\nb\nc", "a\nc\nd")).toEqual([
      { text: "a", kind: "same" },
      { text: "b", kind: "removed" },
      { text: "c", kind: "same" },
      { text: "d", kind: "added" },
    ]);
    expect(lineDiff("", "x")).toEqual([
      { text: "", kind: "removed" },
      { text: "x", kind: "added" },
    ]);
  });

  it("shows proposals, an update as a diff, and approved skills by scope", async () => {
    await setup();
    expect(screen.getByRole("tab", { name: /Habilidades.*2 por aprobar/ })).toHaveAttribute("aria-selected", "true");
    expect(window.location.hash).toBe(`#k=${encodeURIComponent("/code/hedy")}`);
    const proposed = await screen.findByRole("region", { name: "Propuestas por tus IAs · para aprobar" });
    const endpoint = within(proposed).getByRole("listitem", { name: "agregar-endpoint" });
    expect(within(endpoint).getByText("Cómo agregar un endpoint nuevo a la API de citas.")).toBeInTheDocument();
    expect(within(endpoint).getByText(/Crea la ruta en/)).toBeInTheDocument();

    const update = within(proposed).getByRole("listitem", { name: "correr-tests" });
    expect(within(update).getByText("Actualización de una habilidad aprobada")).toBeInTheDocument();
    expect(within(update).getByText("Ahora")).toBeInTheDocument();
    expect(within(update).getByText(/npm run test:e2e/).closest(".skill__line")).toHaveClass("skill__line--added");
    expect(within(update).getByRole("button", { name: "Aprobar cambio" })).toBeInTheDocument();

    const own = screen.getByRole("region", { name: "De este proyecto" });
    expect(within(own).getByRole("listitem", { name: "publicar-version" })).toBeInTheDocument();
    const global = screen.getByRole("region", { name: "Tuyas · en todos los proyectos" });
    expect(within(global).getByText("todos los proyectos")).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Dónde la leen tus IAs" })).toHaveTextContent(".agents/skills");
  });

  it("approves, rejects, edits before approving and deletes", async () => {
    const { user } = await setup();
    const proposed = await screen.findByRole("region", { name: "Propuestas por tus IAs · para aprobar" });
    // An update replaces the approved skill.
    await user.click(within(proposed).getByRole("button", { name: "Aprobar cambio" }));
    expect(await screen.findByText("Habilidad «correr-tests» lista: tus IAs la usan dentro de Hedy.")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("tab", { name: /1 por aprobar/ })).toBeInTheDocument());
    const own = screen.getByRole("region", { name: "De este proyecto" });
    expect(within(own).getAllByRole("listitem", { name: "correr-tests" })).toHaveLength(1);

    // Edit the new proposal, then save and approve it.
    const endpoint = screen.getByRole("listitem", { name: "agregar-endpoint" });
    await user.click(within(endpoint).getByRole("button", { name: "Editar" }));
    const name = screen.getByRole("textbox", { name: /Nombre/ });
    await user.clear(name);
    await user.type(name, "nuevo endpoint");
    await user.click(screen.getByRole("button", { name: "Guardar y aprobar" }));
    expect(await screen.findByText("Habilidad «nuevo-endpoint» lista: tus IAs la usan dentro de Hedy.")).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.queryByRole("region", { name: "Propuestas por tus IAs · para aprobar" })).not.toBeInTheDocument(),
    );

    // Delete asks first.
    const publish = screen.getByRole("listitem", { name: "publicar-version" });
    await user.click(within(publish).getByRole("button", { name: "Quitar" }));
    await user.click(within(publish).getByRole("button", { name: "¿Quitarla?" }));
    expect(await screen.findByText("Habilidad «publicar-version» borrada.")).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByRole("listitem", { name: "publicar-version" })).not.toBeInTheDocument());
  });

  it("rejects a proposal and writes a new skill", async () => {
    const { api, user } = await setup();
    const endpoint = await screen.findByRole("listitem", { name: "agregar-endpoint" });
    await user.click(within(endpoint).getByRole("button", { name: "Rechazar" }));
    await waitFor(() => expect(screen.queryByRole("listitem", { name: "agregar-endpoint" })).not.toBeInTheDocument());

    await user.click(screen.getByRole("button", { name: "+ Nueva habilidad" }));
    const dialog = screen.getByRole("dialog", { name: "Nueva habilidad" });
    const save = within(dialog).getByRole("button", { name: "Guardar habilidad" });
    expect(save).toBeDisabled();
    await user.type(within(dialog).getByRole("textbox", { name: /Nombre/ }), "lint");
    await user.type(within(dialog).getByRole("textbox", { name: /Cuándo usarla/ }), "Antes de cada commit.");
    await user.type(within(dialog).getByRole("textbox", { name: /Pasos/ }), "npm run lint");
    await user.click(within(dialog).getByRole("radio", { name: "Todos los proyectos" }));
    await user.click(save);
    expect(await screen.findByText("Habilidad «lint» lista: tus IAs la usan dentro de Hedy.")).toBeInTheDocument();
    const view = await api.skills("/code/hedy");
    expect(view.approved.find((s) => s.name === "lint")?.project).toBeNull();
  });

  it("shows the mascot when there are no skills, and protected projects stay apart", async () => {
    const api = MockApi.withSampleData();
    api.delayMs = 0;
    for (const skill of (await api.skills("/code/hedy")).approved) await api.removeSkill(skill.id);
    for (const p of (await api.skills("/code/hedy")).proposed) await api.rejectSkill(p.skill.id);
    history.replaceState(null, "", `#k=${encodeURIComponent("/code/tienda")}`);
    render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
    expect(await screen.findByText("Todavía no hay habilidades.")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "Durmiendo" })).toBeInTheDocument();
    expect(screen.getByText(/Proyecto protegido: sus habilidades se quedan aquí/)).toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole("button", { name: "+ Nueva habilidad" }));
    expect(screen.queryByRole("radio", { name: "Todos los proyectos" })).not.toBeInTheDocument();
  });
});
