import type { HedyApi } from "./HedyApi";
import type { Account, NewAccountInput, Overview, Project, TerminalEvent } from "./types";

const PALETTE: [string, ...string[]] = ["#7AA2F7", "#BB9AF7", "#9ECE6A", "#FF9E64", "#7DCFFF", "#FF7AB6", "#E0AF68", "#73DACA"];

function slug(name: string): string {
  return name
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");
}

function encode(text: string): string {
  return btoa(String.fromCharCode(...new TextEncoder().encode(text)));
}

/**
 * In-memory `HedyApi` for tests and for running the UI in a browser
 * (`npm run dev` without Tauri). Applies only the rules the UI relies on.
 */
export class MockApi implements HedyApi {
  private state: Overview;

  constructor(initial?: Partial<Overview>) {
    this.state = {
      userName: "Enrique",
      theme: "system",
      dataDir: "/home/demo/.local/share/hedy",
      defaultAccount: null,
      binaries: { claude: null, codex: null, gemini: null },
      accounts: [],
      projects: [],
      ...initial,
    };
  }

  static withSampleData(): MockApi {
    const api = new MockApi({ defaultAccount: "claude-personal" });
    const account = (id: string, name: string, extra: Partial<Account>): Account => ({
      id,
      name,
      provider: "claude",
      providerName: "Claude Code",
      kind: "subscription",
      folder: `/home/demo/.local/share/hedy/accounts/${id}`,
      defaultFolder: false,
      color: PALETTE[0],
      login: { state: "loggedIn", plan: "max" },
      projects: [],
      ...extra,
    });
    api.state.accounts = [
      account("claude-personal", "Claude personal", { color: "#BB9AF7", projects: ["/code/hedy"] }),
      account("claude-trabajo", "Claude trabajo", {
        folder: null,
        defaultFolder: true,
        login: { state: "loggedIn", plan: "team" },
        projects: ["/code/homming"],
      }),
      account("chatgpt", "ChatGPT", {
        provider: "codex",
        providerName: "Codex",
        color: "#9ECE6A",
        login: { state: "loggedOut", plan: null },
      }),
      account("kimi", "Kimi", {
        provider: "kimi",
        providerName: "Kimi",
        kind: "apiKey",
        folder: null,
        color: "#FF9E64",
        login: { state: "loggedOut", plan: null },
      }),
    ];
    api.state.projects = [
      project("/code/hedy", "claude-personal", { color: "#73DACA" }),
      project("/code/homming", "claude-trabajo", { protected: true, color: "#FF9E64" }),
    ];
    api.recomputeOffers();
    return api;
  }

  async overview(): Promise<Overview> {
    return structuredClone(this.state);
  }

  async addAccount(input: NewAccountInput): Promise<string> {
    const id = slug(input.name);
    if (!id) throw "el nombre de la cuenta no puede estar vacío";
    if (this.state.accounts.some((a) => a.id === id)) {
      throw `ya existe una cuenta con el identificador «${id}»`;
    }
    this.state.accounts.push({
      id,
      name: input.name.trim(),
      provider: input.provider,
      providerName: { claude: "Claude Code", codex: "Codex", gemini: "Gemini", kimi: "Kimi" }[input.provider],
      kind: input.apiKey ? "apiKey" : "subscription",
      folder: input.apiKey || input.defaultFolder ? null : `${this.state.dataDir}/accounts/${id}`,
      defaultFolder: input.defaultFolder,
      color: input.color ?? PALETTE[this.state.accounts.length % PALETTE.length] ?? PALETTE[0],
      login: { state: "loggedOut", plan: null },
      projects: [],
    });
    this.state.defaultAccount ??= id;
    this.recomputeOffers();
    return id;
  }

  async setApiKey(id: string, key: string): Promise<void> {
    if (!key.trim()) throw "la API key está vacía";
    this.account(id).login = { state: "loggedIn", plan: null };
  }

