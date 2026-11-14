import type { HedyApi, Unsubscribe } from "./HedyApi";
import type {
  Account,
  AgentEvent,
  Commit,
  Decision,
  FileDiff,
  NewAccountInput,
  NewTaskInput,
  Overview,
  Project,
  Task,
  TerminalEvent,
  Usage,
} from "./types";

const NO_USAGE: Usage = { weekTokens: 0, weekCostUsd: null, limit: null };

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
      usage: NO_USAGE,
      ...extra,
    });
    api.state.accounts = [
      account("claude-personal", "Claude personal", {
        color: "#BB9AF7",
        projects: ["/code/hedy"],
        usage: { weekTokens: 182_000, weekCostUsd: 3.1, limit: { status: "allowed", percent: 34, resetsAt: null } },
      }),
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
      usage: NO_USAGE,
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

  // ---------------------------------------------------------------- tasks
  // A scripted Claude: plan → approval → edits → a test command → done.

  /** Delay between scripted events (0 in tests). */
  delayMs = 250;
  private tasksById = new Map<string, Task>();
  private eventsById = new Map<string, AgentEvent[]>();
  private diffs = new Map<string, FileDiff[]>();
  private eventListeners = new Set<(taskId: string, event: AgentEvent) => void>();
  private taskListeners = new Set<(task: Omit<Task, "live">) => void>();
  private nextTask = 0;

  async startTask(input: NewTaskInput): Promise<Task> {
    const project = this.project(input.project);
    if (!project.account) throw "este proyecto no tiene cuenta";
    if (!input.prompt.trim()) throw "escribe qué quieres que haga";
    const id = `t${++this.nextTask}`;
    const title = (input.prompt.split("\n")[0] ?? "").slice(0, 60);
    const task: Task = {
      id,
      project: project.path,
      mode: input.mode,
      title,
      prompt: input.prompt,
      account: project.account,
      model: input.model,
      workspace:
        input.mode === "project"
          ? {
              worktree: `${project.path}/.hedy/worktrees/${id}`,
              branch: `hedy/${title.toLowerCase().replace(/[^a-z0-9]+/g, "-").slice(0, 30)}-${id}`,
              baseBranch: "main",
              baseCommit: "abc1234",
            }
          : null,
      sessionId: null,
      status: "running",
      createdAt: Math.floor(Date.now() / 1000) + this.nextTask,
      live: true,
    };
    this.tasksById.set(id, task);
    this.eventsById.set(id, []);
    this.diffs.set(id, []);
    this.push(id, { kind: "userMessage", text: input.prompt });
    this.script(id, [
      { kind: "sessionStarted", sessionId: `s-${id}`, model: "claude-opus-5-5" },
      { kind: "message", text: "Voy a revisar dónde se crean las citas." },
      { kind: "toolUsed", tool: "Grep", summary: "crear_cita" },
      ...(input.mode !== "project"
        ? [
            { kind: "message", text: "Las citas se crean en citas/servicio.py, en crear_cita()." } as AgentEvent,
            { kind: "finished", ok: true, summary: "", durationMs: 3000 } as AgentEvent,
          ]
        : input.planFirst
          ? [{ kind: "plan", text: "1. Buscar dónde se crean las citas\n2. Programar el recordatorio\n3. Correr los tests", requestId: "plan-1" } as AgentEvent]
          : this.implementation(id)),
    ]);
    return { ...task };
  }

  private implementation(id: string): AgentEvent[] {
    const steps = (done: number): AgentEvent => ({
      kind: "steps",
      steps: ["Buscar dónde se crean las citas", "Tarea programada y plantilla del mensaje", "Correr los tests"].map(
        (text, i) => ({ text, status: i < done ? "done" : i === done ? "inProgress" : "pending" }),
      ),
    });
    void id;
    return [
      steps(1),
      { kind: "fileEdited", path: "citas/servicio.py" },
      { kind: "fileEdited", path: "citas/recordatorios.py" },
      steps(2),
      { kind: "permissionRequest", requestId: "req-1", tool: "Bash", summary: "pytest tests/test_recordatorios.py", input: { command: "pytest tests/test_recordatorios.py" } },
    ];
  }

  private finish(): AgentEvent[] {
    return [
      { kind: "testsRun", command: "pytest tests/test_recordatorios.py", passed: true },
      { kind: "steps", steps: ["Buscar dónde se crean las citas", "Tarea programada y plantilla del mensaje", "Correr los tests"].map((text) => ({ text, status: "done" as const })) },
      { kind: "message", text: "Listo: el recordatorio se programa 24 h antes y los tests pasan." },
      { kind: "usage", inputTokens: 1200, outputTokens: 800, cacheReadTokens: 9000, cacheWriteTokens: 0, costUsd: 0.04 },
      { kind: "finished", ok: true, summary: "Listo", durationMs: 42_000 },
    ];
  }

  private script(id: string, events: AgentEvent[]): void {
    const [first, ...rest] = events;
    if (!first) return;
    setTimeout(() => {
      if (!this.tasksById.get(id)?.live) return;
      this.push(id, first);
      this.script(id, rest);
    }, this.delayMs);
  }

  private push(id: string, event: AgentEvent): void {
    this.eventsById.get(id)?.push(event);
    const task = this.tasksById.get(id);
    if (task) {
      const status =
        event.kind === "plan" || event.kind === "permissionRequest"
          ? "waitingApproval"
          : event.kind === "finished"
            ? "review"
            : event.kind === "permissionResolved" || event.kind === "userMessage"
              ? "running"
              : task.status;
      if (event.kind === "fileEdited") {
        this.diffs.get(id)?.push(sampleDiff(event.path));
      }
      if (status !== task.status) {
        task.status = status;
        this.notifyTask(task);
      }
    }
    for (const listener of this.eventListeners) listener(id, event);
  }

  private notifyTask(task: Task): void {
    for (const listener of this.taskListeners) listener({ ...task });
  }

  async tasks(project: string): Promise<Task[]> {
    return [...this.tasksById.values()]
      .filter((t) => t.project === project)
      .sort((a, b) => b.createdAt - a.createdAt)
      .map((t) => ({ ...t }));
  }

  async taskEvents(id: string): Promise<AgentEvent[]> {
    return [...(this.eventsById.get(id) ?? [])];
  }

  async answerPermission(id: string, requestId: string, decision: Decision): Promise<void> {
    this.push(id, { kind: "permissionResolved", requestId, decision, automatic: false, reason: null });
    this.script(id, decision === "deny" ? [{ kind: "message", text: "Entendido, no corro los tests." }, { kind: "finished", ok: true, summary: "", durationMs: null }] : this.finish());
  }

  async answerPlan(id: string, requestId: string, approve: boolean, feedback: string | null): Promise<void> {
    this.push(id, { kind: "permissionResolved", requestId, decision: approve ? "allow" : "deny", automatic: false, reason: null });
    this.script(
      id,
      approve
        ? this.implementation(id)
        : [{ kind: "plan", text: `1. Buscar\n2. Cambiar (${feedback ?? "revisado"})\n3. Tests`, requestId: `${requestId}-b` }],
    );
  }

  async sendMessage(id: string, text: string): Promise<void> {
    const task = this.tasksById.get(id);
    if (!task || task.status === "accepted" || task.status === "discarded") throw "la tarea ya está cerrada";
    task.live = true;
    this.push(id, { kind: "userMessage", text });
    this.script(id, [{ kind: "message", text: `Hecho: ${text}` }, { kind: "finished", ok: true, summary: "", durationMs: 1000 }]);
  }

  async stopTask(id: string): Promise<void> {
    const task = this.tasksById.get(id);
    if (!task) return;
    task.live = false;
    task.status = "review";
    this.notifyTask(task);
  }

  async taskDiff(id: string): Promise<FileDiff[]> {
    return [...(this.diffs.get(id) ?? [])];
  }

  async acceptTask(id: string): Promise<void> {
    this.close(id, "accepted");
  }

  async discardTask(id: string): Promise<void> {
    this.close(id, "discarded");
  }

  private close(id: string, status: "accepted" | "discarded"): void {
    const task = this.tasksById.get(id);
    if (!task) throw `no existe la tarea ${id}`;
    task.live = false;
    task.status = status;
    this.diffs.set(id, []);
    this.notifyTask(task);
  }

  async recentCommits(): Promise<Commit[]> {
    return [
      { hash: "a3f9c1e", subject: "Arreglo del envío" },
      { hash: "7bd20aa", subject: "Plantillas" },
    ];
  }

  onTaskEvent(listener: (taskId: string, event: AgentEvent) => void): Unsubscribe {
    this.eventListeners.add(listener);
    return () => this.eventListeners.delete(listener);
  }

  onTaskUpdated(listener: (task: Omit<Task, "live">) => void): Unsubscribe {
    this.taskListeners.add(listener);
    return () => this.taskListeners.delete(listener);
  }

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

function sampleDiff(path: string): FileDiff {
  return {
    path,
    oldPath: null,
    status: "modified",
    additions: 3,
    deletions: 1,
    binary: false,
    hunks: [
      {
        header: "@@ -12,5 +12,7 @@",
        lines: [
          { kind: "context", oldLine: 12, newLine: 12, text: "def crear_cita(cliente, servicio, hora):" },
          { kind: "context", oldLine: 13, newLine: 13, text: "    cita = Cita.nueva(cliente, servicio, hora)" },
          { kind: "removed", oldLine: 14, newLine: null, text: "    cita.guardar()" },
          { kind: "added", oldLine: null, newLine: 14, text: "    cita.guardar()" },
          { kind: "added", oldLine: null, newLine: 15, text: "    programar_recordatorio(cita, horas=24)" },
          { kind: "added", oldLine: null, newLine: 16, text: "    return cita" },
        ],
      },
    ],
  };
}
