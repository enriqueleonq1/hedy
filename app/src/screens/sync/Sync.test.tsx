import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";
import type { MemoryHome, SyncConflict } from "../../api/types";
import { repoLabel } from "../../sync/hooks";
import { foldWord } from "./SyncDialogs";
import { syncErrorKind } from "../../sync/errors";
import { conflictChoices, summarize } from "./SyncSection";

vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

function renderAt(hash: string, api: MockApi) {
  api.delayMs = 0;
  history.replaceState(null, "", hash);
  render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
  return { api, user: userEvent.setup() };
}

/** Sample data without the sync set up. */
function offline(): MockApi {
  const api = MockApi.withSampleData();
  api.sync = null;
  api.syncConflictList = [];
  return api;
}

// Ajustes draws the title; the section is named after it.
const section = () => screen.getByRole("region", { name: "Sincronización" });

describe("Sync", () => {
  afterEach(() => history.replaceState(null, "", "#"));

  it("sets up with a private repo and asks for a word of the key back", async () => {
    const { api, user } = renderAt("#sync", offline());
    const card = await waitFor(section);
    // "Off" only once the state is read.
    expect(await within(card).findByText("Desactivada")).toBeInTheDocument();
    await user.click(within(card).getByRole("button", { name: "Configurar" }));

    const dialog = screen.getByRole("dialog", { name: "Configurar la sincronización" });
    const address = within(dialog).getByLabelText("Dirección");
    // A token in the address is refused, with the way out.
    await user.type(address, "https://ghp_TOKEN@github.com/alex/hedy-memory.git");
    await user.click(within(dialog).getByRole("button", { name: "Continuar" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("gestor de credenciales");
    // A public repo is warned about, and can still be used.
    await user.clear(address);
    await user.type(address, "https://github.com/alex/public-memory.git");
    await user.click(within(dialog).getByRole("button", { name: "Continuar" }));
    expect(await within(dialog).findByText(/Este repo es público/)).toBeInTheDocument();
    await user.clear(address);
    await user.type(address, "git@github.com:alex/hedy-memory.git");
    await user.click(within(dialog).getByRole("button", { name: "Continuar" }));

    expect(await within(dialog).findByText("Paso 2 de 3")).toBeInTheDocument();
    expect(within(dialog).getByLabelText("Nombre")).toHaveValue("Casa");
    await user.click(within(dialog).getByRole("button", { name: "Configurar" }));

    expect(await within(dialog).findByLabelText("Clave de recuperación")).toHaveTextContent(MockApi.SYNC_WORDS);
    const finish = within(dialog).getByRole("button", { name: "Terminar" });
    expect(finish).toBeDisabled();
    // Neither Escape, a project shortcut nor the palette loses the words.
    expect(within(dialog).queryByRole("button", { name: "Cerrar" })).toBeNull();
    await user.keyboard("{Escape}");
    await user.keyboard("{Alt>}1{/Alt}");
    await user.keyboard("{Control>}k{/Control}");
    expect(screen.getByRole("dialog", { name: "Configurar la sincronización" })).toBeInTheDocument();
    expect(screen.queryByRole("dialog", { name: /Buscar|Paleta/ })).toBeNull();
    expect(window.location.hash).toBe("#sync");
    const check = within(dialog).getByLabelText("Para confirmar, escribe la palabra n.º 7");
    await user.type(check, "pasto");
    expect(within(dialog).getByText(/No coincide/)).toBeInTheDocument();
    await user.clear(check);
    await user.type(check, "COBRE");
    await user.click(finish);

    expect(screen.queryByRole("dialog")).toBeNull();
    expect(await within(section()).findByText("Todo al día")).toBeInTheDocument();
    expect(api.sync?.devices.map((d) => d.name)).toEqual(["Casa"]);
  });

  it("keeps the words while the repo answers, and while a notification asks for another screen", async () => {
    const api = offline().withSampleActivity();
    let release = () => {};
    api.syncGate = new Promise((resolve) => (release = resolve));
    const { user } = renderAt("#sync", api);
    await user.click(within(await waitFor(section)).getByRole("button", { name: "Configurar" }));
    const dialog = screen.getByRole("dialog", { name: "Configurar la sincronización" });
    await user.type(within(dialog).getByLabelText("Dirección"), "git@github.com:alex/hedy-memory.git");
    await user.click(within(dialog).getByRole("button", { name: "Continuar" }));
    await user.click(await within(dialog).findByRole("button", { name: "Configurar" }));
    // The repo takes the first epoch: closing now would lose the words.
    expect(within(dialog).getByRole("button", { name: "Configurando…" })).toBeDisabled();
    expect(within(dialog).queryByRole("button", { name: "Cerrar" })).toBeNull();
    await user.keyboard("{Escape}");
    release();
    expect(await within(dialog).findByLabelText("Clave de recuperación")).toHaveTextContent(MockApi.SYNC_WORDS);

    api.simulateNotificationClick("s3");
    expect(window.location.hash).toBe("#sync");
    expect(screen.getByRole("dialog", { name: "Configurar la sincronización" })).toBeInTheDocument();
    // Nor closing the window, nor a failed refresh of the app's data.
    expect(await api.requestClose()).toBe(false);
    expect(await screen.findByText(/Antes de cerrar, guarda tu clave/)).toBeInTheDocument();
    api.overview = () => Promise.reject(new Error("sin base de datos"));
    await act(() => api.emitSyncUpdated());
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(screen.getByRole("dialog", { name: "Configurar la sincronización" })).toBeInTheDocument();
  });

  it("says a project without a remote keeps its memory here", async () => {
    const api = MockApi.withSampleData();
    api.repoOwners.delete("/code/hedy");
    renderAt(`#m=${encodeURIComponent("/code/hedy")}`, api);
    expect(await screen.findByText(/no tiene un repo remoto/)).toBeInTheDocument();
    expect(screen.queryByRole("combobox", { name: "Memoria de este proyecto" })).toBeNull();
  });

  it("adds this computadora with the same address and the 12 words", async () => {
    const { api, user } = renderAt("#sync", offline());
    await user.click(within(await waitFor(section)).getByRole("button", { name: "Ya la uso en otra computadora" }));
    const dialog = screen.getByRole("dialog", { name: "Agregar esta computadora" });
    await user.type(within(dialog).getByLabelText("Dirección"), "git@github.com:alex/hedy-memory.git");
    await user.click(within(dialog).getByRole("button", { name: "Continuar" }));
    await user.clear(within(dialog).getByLabelText("Nombre"));
    await user.type(within(dialog).getByLabelText("Nombre"), "Oficina");
    await user.click(within(dialog).getByRole("button", { name: "Continuar" }));
    const words = within(dialog).getByLabelText("Clave de recuperación");
    await user.type(words, "árbol nube lápiz");
    await user.click(within(dialog).getByRole("button", { name: "Agregar esta computadora" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("12 palabras");

    await user.clear(words);
    await user.type(words, MockApi.SYNC_WORDS);
    await user.click(within(dialog).getByRole("button", { name: "Agregar esta computadora" }));
    expect(await screen.findByText("Esta computadora ya sincroniza · 8 recibidos")).toBeInTheDocument();
    const table = within(section()).getByRole("table");
    expect(within(table).getByText("Oficina")).toBeInTheDocument();
    expect(within(table).getByText("Casa")).toBeInTheDocument();
    expect(api.sync?.devices.find((d) => d.thisPc)?.name).toBe("Oficina");
  });

  it("shows both versions of a conflict, the other computer's as the primary choice", async () => {
    const { api, user } = renderAt("#sync", MockApi.withSampleData());
    const card = await waitFor(section);
    expect(await within(card).findByText("1 cambio para resolver")).toBeInTheDocument();
    expect(within(card).getByRole("heading", { name: "Para resolver" })).toBeInTheDocument();
    const conflict = within(card).getByRole("article", { name: "Nota" });
    expect(within(conflict).getByText(/con dos decimales/)).toBeInTheDocument();
    expect(within(conflict).getByText(/solo si no son cero/)).toBeInTheDocument();
    expect(within(conflict).getByText(/Oficina · hace 1 h · en uso/)).toBeInTheDocument();
    expect(within(conflict).getByRole("button", { name: "Usar la de Oficina" })).toHaveClass("btn--primary");
    const mine = within(conflict).getByRole("button", { name: "Mantener la mía" });
    expect(mine).toHaveClass("btn--secondary");
    // This computer's version is the one not in use here: keeping it takes the other side.
    await user.click(mine);
    expect(await within(card).findByText("Todo al día")).toBeInTheDocument();
    expect(api.resolved).toEqual([{ id: 1, useOther: true }]);
    expect(within(card).queryByRole("article")).toBeNull();
  });

  it("keeps the other computer's version from its primary button", async () => {
    const { api, user } = renderAt("#sync", MockApi.withSampleData());
    const conflict = await within(await waitFor(section)).findByRole("article", { name: "Nota" });
    await user.click(within(conflict).getByRole("button", { name: "Usar la de Oficina" }));
    await waitFor(() => expect(api.resolved).toEqual([{ id: 1, useOther: false }]));
  });

  it("says when its state cannot be read, with Reintentar, instead of looking off", async () => {
    const api = offline();
    const read = vi.spyOn(api, "syncStatus").mockRejectedValue("disk on fire");
    const { user } = renderAt("#sync", api);
    const card = await waitFor(section);
    expect(await within(card).findByRole("alert")).toHaveTextContent("No se pudo leer el estado de la sincronización");
    expect(within(card).queryByText("Desactivada")).toBeNull();
    expect(within(card).queryByRole("button", { name: "Configurar" })).toBeNull();
    read.mockRestore();
    await user.click(within(card).getByRole("button", { name: "Reintentar" }));
    expect(await within(card).findByText("Desactivada")).toBeInTheDocument();
  });

  it("lists what syncs and what stays, read-only", async () => {
    renderAt("#sync", MockApi.withSampleData());
    const card = await waitFor(section);
    expect(await within(card).findByText("1 proyecto personal")).toBeInTheDocument();
    expect(within(card).getByRole("heading", { name: "Qué se sincroniza" })).toBeInTheDocument();
    for (const item of ["Memoria", "Habilidades", "Plantillas", "Temas"]) {
      expect(within(card).getByText(item)).toBeInTheDocument();
    }
    expect(within(card).getAllByText("Se sincroniza")).toHaveLength(4);
    expect(within(card).getByText(/1 espera tu respuesta/)).toBeInTheDocument();
    expect(within(card).getByText("Sesiones y claves")).toBeInTheDocument();
    // Nothing to switch here: a project's memory is chosen in its Memoria.
    expect(within(card).queryByRole("switch")).toBeNull();
    expect(within(card).getByText(/se elige en su Memoria/)).toBeInTheDocument();
  });

  it("opens from Inicio's row when a change waits, and says when a sync fails", async () => {
    const { api, user } = renderAt("#", MockApi.withSampleData());
    const row = (await screen.findByText("Un cambio en tus computadoras para resolver")).closest("li")!;
    await user.click(within(row).getByRole("button", { name: /^Un cambio en tus computadoras para resolver/ }));
    expect(window.location.hash).toBe("#sync");
    const card = await waitFor(section);
    expect(await within(card).findByText("1 cambio para resolver")).toBeInTheDocument();
    expect(within(within(card).getByRole("table")).getByText("Oficina")).toBeInTheDocument();

    api.syncFailure = "git push falló: fatal: unable to access 'https://github.com/alex/m/': Could not resolve host: github.com";
    await user.click(within(section()).getByRole("button", { name: "Sincronizar ahora" }));
    // In plain words, with git's message behind "Detalles" and a way to try again.
    const alert = await within(section()).findByRole("alert");
    // Its title once, in the status line; the box below holds the sentence.
    expect(within(section()).getByRole("status")).toHaveTextContent("Sin conexión");
    expect(within(alert).queryByText("Sin conexión")).toBeNull();
    expect(within(section()).getAllByText("Sin conexión")).toHaveLength(1);
    expect(within(section()).queryByRole("button", { name: "Sincronizar ahora" })).toBeNull();
    const detail = within(alert).getByText(/Could not resolve host/);
    expect(detail).not.toBeVisible();
    await user.click(within(alert).getByRole("button", { name: "Detalles" }));
    expect(detail).toBeVisible();

    api.syncFailure = null;
    await user.click(within(alert).getByRole("button", { name: "Reintentar" }));
    expect(await screen.findByText("Sincronizado · 0 enviados, 0 recibidos")).toBeInTheDocument();
    await waitFor(() => expect(within(section()).queryByRole("alert")).toBeNull());
    expect(within(section()).getByRole("button", { name: "Sincronizar ahora" })).toBeInTheDocument();
  });

  it("says when git cannot get into the repo, and anything else generically", async () => {
    const api = MockApi.withSampleData();
    api.sync!.lastError = "git fetch falló: git@github.com: Permission denied (publickey).";
    renderAt("#sync", api);
    const alert = await within(await waitFor(section)).findByRole("alert");
    expect(within(section()).getByRole("status")).toHaveTextContent("Sin acceso al repositorio");
    expect(within(alert).getByText(/git ls-remote/)).toBeInTheDocument();
  });

  it("tells failures apart as the core does", () => {
    expect(syncErrorKind("git push falló: sin conexión")).toBe("offline");
    expect(syncErrorKind("ssh: connect to host github.com port 22: Network is unreachable")).toBe("offline");
    // The network first: ssh says it could not read the repo too.
    expect(syncErrorKind("ssh: Could not resolve hostname x\nfatal: Could not read from remote repository.")).toBe("offline");
    expect(syncErrorKind("remote: Repository not found.")).toBe("access");
    expect(syncErrorKind("The requested URL returned error: 403")).toBe("access");
    expect(syncErrorKind("otra computadora acaba de cambiar las claves")).toBe("other");
  });

  it("makes the other computer's version the primary choice, whichever side is in use", () => {
    const side = (device: string, at: number) => ({ text: device, device, at });
    const conflict = (kept: string, other: string): SyncConflict => ({
      id: 1,
      kind: "note",
      kept: side(kept, 2),
      other: side(other, 1),
    });
    const here = (device: string) => device === "casa";
    // Mine in use: using theirs takes the other side.
    let choice = conflictChoices(conflict("casa", "oficina"), here);
    expect(choice.primary).toMatchObject({ useOther: true, mine: false, side: { device: "oficina" } });
    expect(choice.secondary).toMatchObject({ useOther: false, mine: true });
    // Theirs in use: keeping mine takes the other side.
    choice = conflictChoices(conflict("oficina", "casa"), here);
    expect(choice.primary).toMatchObject({ useOther: false, side: { device: "oficina" } });
    expect(choice.secondary).toMatchObject({ useOther: true, mine: true });
    // Neither is mine: the one not in use is offered, the one in use kept.
    choice = conflictChoices(conflict("oficina", "portatil"), here);
    expect(choice.primary).toMatchObject({ useOther: true, side: { device: "portatil" } });
    expect(choice.secondary).toMatchObject({ useOther: false, mine: false });
  });

  it("asks once about a project's owner", async () => {
    const { api, user } = renderAt(`#s=${encodeURIComponent("/code/tienda")}`, MockApi.withSampleData());
    expect(await screen.findByText("¿La memoria de cafe-aurora es solo tuya?")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Compartida" }));
    await waitFor(() => expect(screen.queryByText(/¿La memoria de cafe-aurora/)).toBeNull());
    expect((await api.memoryHome("/code/tienda")).home).toEqual({ kind: "shared", group: "github.com/cafe-aurora" });
  });

  it("changes where a project's memory lives from its memory screen", async () => {
    const { api, user } = renderAt(`#m=${encodeURIComponent("/code/hedy")}`, MockApi.withSampleData());
    const home = await screen.findByRole("combobox", { name: "Memoria de este proyecto" });
    expect(home).toHaveTextContent("Personal");
    await user.click(home);
    await user.click(screen.getByRole("option", { name: /Solo en esta computadora/ }));
    await waitFor(async () => expect((await api.memoryHome("/code/hedy")).home).toEqual({ kind: "local" }));
    expect((await api.memoryHome("/code/hedy")).chosen).toBe(true);
  });

  it("removes another computadora after asking", async () => {
    const { api, user } = renderAt("#sync", MockApi.withSampleData());
    const table = await waitFor(() => within(section()).getByRole("table"));
    // This computadora has no "Quitar": it leaves instead.
    expect(within(table).queryByRole("button", { name: "Quitar Casa" })).toBeNull();
    await user.click(within(table).getByRole("button", { name: "Quitar Oficina" }));
    const dialog = screen.getByRole("dialog", { name: "¿Quitar Oficina?" });
    expect(within(dialog).getByText(/no podrá leer lo que guardes desde ahora/)).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Cancelar" }));
    expect(api.sync?.devices).toHaveLength(2);

    await user.click(within(table).getByRole("button", { name: "Quitar Oficina" }));
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Quitar computadora" }));
    expect(await screen.findByText("Oficina ya no puede leer lo nuevo.")).toBeInTheDocument();
    expect(api.sync?.devices.map((d) => d.name)).toEqual(["Casa"]);
    await waitFor(() => expect(within(section()).queryByText("Oficina")).toBeNull());
  });

  it("stops syncing on this computadora after a second click", async () => {
    const { api, user } = renderAt("#sync", MockApi.withSampleData());
    const card = await waitFor(section);
    await user.click(await within(card).findByRole("button", { name: "Dejar de sincronizar en esta computadora" }));
    await user.click(within(card).getByRole("button", { name: "Sí, dejar de sincronizar" }));
    expect(await within(section()).findByText("Desactivada")).toBeInTheDocument();
    expect(api.sync).toBeNull();
  });

  it("compares recovery words as the core does, and names repos and projects briefly", () => {
    expect(foldWord(" Ábaco ")).toBe("abaco");
    expect(foldWord("PINGÜINO")).toBe("pinguino");
    expect(foldWord("Ñandú")).toBe("ñandu");
    // As the words come: accents as separate marks.
    expect(foldWord("pito\u0301n")).toBe(foldWord("pitón"));
    expect(foldWord("an\u0303o")).toBe("año");
    expect(repoLabel("git@github.com:alex/hedy-memory.git")).toBe("alex/hedy-memory");
    expect(repoLabel("https://gitlab.com/grupo/sub/memoria.git")).toBe("grupo/sub/memoria");
    expect(repoLabel("ssh://git@host:2222/u/m.git")).toBe("u/m");
    const home = (path: string, extra: Partial<MemoryHome>): MemoryHome => ({
      path,
      home: null,
      chosen: false,
      ask: null,
      group: null,
      syncs: false,
      ...extra,
    });
    expect(
      summarize([
        home("/a", { home: { kind: "personal" }, syncs: true }),
        // Personal, but without an origin: it does not sync.
        home("/f", { home: { kind: "personal" } }),
        home("/b", { home: { kind: "shared", group: "github.com/empresa-org" } }),
        home("/c", { home: { kind: "shared", group: "gitlab.com/empresa-org" } }),
        home("/d", { ask: "github.com/cliente" }),
        home("/e", { home: { kind: "local" } }),
      ]),
    ).toEqual({ personal: 1, stay: 5, groups: ["empresa-org"], waiting: 1 });
  });
});
