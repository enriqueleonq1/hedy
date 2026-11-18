import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";

vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

function renderAt(hash: string, api = MockApi.withSampleData()) {
  api.delayMs = 0;
  history.replaceState(null, "", hash);
  render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
  return { api, user: userEvent.setup() };
}

const section = () => screen.getByRole("heading", { name: "Equipos" }).closest("section")!;

describe("Teams", () => {
  afterEach(() => history.replaceState(null, "", "#"));

  it("lists the teams and opens one: people with their PCs, the invite inline, removing someone", async () => {
    const { api, user } = renderAt("#teams");
    const card = await waitFor(section);
    const row = await within(card).findByRole("button", { name: /Plataforma/ });
    expect(row).toHaveTextContent("2 para revisar");
    expect(row).toHaveTextContent("4 personas");
    await user.click(row);

    expect(await within(card).findByRole("heading", { name: "Plataforma" })).toBeInTheDocument();
    expect(within(card).getByText("Marta")).toBeInTheDocument();
    expect(within(card).getByText("Portátil de Ana")).toBeInTheDocument();
    expect(within(card).getByText(/Capa del equipo: \d+ de 1500 tokens/)).toBeInTheDocument();
    // The open invite is a row of the people list; its code shows when asked.
    expect(within(card).queryByText("7-tigre-lima-nube-faro")).toBeNull();
    expect(within(card).getByText("7-••••")).toBeInTheDocument();
    await user.click(within(card).getByRole("button", { name: "Mostrar" }));
    expect(within(card).getByText("7-tigre-lima-nube-faro")).toBeInTheDocument();
    await user.type(within(card).getByPlaceholderText("Invitar a…"), "Pepe");
    await user.click(within(card).getByRole("button", { name: "Crear invitación" }));
    expect(await within(card).findByText("8-••••")).toBeInTheDocument();
    expect(within(card).getAllByText("Pepe").length).toBeGreaterThan(0);

    // Removing Luis warns about git access.
    const luis = within(card).getByText("Luis").closest("li")!;
    await user.click(within(luis).getByRole("button", { name: "Quitar" }));
    const dialog = screen.getByRole("dialog", { name: "¿Quitar a Luis del equipo?" });
    expect(within(dialog).getByText(/acceso al repo en vuestro servidor git/)).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Quitar a Luis" }));
    await waitFor(() => expect(within(card).queryByText("Luis")).toBeNull());
    expect(api.teamList[0]!.people.some((p) => p.name === "Luis")).toBe(false);
  });

  it("creates a team step by step and asks for a word of its key back", async () => {
    const api = MockApi.withSampleData();
    api.teamList = [];
    const { user } = renderAt("#teams", api);
    const card = await waitFor(section);
    await user.click(within(card).getByRole("button", { name: /Crear un equipo/ }));
    const dialog = screen.getByRole("dialog", { name: "Crear un equipo" });
    await user.type(within(dialog).getByLabelText("Dirección"), "git@github.com:empresa/hedy-team.git");
    await user.click(within(dialog).getByRole("button", { name: "Continuar" }));
    expect(await within(dialog).findByText("Paso 2 de 3")).toBeInTheDocument();
    await user.type(within(dialog).getByLabelText("Nombre del equipo"), "Plataforma");
    await user.type(within(dialog).getByLabelText("Tu nombre en el equipo"), "Alex");
    await user.click(within(dialog).getByRole("button", { name: "Crear el equipo" }));
    expect(await within(dialog).findByLabelText("Clave de recuperación")).toHaveTextContent(MockApi.TEAM_WORDS);
    const finish = within(dialog).getByRole("button", { name: "Terminar" });
    expect(finish).toBeDisabled();
    await user.type(within(dialog).getByLabelText("Para confirmar, escribe la palabra n.º 7"), "rio");
    await user.click(finish);
    expect(await within(section()).findByRole("button", { name: /Plataforma/ })).toHaveTextContent("Admin");
  });

  it("joins with the address and the code, then waits for the team", async () => {
    const api = MockApi.withSampleData();
    api.teamList = [];
    const { user } = renderAt("#teams", api);
    const card = await waitFor(section);
    await user.click(within(card).getByRole("button", { name: /Unirme con una invitación/ }));
    const dialog = screen.getByRole("dialog", { name: "Unirme con una invitación" });
    await user.type(within(dialog).getByLabelText("Dirección del repo"), "git@github.com:empresa/hedy-team.git");
    await user.type(within(dialog).getByLabelText("Código de invitación"), "7-zzz");
    await user.type(within(dialog).getByLabelText("Tu nombre en el equipo"), "Luis");
    await user.click(within(dialog).getByRole("button", { name: "Unirme" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("no es un código de invitación");
    await user.clear(within(dialog).getByLabelText("Código de invitación"));
    await user.type(within(dialog).getByLabelText("Código de invitación"), "7-tigre-lima-nube-faro");
    await user.click(within(dialog).getByRole("button", { name: "Unirme" }));
    expect(await within(dialog).findByText("Esperando al equipo")).toBeInTheDocument();
    await user.click(within(dialog).getAllByRole("button", { name: "Cerrar" }).at(-1)!);
    expect(await within(section()).findByText("Esperando para entrar")).toBeInTheDocument();
    // A join that never gets in can be cancelled.
    await user.click(within(section()).getByRole("button", { name: "Cancelar" }));
    await user.click(within(section()).getByRole("button", { name: /Dejar de entrar/ }));
    await waitFor(() => expect(within(section()).queryByText("Esperando para entrar")).toBeNull());
  });

  it("counts only the suggestions this person may review", async () => {
    const api = MockApi.withSampleData();
    // Luis's own suggestion: his to wait for, not to review.
    for (const s of api.teamList[0]!.suggestions) s.canReview = false;
    const { user } = renderAt("#teams", api);
    const card = await waitFor(section);
    const row = await within(card).findByRole("button", { name: /Plataforma/ });
    expect(row).toHaveTextContent("Al día");
    await user.click(row);
    expect(await within(card).findByRole("heading", { name: "Plataforma" })).toBeInTheDocument();
    expect(within(card).queryByRole("button", { name: "Revisar" })).toBeNull();
  });

  it("reviews suggestions one at a time, from the status bar", async () => {
    const { api, user } = renderAt("#settings");
    const statusBar = await screen.findByRole("contentinfo", { name: "Barra de estado" });
    await user.click(await within(statusBar).findByRole("button", { name: "Equipo · 2 para revisar" }));
    expect(window.location.hash).toBe("#teams");
    const card = await waitFor(section);
    await user.click(await within(card).findByRole("button", { name: /Plataforma/ }));
    await user.click(await within(card).findByRole("button", { name: "Revisar" }));
    const dialog = screen.getByRole("dialog", { name: "Sugerencias por revisar" });
    expect(within(dialog).getByText("Luis sugiere un cambio")).toBeInTheDocument();
    expect(within(dialog).getByText("Los commits siguen Conventional Commits.")).toBeInTheDocument();
    expect(within(dialog).getByText(/en una línea y sin cuerpo/)).toBeInTheDocument();
    expect(within(dialog).getByText(/Ocupa \d+ tokens más/)).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Aceptar" }));
    expect(await within(dialog).findByText("Ana sugiere una nota nueva")).toBeInTheDocument();
    expect(api.teamList[0]!.notes.find((n) => n.uid === "n1")?.text).toMatch(/sin cuerpo/);
  });

  it("filters a team project's memory by layer, with bylines", async () => {
    const api = MockApi.withSampleData();
    await api.teamLinkProject("team-1", "/code/tienda");
    const { user } = renderAt("#m=%2Fcode%2Ftienda", api);
    const filter = await screen.findByRole("radiogroup", { name: "Capa" });
    await user.click(within(filter).getByRole("radio", { name: /^Equipo · 2/ }));
    const team = screen.getByRole("heading", { name: "Memoria del equipo Plataforma" }).closest("section")!;
    expect(within(team).getByText("Los commits siguen Conventional Commits.")).toBeInTheDocument();
    expect(within(team).queryByText("Los importes van en céntimos.")).toBeNull();
    await user.click(within(filter).getByRole("radio", { name: /^Proyecto · 3/ }));
    const ana = within(team).getByText("Los webhooks del banco reintentan 3 veces.").closest("li")!;
    // Someone else's note: a suggestion.
    expect(within(ana).getByRole("button", { name: "Sugerir cambio" })).toBeInTheDocument();
    const mine = within(team).getByText("Los importes van en céntimos.").closest("li")!;
    expect(within(mine).getByRole("button", { name: "Editar" })).toBeInTheDocument();
  });

  it("asks who an agent's proposal is for, and shares it with the project", async () => {
    const api = MockApi.withSampleData();
    await api.teamLinkProject("team-1", "/code/tienda");
    const proposal = api.addProposal("/code/tienda", "Los webhooks del banco son idempotentes.");
    const { user } = renderAt("#m=%2Fcode%2Ftienda", api);
    const row = (await screen.findByText("Los webhooks del banco son idempotentes.")).closest("li")!;
    await user.click(within(row).getByRole("button", { name: "Aprobar" }));
    const dialog = screen.getByRole("dialog", { name: "¿A quién le sirve?" });
    expect(within(dialog).getByText(/Solo a mí/)).toBeInTheDocument();
    expect(within(dialog).getByText(/A todo el equipo Plataforma/)).toBeInTheDocument();
    // The button says where it goes.
    await user.click(within(dialog).getByRole("button", { name: /^Compartir con quien trabaje en .*tienda/ }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect((await api.memory("/code/tienda")).proposed.some((p) => p.note.id === proposal.id)).toBe(false);
    expect(api.teamList[0]!.notes.some((n) => n.text === "Los webhooks del banco son idempotentes." && !n.teamLayer)).toBe(true);
  });

  it("connects a project to a team from its memory card", async () => {
    const { api, user } = renderAt("#m=%2Fcode%2Ftienda");
    const home = await screen.findByRole("combobox", { name: "Memoria de este proyecto" });
    await user.click(home);
    await user.click(screen.getByRole("option", { name: /Equipo · Plataforma/ }));
    expect(await screen.findByRole("radiogroup", { name: "Capa" })).toBeInTheDocument();
    expect((await api.memoryHome("/code/tienda")).home).toEqual({ kind: "team", team: "team-1" });
  });
});