  async removeAccount(id: string): Promise<void> {
    if (this.account(id).projects.length > 0) {
      throw `la cuenta «${id}» está asignada a proyectos; asígnales otra antes de borrarla`;
    }
    this.state.accounts = this.state.accounts.filter((a) => a.id !== id);
    if (this.state.defaultAccount === id) this.state.defaultAccount = null;
    this.recomputeOffers();
  }

  async setDefaultAccount(id: string | null): Promise<void> {
    this.state.defaultAccount = id;
  }

  async setAccountColor(id: string, color: string): Promise<void> {
    this.account(id).color = color;
  }

  async useAccount(path: string, account: string, protect?: boolean): Promise<void> {
    let project = this.state.projects.find((p) => p.path === path);
    if (!project) {
      project = { ...emptyProject(path) };
      this.state.projects.push(project);
    }
    project.account = account;
    if (protect !== undefined) project.protected = protect;
    for (const a of this.state.accounts) {
      a.projects = this.state.projects.filter((p) => p.account === a.id).map((p) => p.path);
    }
    this.recomputeOffers();
  }

  async offerableAccounts(path: string): Promise<string[]> {
    return this.offersFor(path);
  }

  async setProjectColor(path: string, color: string | null): Promise<void> {
    this.project(path).color = color;
  }

  async removeProject(path: string): Promise<void> {
    this.state.projects = this.state.projects.filter((p) => p.path !== path);
    for (const a of this.state.accounts) a.projects = a.projects.filter((p) => p !== path);
    this.recomputeOffers();
  }

  async setSetting(key: string, value: string | null): Promise<void> {
    if (key === "ui.theme") this.state.theme = (value ?? "system") as Overview["theme"];
    if (key === "ui.name") this.state.userName = value ?? "Enrique";
    if (key.startsWith("binary.")) {
      this.state.binaries[key.slice(7) as keyof Overview["binaries"]] = value;
    }
  }

  async openInEditor(): Promise<void> {}

  async pickFolder(): Promise<string | null> {
    return "/code/nuevo-proyecto";
  }

  private terminals = 0;

  async terminalOpen(
    request: Parameters<HedyApi["terminalOpen"]>[0],
    _size: { cols: number; rows: number },
    onEvent: (event: TerminalEvent) => void,
  ): Promise<number> {
    const id = ++this.terminals;
    const text = `\x1b[38;2;125;207;255mTerminal de demostración\x1b[0m (${request.kind})\r\n`;
    queueMicrotask(() => onEvent({ event: "data", data: encode(text) }));
    return id;
  }

  async terminalWrite(): Promise<void> {}
  async terminalResize(): Promise<void> {}
  async terminalClose(): Promise<void> {}

  private account(id: string): Account {
    const account = this.state.accounts.find((a) => a.id === id);
    if (!account) throw `no existe la cuenta «${id}»`;
    return account;
  }

  private project(path: string): Project {
    const project = this.state.projects.find((p) => p.path === path);
    if (!project) throw `no existe el proyecto ${path}`;
    return project;
  }

  private offersFor(path: string): string[] {
    const here = this.state.projects.find((p) => p.path === path);
    if (here?.protected && here.account) return [here.account];
    const reserved = new Set(
      this.state.projects.filter((p) => p.protected && p.path !== path).map((p) => p.account),
    );
    return this.state.accounts.map((a) => a.id).filter((id) => !reserved.has(id));
  }

  private recomputeOffers(): void {
    for (const p of this.state.projects) p.offerableAccounts = this.offersFor(p.path);
  }
}

function emptyProject(path: string): Project {
  return {
    path,
    name: path.split("/").filter(Boolean).pop() ?? path,
    account: null,
    protected: false,
    color: null,
    offerableAccounts: [],
  };
}

function project(path: string, account: string, extra: Partial<Project>): Project {
  return { ...emptyProject(path), account, ...extra };
}
