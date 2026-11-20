import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";

vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

async function setup(api = MockApi.withSampleData()) {
  api.delayMs = 0;
  history.replaceState(null, "", "#skills");
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<App api={api} queryClient={queryClient} />);
  const user = userEvent.setup();
  // The old address lands in Memoria's Habilidades tab, for everything.
  await screen.findByRole("heading", { name: "Memoria", level: 1 });
  expect(screen.getByRole("tab", { name: /^Habilidades/ })).toHaveAttribute("aria-selected", "true");
  const list = await screen.findByRole("list", { name: "Habilidades" });
  return { api, user, list };
}

const row = (list: HTMLElement, name: string) =>
  within(list)
    .getAllByRole("button")
    .find((b) => b.textContent?.startsWith(name)) as HTMLElement;

describe("Skills section", () => {
  afterEach(() => history.replaceState(null, "", "#"));

  it("lists every skill with filters and a search, and shows the chosen one in detail", async () => {
    const { user, list } = await setup();
    // Embedded in Memoria: the tab is its title.
    expect(screen.queryByRole("heading", { name: "Habilidades", level: 1 })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Todas · 5" })).toHaveAttribute("aria-pressed", "true");
    expect(within(list).getAllByRole("button")).toHaveLength(5);

    await user.click(screen.getByRole("button", { name: "Propuestas · 2" }));
    expect(within(list).getAllByRole("button")).toHaveLength(2);
    await user.click(screen.getByRole("button", { name: "Globales · 1" }));
    expect(within(list).getAllByRole("button")).toHaveLength(1);
    await user.click(screen.getByRole("button", { name: "Todas · 5" }));

    await user.type(screen.getByRole("searchbox", { name: "Buscar habilidades" }), "publicar");
    expect(within(list).getAllByRole("button")).toHaveLength(1);
    await user.click(row(list, "publicar-version"));
    const detail = screen.getByRole("region", { name: "publicar-version" });
    expect(within(detail).getByText("Lista de pasos para publicar una versión nueva.")).toBeInTheDocument();
    expect(within(detail).getByText("Contexto")).toBeInTheDocument();
    expect(within(detail).getByText("Tiene pasos concretos, con comandos o rutas.")).toBeInTheDocument();
    expect(within(detail).getByRole("button", { name: "Probarla" })).toBeEnabled();
  });

  it("uses a project skill outside Hedy too, but never a global one", async () => {
    const { user, list, api } = await setup();
    await user.click(row(list, "commits-en-ingles"));
    const global = screen.getByRole("region", { name: "commits-en-ingles" });
    expect(within(global).getByRole("switch", { name: "Usar también fuera de Hedy" })).toBeDisabled();

    await user.click(row(list, "publicar-version"));
    const detail = screen.getByRole("region", { name: "publicar-version" });
    await user.click(within(detail).getByRole("switch", { name: "Usar también fuera de Hedy" }));
    expect(await screen.findByText("«publicar-version» también fuera de Hedy: 2 archivos escritos.")).toBeInTheDocument();
    await waitFor(() =>
      expect(within(detail).getByRole("switch", { name: "Usar también fuera de Hedy" })).toHaveAttribute("aria-checked", "true"),
    );
    expect(row(list, "publicar-version")).toHaveTextContent("también fuera de Hedy");
    expect((await api.skillsOverview()).find((s) => s.skill.name === "publicar-version")?.outside).toBe(true);
  });

  it("imports what the user picks after showing what Hedy measured", async () => {
    const { user, list } = await setup();
    await user.click(screen.getByRole("button", { name: "Importar" }));
    const dialog = await screen.findByRole("dialog", { name: "Importar habilidades" });
    await user.click(within(dialog).getByRole("button", { name: "Elegir SKILL.md" }));
    await user.click(within(dialog).getByRole("button", { name: "Ver qué entra" }));
    await within(dialog).findByText("2 habilidades encontradas.");
    const good = within(dialog).getByRole("checkbox", { name: "revisar-pr" });
    const long = within(dialog).getByRole("checkbox", { name: "mega-react-guide" });
    expect(good).toBeChecked();
    expect(within(dialog).getByText("Probablemente dudosa")).toBeInTheDocument();
    await user.click(long);
    expect(long).not.toBeChecked();
    await user.click(within(dialog).getByRole("button", { name: "Importar 1" }));
    expect(await screen.findByText("Una habilidad importada.")).toBeInTheDocument();
    await waitFor(() => expect(row(list, "revisar-pr")).toBeDefined());
  });

  it("tries a skill with and without it and shows the result", async () => {
    const { user, list } = await setup();
    await user.click(row(list, "publicar-version"));
    await user.click(screen.getByRole("button", { name: "Probarla" }));
    const dialog = await screen.findByRole("dialog", { name: "Probar publicar-version" });
    await user.click(within(dialog).getByRole("button", { name: "Que la IA proponga tareas" }));
    await waitFor(() =>
      expect(within(dialog).getByRole("textbox", { name: "Tarea de prueba 1" })).toHaveValue(
        "Usa «publicar-version» para preparar el cambio del carrito.",
      ),
    );
    expect(within(dialog).getByRole("textbox", { name: "Comprobación" })).toHaveValue("npm test");
    expect(within(dialog).getByText(/aislado: sin red/)).toBeInTheDocument();
    expect(within(dialog).getByText(/Son 4 tareas de verdad/)).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Empezar" }));
    expect(await within(dialog).findByText("Tarea 1 · con ella")).toBeInTheDocument();
    expect(
      await within(dialog).findByText("Con ella acierta 2 de 2; sin ella, 1 de 2. Cuesta un 8 % más.", {}, { timeout: 8000 }),
    ).toBeInTheDocument();
    // The footer's, not the dialog's ✕.
    await user.click(within(dialog).getAllByRole("button", { name: "Cerrar" }).at(-1) as HTMLElement);
    await waitFor(() => expect(row(list, "publicar-version")).toHaveTextContent("Útil"));
  }, 15000);

  it("never throws a running trial away on Escape, and can leave it running", async () => {
    const { user, list } = await setup();
    await user.click(row(list, "publicar-version"));
    await user.click(screen.getByRole("button", { name: "Probarla" }));
    const dialog = await screen.findByRole("dialog", { name: "Probar publicar-version" });
    await user.type(within(dialog).getByRole("textbox", { name: "Tarea de prueba 1" }), "Sube la versión");
    await user.click(within(dialog).getByRole("button", { name: "Empezar" }));
    await within(dialog).findByText("Tarea 1 · con ella");
    await user.keyboard("{Escape}");
    expect(screen.getByRole("dialog", { name: "Probar publicar-version" })).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Seguir en segundo plano" }));
    expect(screen.queryByRole("dialog", { name: "Probar publicar-version" })).not.toBeInTheDocument();
    expect(screen.queryByText("Prueba cancelada: no se guardó ningún resultado.")).not.toBeInTheDocument();
    // Its outcome is read and said once, even after leaving the section.
    window.location.hash = "#activity";
    await waitFor(() => expect(screen.queryByRole("list", { name: "Habilidades" })).not.toBeInTheDocument());
    expect(await screen.findByText("Terminó la prueba de «publicar-version»: Útil.", {}, { timeout: 12000 })).toBeInTheDocument();
    window.location.hash = "#skills";
    const again = await screen.findByRole("list", { name: "Habilidades" });
    await waitFor(() => expect(row(again, "publicar-version")).toHaveTextContent("Útil"));
    expect(list.isConnected).toBe(false);
  }, 20000);

  it("imports a skill with a taken name only when replacing it", async () => {
    const api = MockApi.withSampleData();
    const { user } = await setup(api);
    const first = await api.previewSkillImport("/code/hedy", "/home/tu/Descargas/skills/revisar-pr/SKILL.md", false);
    const good = first.candidates.find((c) => c.name === "revisar-pr")!;
    await api.importSkills("/code/hedy", "/home/tu/Descargas/skills/revisar-pr/SKILL.md", false, [
      { file: good.source, fingerprint: good.fingerprint },
    ], []);
    await user.click(screen.getByRole("button", { name: "Importar" }));
    const dialog = await screen.findByRole("dialog", { name: "Importar habilidades" });
    await user.click(within(dialog).getByRole("button", { name: "Elegir SKILL.md" }));
    await user.click(within(dialog).getByRole("button", { name: "Ver qué entra" }));
    const taken = await within(dialog).findByRole("checkbox", { name: "revisar-pr" });
    expect(taken).toBeDisabled();
    await user.click(within(dialog).getByRole("checkbox", { name: "Reemplazar la que ya tienes con ese nombre" }));
    expect(taken).toBeChecked();
  });

  it("exports the chosen approved skills to a folder", async () => {
    const { user } = await setup();
    await user.click(screen.getByRole("button", { name: "Exportar" }));
    const dialog = await screen.findByRole("dialog", { name: "Exportar habilidades" });
    const boxes = within(dialog).getAllByRole("checkbox");
    expect(boxes.length).toBe(3);
    await user.click(within(dialog).getByRole("button", { name: /^Elegir carpeta y exportar/ }));
    expect(await screen.findByText(/exportadas? a \/code\/nuevo-proyecto\./)).toBeInTheDocument();
  });

  it("says why Importar and Exportar are off with no projects and no skills", async () => {
    const api = new MockApi();
    api.delayMs = 0;
    history.replaceState(null, "", "#skills");
    render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
    const exporting = await screen.findByRole("button", { name: "Exportar" });
    await waitFor(() => expect(exporting).toBeDisabled());
    expect(exporting).toHaveAccessibleDescription(/Aún no hay habilidades que exportar\./);
    const importing = screen.getByRole("button", { name: "Importar" });
    expect(importing).toBeDisabled();
    expect(importing).toHaveAccessibleDescription(/Para importar, primero crea o abre un proyecto\./);
  });

  it("says when the skills did not load and tries again", async () => {
    const api = MockApi.withSampleData();
    api.delayMs = 0;
    const real = api.skillsOverview.bind(api);
    vi.spyOn(api, "skillsOverview").mockRejectedValueOnce("sin permiso").mockImplementation(real);
    history.replaceState(null, "", "#skills");
    render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
    const alert = (await screen.findByText("No se pudieron leer las habilidades.")).closest("[role=alert]") as HTMLElement;
    expect(alert).toHaveTextContent("sin permiso");
    await userEvent.setup().click(within(alert).getByRole("button", { name: "Reintentar" }));
    expect(await screen.findByRole("list", { name: "Habilidades" })).toBeInTheDocument();
  });
});
