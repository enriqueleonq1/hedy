import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { App } from "../../App";
import { ApiProvider } from "../../api/context";
import { MockApi } from "../../api/MockApi";
import { ToastProvider } from "../../components/ui/Toast";
import { parseInvite } from "./TeamDialogs";
import { WhoForDialog } from "./TeamMemory";

vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

function renderAt(hash: string, api = MockApi.withSampleData()) {
  api.delayMs = 0;
  history.replaceState(null, "", hash);
  render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
  return { api, user: userEvent.setup() };
}

// Ajustes draws the title; the section is named after it.
const section = () => screen.getByRole("region", { name: "Equipos" });

async function openPlataforma(api = MockApi.withSampleData()) {
  const rendered = renderAt("#teams", api);
  const card = await waitFor(section);
  await rendered.user.click(await within(card).findByRole("button", { name: /Plataforma/ }));
  await within(card).findByRole("heading", { name: "Plataforma" });
  return { ...rendered, card };
}

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

  it("copies one invite with the address and the code, to paste in a message", async () => {
    const { user, card } = await openPlataforma();
    const write = vi.spyOn(navigator.clipboard, "writeText");
    await user.click(within(card).getByRole("button", { name: "Copiar invitación" }));
    expect(await screen.findByText("Invitación copiada: pégala en un mensaje.")).toBeInTheDocument();
    const text = write.mock.calls.at(-1)![0];
    expect(text).toContain("«Plataforma»");
    expect(text).toContain("Dirección: git@github.com:empresa/hedy-team.git");
    expect(text).toContain("Código: 7-tigre-lima-nube-faro");
    expect(text).toMatch(/caduca en 4[67] h/);
    // What "Unirme con una invitación" reads back.
    expect(parseInvite(text)).toEqual({ url: "git@github.com:empresa/hedy-team.git", code: "7-tigre-lima-nube-faro" });
  });

  describe("parseInvite", () => {
    const message = (team: string, url: string, code: string) =>
      `Te invito al equipo «${team}» en Hedy. En Hedy, abre Ajustes → Equipos → «Unirme con una invitación» y pega este mensaje.\n\nDirección: ${url}\nCódigo: ${code}\n\nEl código sirve una vez y caduca en 47 h.`;

    it("takes the labelled code over a team name that looks like one", () => {
      expect(parseInvite(message("Squad 2-front-end", "git@github.com:acme/team.git", "7-tigre-lima-nube"))).toEqual({
        url: "git@github.com:acme/team.git",
        code: "7-tigre-lima-nube",
      });
    });

    it("takes the labelled address over a team name that looks like one", () => {
      expect(parseInvite(message("x@y:z", "https://git.acme.dev/team.git", "3-sol-mar-luz"))).toEqual({
        url: "https://git.acme.dev/team.git",
        code: "3-sol-mar-luz",
      });
    });

    it("recognises local folders and file:// addresses", () => {
      expect(parseInvite(message("Local", "/srv/git/team.git", "3-sol-mar-luz"))?.url).toBe("/srv/git/team.git");
      expect(parseInvite(message("Local", "file:///srv/git/team.git", "3-sol-mar-luz"))?.url).toBe("file:///srv/git/team.git");
      expect(parseInvite("~/repos/team.git 3-sol-mar-luz")).toEqual({ url: "~/repos/team.git", code: "3-sol-mar-luz" });
      expect(parseInvite("file:///srv/team.git, 3-sol-mar-luz.")).toEqual({ url: "file:///srv/team.git", code: "3-sol-mar-luz" });
    });

    it("reads the English message and labels on one line", () => {
      const english =
        'You\'re invited to the team "Squad 2-front-end" in Hedy.\n\nAddress: ssh://git@acme.dev/team.git\nCode: 5-red-blue-green\n\nThe code works once.';
      expect(parseInvite(english)).toEqual({ url: "ssh://git@acme.dev/team.git", code: "5-red-blue-green" });
      expect(parseInvite("Dirección: /srv/team.git Código: 5-uno-dos")).toEqual({ url: "/srv/team.git", code: "5-uno-dos" });
    });

    it("needs both the address and the code", () => {
      expect(parseInvite("Código: 5-uno-dos-tres")).toBeNull();
      expect(parseInvite("git@github.com:acme/team.git")).toBeNull();
    });
  });

  it("adds another computer of the user's with its own invite", async () => {
    const { api, user, card } = await openPlataforma();
    const invite = vi.spyOn(api, "teamInvite");
    await user.click(within(card).getByRole("button", { name: "Agregar otra computadora tuya" }));
    await waitFor(() => expect(invite).toHaveBeenCalledWith("team-1", false, true));
    expect(await within(card).findByText("Otra computadora tuya")).toBeInTheDocument();
    const write = vi.spyOn(navigator.clipboard, "writeText");
    const row = within(card).getByText("Otra computadora tuya").closest("li")!;
    await user.click(within(row).getByRole("button", { name: "Copiar invitación" }));
    await waitFor(() => expect(write.mock.calls.at(-1)![0]).toMatch(/^Para agregar otra computadora tuya al equipo «Plataforma»/));
  });

  it("shows the team layer's usage as a meter", async () => {
    const { api, card } = await openPlataforma();
    const team = await api.teamStatus("team-1");
    const meter = within(card).getByRole("meter", { name: "Uso de la capa del equipo" });
    expect(meter).toHaveAttribute("aria-valuenow", String(Math.round((team.layerTokens / team.layerMax) * 100)));
  });

  it("leaves from the danger zone after a second click", async () => {
    const api = MockApi.withSampleData();
    // The only admin left would be Marta: Alex steps down to member first.
    api.teamList[0]!.people.find((p) => p.me)!.role = "member";
    api.teamList[0]!.summary.role = "member";
    const { user, card } = await openPlataforma(api);
    const zone = within(card).getByRole("region", { name: "Zona de peligro" });
    const leave = within(zone).getByRole("button", { name: "Salir" });
    expect(leave).toHaveClass("btn--danger");
    await user.click(leave);
    expect(api.teamList).toHaveLength(1);
    await user.click(within(zone).getByRole("button", { name: /¿Salir\? Sus notas se borran/ }));
    await waitFor(() => expect(api.teamList).toHaveLength(0));
    expect(await within(section()).findByRole("button", { name: /Crear un equipo/ })).toBeInTheDocument();
  });

  it("lets an admin rename the team", async () => {
    const { api, user, card } = await openPlataforma();
    await user.click(within(card).getByRole("button", { name: "Renombrar" }));
    const input = within(card).getByLabelText("Nombre del equipo");
    await user.clear(input);
    await user.type(input, "Plataforma web{Enter}");
    expect(await within(card).findByRole("heading", { name: "Plataforma web" })).toBeInTheDocument();
    expect(api.teamList[0]!.summary.name).toBe("Plataforma web");
  });

  it("says in plain words why a team's sync failed, with Reintentar", async () => {
    const api = MockApi.withSampleData();
    api.teamList[0]!.summary.lastError = "git fetch falló: Authentication failed for 'https://github.com/empresa/hedy-team.git'";
    const sync = vi.spyOn(api, "teamSync");
    const { user, card } = await openPlataforma(api);
    const alert = within(card).getByRole("alert");
    expect(within(alert).getByText("Sin acceso al repositorio")).toBeInTheDocument();
    await user.click(within(alert).getByRole("button", { name: "Reintentar" }));
    expect(sync).toHaveBeenCalledWith("team-1");
  });

  it("says when a team cannot be read, with Reintentar and the way back", async () => {
    const api = MockApi.withSampleData();
    const read = vi.spyOn(api, "teamStatus").mockRejectedValue("disk on fire");
    const { user } = renderAt("#teams", api);
    const card = await waitFor(section);
    await user.click(await within(card).findByRole("button", { name: /Plataforma/ }));
    const alert = await within(card).findByRole("alert");
    expect(alert).toHaveTextContent("No se pudo leer el equipo");
    expect(alert).toHaveTextContent("disk on fire");
    read.mockRestore();
    await user.click(within(alert).getByRole("button", { name: "Reintentar" }));
    expect(await within(card).findByRole("heading", { name: "Plataforma" })).toBeInTheDocument();
  });

  it("reads the address and the code out of a pasted invite", async () => {
    const api = MockApi.withSampleData();
    api.teamList = [];
    const { user } = renderAt("#teams", api);
    await user.click(within(await waitFor(section)).getByRole("button", { name: /Unirme con una invitación/ }));
    const dialog = screen.getByRole("dialog", { name: "Unirme con una invitación" });
    await user.click(within(dialog).getByLabelText("Código de invitación"));
    await user.paste("Te invito.\n\nDirección: git@github.com:empresa/hedy-team.git\nCódigo: 7-tigre-lima-nube-faro\n\nCaduca.");
    expect(within(dialog).getByLabelText("Dirección del repo")).toHaveValue("git@github.com:empresa/hedy-team.git");
    expect(within(dialog).getByLabelText("Código de invitación")).toHaveValue("7-tigre-lima-nube-faro");
    expect(parseInvite("solo 7-tigre-lima-nube-faro")).toBeNull();
    expect(parseInvite("https://gitlab.com/g/7-a-b-c.git y 12-ñandú-río-sol.")).toEqual({
      url: "https://gitlab.com/g/7-a-b-c.git",
      code: "12-ñandú-río-sol",
    });
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

  it("reviews suggestions one at a time, from Inicio's row", async () => {
    const { api, user } = renderAt("#");
    const row = (await screen.findByText("2 sugerencias del equipo Plataforma")).closest("li")!;
    await user.click(within(row).getByRole("button", { name: "Revisar" }));
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

  it("approves an agent's proposal in a team's project only for the user, without asking", async () => {
    const api = MockApi.withSampleData();
    await api.teamLinkProject("team-1", "/code/tienda");
    const proposal = api.addProposal("/code/tienda", "Los webhooks del banco son idempotentes.");
    const publish = vi.spyOn(api, "teamPublishNote");
    const { user } = renderAt(`#m=${encodeURIComponent("/code/tienda")}&pending`, api);
    const row = (await screen.findByText("Los webhooks del banco son idempotentes.")).closest("li")!;
    // A split button: the plain approve, and a menu to share.
    const approve = await within(row).findByRole("button", { name: "Aprobar · solo para mí" });
    expect(within(row).getByRole("button", { name: "Aprobar y compartir" })).toHaveAttribute("aria-haspopup", "menu");
    // Solid, like any other card.
    expect(row).toHaveClass("note--proposed");
    await user.click(approve);
    expect(screen.queryByRole("dialog")).toBeNull();
    await waitFor(async () => expect((await api.memory("/code/tienda")).notes.some((n) => n.id === proposal.id)).toBe(true));
    expect(publish).not.toHaveBeenCalled();
    await waitFor(() => expect(screen.queryByText("Los webhooks del banco son idempotentes.")).toBeNull());
  });

  it("shares a proposal with the project from the approve menu, that option chosen", async () => {
    const api = MockApi.withSampleData();
    await api.teamLinkProject("team-1", "/code/tienda");
    const proposal = api.addProposal("/code/tienda", "Los webhooks del banco son idempotentes.");
    const { user } = renderAt(`#m=${encodeURIComponent("/code/tienda")}&pending`, api);
    const row = (await screen.findByText("Los webhooks del banco son idempotentes.")).closest("li")!;
    await user.click(await within(row).findByRole("button", { name: "Aprobar y compartir" }));
    const menu = screen.getByRole("menu", { name: "Aprobar y compartir" });
    expect(within(menu).getByRole("menuitem", { name: "A todo el equipo…" })).toBeInTheDocument();
    await user.click(within(menu).getByRole("menuitem", { name: /^A quien trabaje en .*tienda$/ }));
    const dialog = screen.getByRole("dialog", { name: "¿A quién le sirve?" });
    expect(within(dialog).getByRole("radio", { name: /^A quien trabaje en .*tienda/ })).toBeChecked();
    expect(within(dialog).getByRole("radio", { name: /^Solo a mí/ })).not.toBeChecked();
    // The button says where it goes.
    await user.click(within(dialog).getByRole("button", { name: /^Compartir con quien trabaje en .*tienda/ }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect((await api.memory("/code/tienda")).proposed.some((p) => p.note.id === proposal.id)).toBe(false);
    expect(api.teamList[0]!.notes.some((n) => n.text === "Los webhooks del banco son idempotentes." && !n.teamLayer)).toBe(true);
  });

  it("opens the dialog on the whole team from the approve menu", async () => {
    const api = MockApi.withSampleData();
    await api.teamLinkProject("team-1", "/code/tienda");
    api.addProposal("/code/tienda", "Los webhooks del banco son idempotentes.");
    const { user } = renderAt(`#m=${encodeURIComponent("/code/tienda")}&pending`, api);
    const row = (await screen.findByText("Los webhooks del banco son idempotentes.")).closest("li")!;
    await user.click(await within(row).findByRole("button", { name: "Aprobar y compartir" }));
    await user.click(screen.getByRole("menuitem", { name: "A todo el equipo…" }));
    const dialog = screen.getByRole("dialog", { name: "¿A quién le sirve?" });
    expect(await within(dialog).findByRole("radio", { name: /^A todo el equipo Plataforma/ })).toBeChecked();
    expect(within(dialog).getByRole("button", { name: "Enviar a todo el equipo" })).toBeInTheDocument();
  });

  it("asks who it is for with only the user chosen, unless they asked to share", async () => {
    const api = MockApi.withSampleData();
    const proposal = api.addProposal("/code/tienda", "Los webhooks del banco son idempotentes.");
    const approve = vi.spyOn(api, "approveNote");
    const publish = vi.spyOn(api, "teamPublishNote");
    const closed = vi.fn();
    render(
      <QueryClientProvider client={new QueryClient()}>
        <ApiProvider api={api}>
          <ToastProvider>
            <WhoForDialog note={proposal} project="/code/tienda" projectName="tienda" team="team-1" onClose={closed} />
          </ToastProvider>
        </ApiProvider>
      </QueryClientProvider>,
    );
    const dialog = screen.getByRole("dialog", { name: "¿A quién le sirve?" });
    expect(within(dialog).getByRole("radio", { name: /^Solo a mí/ })).toBeChecked();
    expect(within(dialog).getByRole("radio", { name: /^A quien trabaje en tienda/ })).not.toBeChecked();
    await userEvent.setup().click(within(dialog).getByRole("button", { name: "Aprobar solo para mí" }));
    await waitFor(() => expect(closed).toHaveBeenCalled());
    expect(approve).toHaveBeenCalledWith(proposal.id);
    expect(publish).not.toHaveBeenCalled();
  });

  it("connects a project to a team from its memory card", async () => {
    const { api, user } = renderAt("#m=%2Fcode%2Ftienda");
    const home = await screen.findByRole("combobox", { name: "Memoria de este proyecto" });
    await user.click(home);
    // Not "Para"'s option of the same name: the card's own list.
    await user.click(within(screen.getByRole("listbox")).getByRole("option", { name: /Equipo · Plataforma/ }));
    expect(await screen.findByRole("radiogroup", { name: "Capa" })).toBeInTheDocument();
    expect((await api.memoryHome("/code/tienda")).home).toEqual({ kind: "team", team: "team-1" });
  });
});
