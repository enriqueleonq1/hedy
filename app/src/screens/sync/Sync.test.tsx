import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";
import type { MemoryHome } from "../../api/types";
import { repoLabel } from "../../sync/hooks";
import { foldWord } from "./SyncDialogs";
import { summarize } from "./SyncSection";

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

const section = () => screen.getByRole("heading", { name: "Sincronización" }).closest("section")!;
const bar = () => screen.getByRole("contentinfo", { name: "Barra de estado" });

describe("Sync", () => {
  afterEach(() => history.replaceState(null, "", "#"));

  it("sets up with a private repo and asks for a word of the key back", async () => {
    const { api, user } = renderAt("#sync", offline());
    const card = await waitFor(section);
    expect(within(card).getByText("Desactivada")).toBeInTheDocument();
    expect(within(bar()).queryByRole("button", { name: /Sincronización:/ })).toBeNull();
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
    expect(within(bar()).getByRole("button", { name: "Sincronización: Sincronizado. Ver detalles" })).toBeInTheDocument();
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

  it("adds this PC with the same address and the 12 words", async () => {
    const { api, user } = renderAt("#sync", offline());
    await user.click(within(await waitFor(section)).getByRole("button", { name: "Ya la uso en otra PC" }));
    const dialog = screen.getByRole("dialog", { name: "Sumar esta PC" });
    await user.type(within(dialog).getByLabelText("Dirección"), "git@github.com:alex/hedy-memory.git");
    await user.click(within(dialog).getByRole("button", { name: "Continuar" }));
    await user.clear(within(dialog).getByLabelText("Nombre"));
    await user.type(within(dialog).getByLabelText("Nombre"), "Oficina");
    await user.click(within(dialog).getByRole("button", { name: "Continuar" }));
    const words = within(dialog).getByLabelText("Clave de recuperación");
    await user.type(words, "árbol nube lápiz");
    await user.click(within(dialog).getByRole("button", { name: "Sumar esta PC" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("12 palabras");

    await user.clear(words);
    await user.type(words, MockApi.SYNC_WORDS);
    await user.click(within(dialog).getByRole("button", { name: "Sumar esta PC" }));
    expect(await screen.findByText("Esta PC ya sincroniza · 8 recibidos")).toBeInTheDocument();
    const table = within(section()).getByRole("table");
    expect(within(table).getByText("Oficina")).toBeInTheDocument();
    expect(within(table).getByText("Casa")).toBeInTheDocument();
    expect(api.sync?.devices.find((d) => d.thisPc)?.name).toBe("Oficina");
  });

  it("shows both versions of a conflict and keeps the one chosen", async () => {
    const { api, user } = renderAt("#sync", MockApi.withSampleData());
    const card = await waitFor(section);
    expect(await within(card).findByText("1 cambio para revisar")).toBeInTheDocument();
    const conflict = within(card).getByRole("article", { name: "Nota" });
    expect(within(conflict).getByText(/con dos decimales/)).toBeInTheDocument();
    expect(within(conflict).getByText(/solo si no son cero/)).toBeInTheDocument();
    expect(within(conflict).getByText(/Oficina · hace 1 h · en uso/)).toBeInTheDocument();
    await user.click(within(conflict).getByRole("button", { name: "Usar la de esta PC" }));
    expect(await within(card).findByText("Todo al día")).toBeInTheDocument();
    expect(api.resolved).toEqual([{ id: 1, useOther: true }]);
    expect(within(card).queryByRole("article")).toBeNull();
    // What syncs and what stays.
    expect(within(card).getByText("1 proyecto personal")).toBeInTheDocument();
    expect(within(card).getByText(/1 espera tu respuesta/)).toBeInTheDocument();
  });

  it("opens the sync panel from the status bar and says when a sync fails", async () => {
    const { api, user } = renderAt("#settings", MockApi.withSampleData());
    await waitFor(section);
    const item = await within(bar()).findByRole("button", { name: /Sincronización: 1 para revisar/ });
    await user.click(item);
    const panel = screen.getByRole("dialog", { name: "Sincronización" });
    expect(within(panel).getByText("Oficina")).toBeInTheDocument();
    expect(within(panel).getByText(/esta/)).toBeInTheDocument();

    api.syncFailure = "git push falló: sin conexión";
    await user.click(within(panel).getByRole("button", { name: "Sincronizar ahora" }));
    expect(await within(bar()).findByRole("button", { name: /Sincronización: Sin sincronizar/ })).toBeInTheDocument();
    expect(within(section()).getAllByText("git push falló: sin conexión").length).toBeGreaterThan(0);

    api.syncFailure = null;
    await user.click(within(section()).getByRole("button", { name: "Sincronizar ahora" }));
    expect(await screen.findByText("Sincronizado · 0 enviados, 0 recibidos")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "Sincronización" })).toBeNull();
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
    await user.click(screen.getByRole("option", { name: /Solo en esta PC/ }));
    await waitFor(async () => expect((await api.memoryHome("/code/hedy")).home).toEqual({ kind: "local" }));
    expect((await api.memoryHome("/code/hedy")).chosen).toBe(true);
  });

  it("removes another PC after asking", async () => {
    const { api, user } = renderAt("#sync", MockApi.withSampleData());
    const table = await waitFor(() => within(section()).getByRole("table"));
    // This PC has no "Quitar": it leaves instead.
    expect(within(table).queryByRole("button", { name: "Quitar Casa" })).toBeNull();
    await user.click(within(table).getByRole("button", { name: "Quitar Oficina" }));
    const dialog = screen.getByRole("dialog", { name: "¿Quitar Oficina?" });
    expect(within(dialog).getByText(/no podrá leer lo que guardes desde ahora/)).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Cancelar" }));
    expect(api.sync?.devices).toHaveLength(2);

    await user.click(within(table).getByRole("button", { name: "Quitar Oficina" }));
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Quitar PC" }));
    expect(await screen.findByText("Oficina ya no puede leer lo nuevo.")).toBeInTheDocument();
    expect(api.sync?.devices.map((d) => d.name)).toEqual(["Casa"]);
    await waitFor(() => expect(within(section()).queryByText("Oficina")).toBeNull());
  });

  it("stops syncing on this PC after a second click", async () => {
    const { api, user } = renderAt("#sync", MockApi.withSampleData());
    const card = await waitFor(section);
    await user.click(await within(card).findByRole("button", { name: "Dejar de sincronizar en esta PC" }));
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
