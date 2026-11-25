import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";
import type { Account, ActivityItem, PlanState, Task } from "../../api/types";
import { pausedOn, taskHref, untilLabel } from "./AccountPlan";

vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

async function openAccounts(api = MockApi.withSampleData()) {
  history.replaceState(null, "", "#accounts");
  render(<App api={api} queryClient={new QueryClient()} />);
  const user = userEvent.setup();
  await screen.findByRole("heading", { name: /Tus cuentas/ });
  return { api, user };
}

const NO_DATA: PlanState = { usage: null, attemptedAt: 1, problem: "unavailable", supported: true };

describe("The accounts list", () => {
  afterEach(() => history.replaceState(null, "", "#"));

  it("shows one meter per plan window, grey, amber from 70 and red from 90, with Actualizar by its source", async () => {
    const { api, user } = await openAccounts();
    const personal = screen.getByRole("article", { name: "Claude personal" });
    expect(within(personal).getByRole("meter", { name: "Sesión actual (5 h): 58%" })).toHaveClass("meter--ok");
    expect(within(personal).getByRole("meter", { name: "Esta semana: 83%" })).toHaveClass("meter--warn");
    // The account's color is a dot, never its text.
    expect(personal.querySelector(".account-card__dot")).toHaveStyle({ background: "#BB9AF7" });

    const refresh = vi.spyOn(api, "refreshPlanUsage");
    const source = within(personal).getByText("según /usage de Claude Code · hace 2 min").parentElement!;
    await user.click(within(source).getByRole("button", { name: "Actualizar el plan de Claude personal" }));
    await waitFor(() => expect(refresh).toHaveBeenCalledWith("claude-personal"));
  });

  it("shows no plan figures without a session, and Sin datos with Actualizar when nothing was read", async () => {
    const api = MockApi.withSampleData();
    const overview = api.overview.bind(api);
    vi.spyOn(api, "overview").mockImplementation(async () => {
      const o = await overview();
      const work = o.accounts.find((a) => a.id === "claude-trabajo")!;
      work.usage = { ...work.usage, plan: NO_DATA };
      return o;
    });
    const original = api.refreshPlanUsage.bind(api);
    vi.spyOn(api, "refreshPlanUsage").mockImplementation(async (id) =>
      id === "claude-trabajo" ? NO_DATA : original(id),
    );
    const { user } = await openAccounts(api);

    const codex = screen.getByRole("article", { name: "ChatGPT" });
    expect(within(codex).queryByRole("meter")).not.toBeInTheDocument();
    expect(within(codex).queryByText(/Sin datos|no se pudo actualizar/)).not.toBeInTheDocument();
    expect(within(codex).getByRole("button", { name: "Iniciar sesión" })).toBeInTheDocument();

    const work = screen.getByRole("article", { name: "Claude trabajo" });
    expect(within(work).queryByRole("meter")).not.toBeInTheDocument();
    expect(within(work).getByText("Sin datos · no se pudo actualizar")).toBeInTheDocument();
    await user.click(within(work).getByRole("button", { name: "Actualizar el plan de Claude trabajo" }));
    await waitFor(() => expect(api.refreshPlanUsage).toHaveBeenCalledWith("claude-trabajo"));
  });

  it("offers a reached limit's way out: the paused task, never another account", async () => {
    const api = MockApi.withSampleData().withSampleActivity();
    const switchAccount = vi.spyOn(api, "useAccount");
    const { user } = await openAccounts(api);
    const personal = screen.getByRole("article", { name: "Claude personal" });
    const paused = await within(personal).findByRole("status");
    expect(paused).toHaveTextContent(/^1 tarea en pausa hasta (\p{L}+, )?\d{2}:\d{2} · Ver la tarea$/u);
    expect(within(screen.getByRole("article", { name: "Claude trabajo" })).queryByText(/en pausa/)).not.toBeInTheDocument();

    await user.click(within(paused).getByRole("link", { name: "Ver la tarea en pausa «Traduce los correos de confirmación»" }));
    await waitFor(() => expect(window.location.hash).toBe(`#p=${encodeURIComponent("/code/hedy")}&t=s3`));
    expect(switchAccount).not.toHaveBeenCalled();
  });

  it("keeps an API account's address and price behind its own Avanzado", async () => {
    const { api, user } = await openAccounts();
    const setSetting = vi.spyOn(api, "setSetting");
    const kimi = screen.getByRole("article", { name: "Kimi" });
    const advanced = within(kimi).getByRole("button", { name: "Avanzado" });
    expect(advanced).toHaveAttribute("aria-expanded", "false");
    expect(within(kimi).queryByRole("textbox", { name: "Kimi: dirección" })).not.toBeInTheDocument();
    await user.click(advanced);
    const url = within(kimi).getByRole("textbox", { name: "Kimi: dirección" });
    await user.type(url, "https://api.moonshot.cn/v1");
    await user.tab();
    expect(setSetting).toHaveBeenCalledWith("api_url.kimi", "https://api.moonshot.cn/v1");
    // Subscriptions have no such section.
    expect(within(screen.getByRole("article", { name: "Claude personal" })).queryByRole("button", { name: "Avanzado" })).toBeNull();
  });

  it("keeps the projects table under the accounts", async () => {
    await openAccounts();
    const list = document.querySelector(".account-list")!;
    const table = screen.getByRole("table", { name: "Proyectos y cuentas" });
    expect(list.compareDocumentPosition(table) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("sums up each project read-only, and a click on its row opens its sheet", async () => {
    const api = MockApi.withSampleData();
    await api.setProjectPhases("/code/hedy", {
      implement: { account: "chatgpt", model: null },
      tests: { account: "kimi", model: "kimi-k3" },
      review: { account: "kimi", model: null },
    });
    const { user } = await openAccounts(api);
    const table = screen.getByRole("table", { name: "Proyectos y cuentas" });
    expect(within(table).getAllByRole("columnheader").map((h) => h.textContent)).toEqual([
      "Proyecto",
      "Cuenta",
      "Protegido",
      "Cliente",
      "Quién hace cada paso",
    ]);
    const [, hedy, tienda] = within(table).getAllByRole("row");
    expect(within(hedy!).getAllByRole("cell").map((c) => c.textContent)).toEqual([
      "Claude personal",
      "No",
      "Estudio Norte",
      "Claude planifica · Codex implementa · Kimi tests · Kimi verifica",
    ]);
    expect(within(tienda!).getAllByRole("cell").map((c) => c.textContent)).toEqual(["Claude trabajo", "Sí", "Café Aurora", "Claude"]);
    // Nothing is edited here: everything is in the project's sheet.
    expect(within(table).queryByRole("combobox")).toBeNull();
    expect(within(table).queryByRole("textbox")).toBeNull();
    expect(within(table).queryByRole("switch")).toBeNull();
    expect(within(table).queryByRole("button")).toBeNull();
    expect(within(table.closest("section")!).getByRole("button", { name: "Abrir carpeta…" })).toBeEnabled();

    await user.click(within(tienda!).getByText("Café Aurora"));
    expect(await screen.findByRole("heading", { level: 1, name: "Ajustes de tienda" })).toBeInTheDocument();
    expect(window.location.hash).toBe(`#ps=${encodeURIComponent("/code/tienda")}`);
  });
});

describe("Adding an account", () => {
  afterEach(() => history.replaceState(null, "", "#"));

  it("asks how the AI is paid, then shows that kind's providers grouped", async () => {
    const { user } = await openAccounts();
    await user.click(screen.getByRole("button", { name: "+ Agregar cuenta" }));
    const dialog = screen.getByRole("dialog", { name: "Agregar cuenta" });
    expect(within(dialog).getByRole("heading", { name: "¿Cómo pagas esta IA?" })).toBeInTheDocument();
    expect(within(dialog).queryByRole("radio")).not.toBeInTheDocument();

    await user.click(within(dialog).getByRole("button", { name: /^Con tu suscripción/ }));
    const groups = () =>
      within(within(dialog).getByRole("group", { name: "Proveedor" }))
        .getAllByRole("group")
        .map((g) => g.getAttribute("aria-label"));
    expect(groups()).toEqual(["Claude", "ChatGPT", "Google", "Kimi"]);
    const google = within(dialog).getByRole("group", { name: "Google" });
    expect(within(google).getAllByRole("radio").map((r) => r.closest("label")?.querySelector(".kind__title")?.textContent)).toEqual([
      "Antigravity",
      "Gemini CLI",
    ]);
    expect(within(dialog).getByRole("radio", { name: /^Claude Code/ })).toBeChecked();
    // Each step starts at its first control: the chosen provider, then the first choice.
    expect(within(dialog).getByRole("radio", { name: /^Claude Code/ })).toHaveFocus();

    await user.click(within(dialog).getByRole("button", { name: "Atrás" }));
    expect(dialog.querySelector(".add-account__step button")).toHaveFocus();
    await user.click(within(dialog).getByRole("button", { name: /^Con una API key/ }));
    expect(groups()).toEqual(["Google", "Kimi"]);
    expect(within(dialog).getByRole("radio", { name: /^Gemini/ })).toBeChecked();
    expect(within(dialog).getByRole("radio", { name: /^Gemini/ })).toHaveFocus();
    expect(within(dialog).getByRole("button", { name: "Crear y guardar key" })).toBeDisabled();
  });

  it("offers a session already on this computer first, and adopts its folder without a new login", async () => {
    const api = new MockApi();
    const add = vi.spyOn(api, "addAccount");
    const { user } = await openAccounts(api);
    await user.click(screen.getByRole("button", { name: "+ Agregar cuenta" }));
    const dialog = screen.getByRole("dialog", { name: "Agregar cuenta" });
    const found = await within(dialog).findByRole("button", { name: /^Claude Code\s*Usar la sesión abierta en ~\/\.claude$/ });
    // Offered before the question about paying.
    const question = within(dialog).getByRole("heading", { name: "¿Cómo pagas esta IA?" });
    expect(found.compareDocumentPosition(question) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    // Codex is installed but signed out: nothing to adopt.
    expect(within(dialog).queryByRole("button", { name: /^Codex/ })).not.toBeInTheDocument();

    await user.click(found);
    expect(within(dialog).getByLabelText("Nombre")).toHaveValue("Claude Code");
    expect(within(dialog).getByRole("checkbox", { name: /Usar la sesión que ya existe en/ })).toBeChecked();
    await user.click(within(dialog).getByRole("button", { name: "Crear" }));
    await waitFor(() =>
      expect(add).toHaveBeenCalledWith(expect.objectContaining({ provider: "claude", apiKey: false, defaultFolder: true })),
    );
    expect(await screen.findByRole("article", { name: "Claude Code" })).toBeInTheDocument();
    expect(screen.queryByRole("dialog", { name: /Login/ })).not.toBeInTheDocument();
  });
});

describe("pausedOn", () => {
  const now = 1_800_000_000;
  const account = (extra: Partial<Account["usage"]> = {}): Account => ({
    id: "personal",
    name: "Claude personal",
    provider: "claude",
    providerName: "Claude Code",
    kind: "subscription",
    defaultFolder: false,
    color: "#BB9AF7",
    login: { state: "loggedIn", plan: "max" },
    projects: [],
    usage: {
      weekTokens: 0,
      weekCostUsd: null,
      limit: null,
      plan: { usage: null, attemptedAt: null, problem: null, supported: true },
      ...extra,
    },
  });
  const item = (id: string, state: ActivityItem["state"], extra: Partial<Task> = {}, updatedAt = now): ActivityItem => ({
    task: {
      id,
      project: "/code/hedy",
      account: "personal",
      title: id,
      stages: [{ phase: "implement", account: "personal", model: null }],
      stage: 0,
      ...extra,
    } as Task,
    state,
    phase: "implement",
    agent: "Claude",
    projectName: "hedy",
    updatedAt,
    queued: 0,
  });

  it("counts the limited tasks on the account, the latest first, until the wait's time", () => {
    const paused = pausedOn(account(), [
      item("a", "limited", { wait: { account: "personal", until: now + 600, since: now, ready: false } }, now - 10),
      item("b", "limited", {}, now),
      item("c", "working"),
      item("d", "limited", { account: "other", stages: [{ phase: "implement", account: "other", model: null }] }),
    ]);
    expect(paused).toEqual({ count: 2, latest: expect.objectContaining({ id: "b" }), until: now + 600 });
    expect(pausedOn(account(), [item("c", "working")])).toBeNull();
    expect(pausedOn(account(), undefined)).toBeNull();
  });

  it("falls back to the full window's reset, then to what the CLI reported", () => {
    const window = (percent: number, resetsAt: number) => ({
      kind: "session" as const,
      minutes: 300,
      model: null,
      percent,
      resetsText: null,
      resetsAt,
    });
    const full = account({
      plan: {
        usage: {
          windows: [window(100, now + 900), window(40, now + 99_000)],
          checkedAt: now,
          source: "claudeUsage",
          balance: null,
        },
        attemptedAt: now,
        problem: null,
        supported: true,
      },
    });
    expect(pausedOn(full, [item("a", "limited")])?.until).toBe(now + 900);
    const reported = account({ limit: { status: "rejected", percent: 100, resetsAt: now + 1200 } });
    expect(pausedOn(reported, [item("a", "limited")])?.until).toBe(now + 1200);
    expect(pausedOn(account(), [item("a", "limited")])?.until).toBeNull();
  });

  it("says the time alone today and the day otherwise, and links a task to its project or conversation", () => {
    const today = new Date(2026, 10, 20, 9, 0).getTime();
    expect(untilLabel(new Date(2026, 10, 20, 19, 40).getTime() / 1000, "es", today)).toBe("19:40");
    expect(untilLabel(new Date(2026, 10, 21, 9, 5).getTime() / 1000, "es", today)).toMatch(/^sábado,? 09:05$/);
    expect(taskHref([{ path: "/code/hedy" }], { id: "t 1", project: "/code/hedy" })).toBe("#p=%2Fcode%2Fhedy&t=t%201");
    expect(taskHref([], { id: "q1", project: "/tmp/scratch" })).toBe("#c=q1");
  });
});
