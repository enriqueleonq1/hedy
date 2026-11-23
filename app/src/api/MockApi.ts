import type { HedyApi, Unsubscribe } from "./HedyApi";
import type {
  Blame,
  CodeCopy,
  CodeDiff,
  CodeSymbol,
  CodeTree,
  CopyInfo,
  DiffOptions,
  FileContent,
  FileHistory,
  LineRange,
  Rev,
  SearchOptions,
  SearchResults,
  WriteResult,
} from "./codeTypes";
import { MockCode, SUGGESTED_BLOCKED } from "./mockCode";
import { MOCK_ROLES, mockProblems, mockProjectFlow, mockTeamFlow, mockTemplates } from "./mockFlows";
import { MOCK_THEME_SEEDS } from "./mockThemes";
import { resolveLanguage } from "../i18n";
import { placeholders } from "../templates/placeholders";
import { buildTheme } from "../theme/derive";
import type {
  BoardEntry,
  Flow,
  FlowEntry,
  FlowIssue,
  FlowValue,
  RoleEntry,
  TeamShare,
  ImportPick,
  TrialResult,
  TrialOutcome,
  TrialCheck,
  StartedTrial,
  SkillVerdict,
  SkillReview,
  SkillOverview,
  SkillFinding,
  ImportReport,
  ImportPreview,
  ImportCandidate,
  ExportReport,
  AskScope,
  CodeLookup,
  GraphNode,
  MemoryAnswer,
  MemoryGraph,
  MemorySource,
  RuleBreak,
  RuleCheck,
  MemoryHome,
  MemoryHomeKind,
  SyncConflict,
  SyncReport,
  SyncStatus,
  TeamCreated,
  TeamInvite,
  TeamJoin,
  TeamNote,
  TeamPerson,
  TeamRole,
  TeamStatus,
  TeamSuggestion,
  TeamSummary,
  TeamWrite,
  CustomTheme,
  ThemeBase,
  ThemeDraft,
  AcceptMode,
  AcceptPreview,
  FolderStatus,
  GhState,
  NewPullRequestInput,
  PullOutcome,
  PullRequest,
  PullRequestDraft,
  PushOutcome,
  ShipFailure,
  Shipment,
  ShipState,
  Account,
  Attachment,
  CarryInput,
  ContextTurn,
  ConversationContext,
  TaskOrigin,
  ImageInput,
  ImageMediaType,
  ImageSupport,
  ActivityItem,
  AgentEvent,
  BoardState,
  Check,
  Commit,
  Decision,
  DetectedCli,
  DoctorReport,
  FileDiff,
  OutputFile,
  PermissionMode,
  MemoryView,
  NewAccountInput,
  NewConversationInput,
  NewProjectInput,
  Note,
  NoteCategory,
  NewTaskInput,
  NewTemplateInput,
  OpenTaskRequest,
  Overview,
  PhasePlan,
  Project,
  ProjectStatus,
  Skill,
  SkillChange,
  SkillDraft,
  SkillFiles,
  SkillsView,
  PromptTemplate,
  AccountCommands,
  AgentCommand,
  QueuedMessage,
  HoldReason,
  UsagePoint,
  UsageSummary,
  Task,
  TerminalEvent,
  Usage,
  PlanState,
  UsageWindow,
} from "./types";
import { taskState } from "../tasks/state";

const NO_PLAN: PlanState = { usage: null, attemptedAt: null, problem: null, supported: false };
const NO_USAGE: Usage = { weekTokens: 0, weekCostUsd: null, limit: null, plan: NO_PLAN };

/** Like the backend: Claude Code and Codex subscriptions, and Kimi API keys. */
function planSupported(provider: string, kind: Account["kind"]): boolean {
  return kind === "subscription" ? provider === "claude" || provider === "codex" : provider === "kimi";
}

/** A `/usage` reading of Claude Code, as the backend caches it. */
function claudePlan(
  now: number,
  session: number,
  week: number,
  models: [string, number][],
  sessionResetsIn: number,
): PlanState {
  const window = (kind: UsageWindow["kind"], percent: number, resetsAt: number, model: string | null = null) => ({
    kind,
    minutes: kind === "session" ? 300 : 10080,
    model,
    percent,
    resetsText: null,
    resetsAt,
  });
  const weekResets = now + 4 * 86_400 + 3 * 3600;
  return {
    usage: {
      windows: [
        window("session", session, now + sessionResetsIn),
        window("week", week, weekResets),
        ...models.map(([model, percent]) => window("week", percent, weekResets, model)),
      ],
      checkedAt: now - 120,
      source: "claudeUsage",
      balance: null,
    },
    attemptedAt: now - 120,
    problem: null,
    supported: true,
  };
}

/** The built-in examples, as the backend has them (`domain/template.rs`). */
const BUILTIN_TEMPLATES: Record<"es" | "en", [string, string, string][]> = {
  es: [
    ["explain", "Explicar código", "Explica qué hace {archivo}, cómo encaja en el resto del proyecto y qué partes conviene tocar con cuidado."],
    ["tests", "Escribir pruebas", "Escribe pruebas para {archivo}. Cubre los casos normales y los bordes, y ejecútalas al terminar."],
    ["review", "Revisar el cambio", "Revisa los cambios de esta tarea como lo haría un compañero: errores, casos sin cubrir y nombres poco claros. Propón arreglos concretos."],
    ["fix", "Corregir un error", "Aparece este error: {error}. Encuentra la causa, corrígela y explica qué pasaba."],
    ["summary", "Resumir lo hecho", "Resume lo que hiciste: qué cambiaste, por qué y qué queda pendiente."],
  ],
  en: [
    ["explain", "Explain code", "Explain what {file} does, how it fits in the rest of the project and which parts need care."],
    ["tests", "Write tests", "Write tests for {file}. Cover the usual cases and the edge cases, and run them when you finish."],
    ["review", "Review the change", "Review the changes in this task as a teammate would: bugs, uncovered cases and unclear names. Suggest concrete fixes."],
    ["fix", "Fix an error", "This error shows up: {error}. Find the cause, fix it and explain what was going on."],
    ["summary", "Summarize the work", "Summarize what you did: what you changed, why, and what is left to do."],
  ],
};

/** A stored template row, like the backend's: the user's, or an edited/deleted built-in. */
interface TemplateRow {
  id: number;
  project: string | null;
  name: string;
  text: string;
  builtin: string | null;
  deleted: boolean;
}


const PALETTE: [string, ...string[]] = ["#7AA2F7", "#BB9AF7", "#9ECE6A", "#FF9E64", "#7DCFFF", "#FF7AB6", "#E0AF68", "#73DACA"];

function slug(name: string): string {
  return name
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");
}

/** Like the backend's `project::folder_name`: "Mi tienda online" → `mi-tienda-online`. */
export function folderName(name: string): string {
  return name
    .trim()
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9_.]+/g, "-")
    .replace(/^[-.]+/, "")
    .slice(0, 64)
    .replace(/[-.]+$/, "");
}

/** Board order, like the backend: what needs the user first, closed tasks last. */
const BOARD_RANK: Record<BoardState, number> = {
  plan: 0,
  permission: 0,
  limited: 1,
  working: 2,
  review: 3,
  failed: 4,
  answered: 5,
  accepted: 6,
  cancelled: 7,
};
const ACTIVE: BoardState[] = ["working", "plan", "permission", "limited"];

/** The image type from the first bytes of its base64, like the backend's sniffing. */
const IMAGE_PREFIX: [string, ImageMediaType][] = [
  ["iVBORw0KGg", "image/png"],
  ["/9j/", "image/jpeg"],
  ["R0lGOD", "image/gif"],
  ["UklGR", "image/webp"],
  ["JVBERi0", "application/pdf"],
];
const MAX_IMAGES = 4;
const MAX_IMAGE_BYTES = 5 * 1024 * 1024;

function encode(text: string): string {
  return btoa(String.fromCharCode(...new TextEncoder().encode(text)));
}

/**
 * In-memory `HedyApi` for tests and for running the UI in a browser
 * (`npm run dev` without Tauri). Applies only the rules the UI relies on.
 */
/** Commands like the ones each CLI reports, for the sample data. */
function sampleCommands(provider: string): AgentCommand[] {
  if (provider !== "claude") {
    return [
      { name: "init", description: "Create a context file for this project", source: "unknown", argumentHint: null },
      { name: "web", description: "Search the web", source: "unknown", argumentHint: "query to search for" },
    ];
  }
  return [
    {
      name: "design",
      description: "Design a screen or a component with the studio's style",
      source: "plugin",
      argumentHint: "<screen>",
    },
    { name: "deploy-notes", description: "Write release notes from the last tag", source: "custom", argumentHint: "[tag]" },
    { name: "brand-voice", description: "Rewrite a text in the brand's voice", source: "skill", argumentHint: null },
    {
      name: "code-review",
      description: "Review the current diff for correctness bugs",
      source: "builtin",
      argumentHint: "[low|medium|high]",
    },
    { name: "compact", description: "Free up context by summarizing the conversation so far", source: "builtin", argumentHint: null },
    { name: "usage", description: "Show session cost, plan usage, and what's contributing to your limits", source: "builtin", argumentHint: null },
  ];
}

export class MockApi implements HedyApi {
  private state: Overview;

  constructor(initial?: Partial<Overview>) {
    this.state = {
      userName: "Alex",
      theme: "system",
      themeDark: "hedy",
      themeLight: "hedy-light",
      language: "es",
      onboarded: true,
      dataDir: "/home/demo/.local/share/hedy",
      defaultAccount: null,
      binaries: { claude: null, codex: null, gemini: null, kimi: null, antigravity: null },
      apiUrls: { kimi: null, gemini: null },
      apiPrices: { kimi: null, gemini: null },
      accounts: [],
      projects: [],
      codeMap: { enabled: null, found: null },
      notifications: true,
      contextSaving: true,
      contextSavingByEnv: false,
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
      defaultFolder: false,
      color: PALETTE[0],
      login: { state: "loggedIn", plan: "max" },
      projects: [],
      usage: NO_USAGE,
      ...extra,
    });
    const now = Math.floor(Date.now() / 1000);
    api.state.accounts = [
      account("claude-personal", "Claude personal", {
        color: "#BB9AF7",
        projects: ["/code/hedy"],
        usage: {
          weekTokens: 182_000,
          weekCostUsd: 3.1,
          limit: { status: "allowed", percent: 34, resetsAt: null },
          plan: claudePlan(now, 58, 83, [["Opus", 12]], 3 * 3600),
        },
      }),
      account("claude-trabajo", "Claude trabajo", {
        defaultFolder: true,
        login: { state: "loggedIn", plan: "team" },
        projects: ["/code/tienda"],
        usage: { ...NO_USAGE, plan: claudePlan(now, 34, 29, [["Fable", 0]], 90 * 60) },
      }),
      account("chatgpt", "ChatGPT", {
        provider: "codex",
        providerName: "Codex",
        color: "#9ECE6A",
        login: { state: "loggedOut", plan: null },
        usage: { ...NO_USAGE, plan: { ...NO_PLAN, supported: true, attemptedAt: now - 60, problem: "unavailable" } },
      }),
      account("kimi", "Kimi", {
        provider: "kimi",
        providerName: "Kimi",
        kind: "apiKey",
        color: "#FF9E64",
        login: { state: "loggedOut", plan: null },
        usage: { ...NO_USAGE, plan: { ...NO_PLAN, supported: true } },
      }),
    ];
    api.setAccountCommands("claude-personal", null, sampleCommands("claude"));
    api.state.projects = [
      project("/code/hedy", "claude-personal", { color: "#73DACA", client: "Estudio Norte" }),
      project("/code/tienda", "claude-trabajo", { protected: true, color: "#FF9E64", client: "Café Aurora" }),
    ];
    api.recomputeOffers();
    const day = 86_400;
    // Personal sync between two PCs, with one note edited on both.
    api.repoOwners.set("/code/hedy", "github.com/alex");
    api.repoOwners.set("/code/tienda", "github.com/cafe-aurora");
    api.flowRecords.push({ id: api.nextFlow++, project: "/code/tienda", flow: mockProjectFlow() });
    api.teamProjects.add("/code/tienda");
    api.teamFlows.push({ uid: "u1", flow: mockTeamFlow(), by: "Marta" });
    api.owners.mine.add("github.com/alex");
    api.sync = {
      url: "git@github.com:alex/hedy-memory.git",
      lastSync: now - 120,
      lastError: null,
      devices: [
        { id: "pc-1", name: "Casa", lastSync: now - 120, thisPc: true },
        { id: "pc-2", name: "Oficina", lastSync: now - 3600, thisPc: false },
      ],
      conflicts: 0,
    };
    // A team, with no project connected (tests connect one).
    api.teamList = [sampleTeam(now)];
    api.syncConflictList = [
      {
        id: 1,
        kind: "note",
        kept: {
          text: "Los precios se muestran como «USD 12.50», con decimales solo si no son cero.",
          device: "pc-2",
          at: now - 3600,
        },
        other: {
          text: "Los precios se muestran como «USD 12.50», con dos decimales.",
          device: "pc-1",
          at: now - 3 * 3600,
        },
      },
    ];
    const note = (text: string, category: NoteCategory, extra: Partial<Note> = {}) =>
      api.notes.push({
        id: api.notes.length + 1,
        project: "/code/hedy",
        text,
        category,
        author: "user",
        status: "approved",
        createdAt: now - 30 * day,
        uses: 3,
        lastUsed: now - day,
        topic: null,
        updatedAt: null,
        ...extra,
      });
    note("Los recordatorios se envían 24 h antes de la cita.", "decision", { createdAt: now - 2 * day });
    note("Los recordatorios se envían 2 h antes de la cita.", "decision", { author: "Claude", createdAt: now - 21 * day });
    note("Los precios se guardan en USD y se muestran también en Bs a tasa BCV", "decision", { uses: 23 });
    note("Las citas usan la zona horaria del salón, no la del servidor", "decision", {
      author: "Claude",
      uses: 4,
      topic: "citas/zona-horaria",
    });
    note("Tests con pytest; correr pytest -q antes de cada commit", "convention", { uses: 31 });
    note("Nunca tocar .env ni correr migraciones en producción", "rule", { uses: 12 });
    note("El deploy se hace los viernes", "convention", { lastUsed: now - 90 * day, createdAt: now - 120 * day });
    note("Los comentarios del código van en inglés", "convention", { uses: 9 });
    note("Prefiere respuestas cortas y en español", "preference", { project: null, uses: 58 });
    const proposal = { status: "proposed", uses: 0, lastUsed: null, createdAt: now - 3600 } as const;
    note("Los formularios validan con zod", "convention", { ...proposal, author: "Codex" });
    // A change to the approved note with the same topic.
    note("Las citas usan la zona horaria del salón; el servidor guarda las fechas en UTC", "decision", {
      ...proposal,
      author: "Claude",
      topic: "citas/zona-horaria",
    });
    // A new note similar to an approved one.
    note("Los precios se guardan en USD", "decision", { ...proposal, author: "Codex" });
    api.usedLast = [3, 5];
    const skill = (name: string, description: string, body: string, extra: Partial<Skill> = {}) =>
      api.skillList.push({
        id: api.skillList.length + 1,
        name,
        description,
        body,
        project: "/code/hedy",
        status: "approved",
        author: "Claude",
        createdAt: now - 12 * day,
        updatedAt: null,
        sourceTask: "t-demo",
        ...extra,
      });
    skill(
      "correr-tests",
      "Correr los tests del backend y del frontend antes de entregar una tarea.",
      "1. `pytest -q` en la raíz.\n2. `npm test` en `web/`.\n3. Si falla un test de fechas, revisa la zona horaria del salón.",
    );
    skill(
      "publicar-version",
      "Lista de pasos para publicar una versión nueva.",
      "1. Sube la versión en `pyproject.toml`.\n2. Actualiza `CHANGELOG.md`.\n3. `git tag vX.Y.Z` y `git push --tags`.",
      { author: "user", sourceTask: null, createdAt: now - 40 * day },
    );
    skill(
      "commits-en-ingles",
      "Cómo escribir los mensajes de commit en todos los proyectos.",
      "Mensajes cortos, en inglés y en imperativo: «Add reminders by WhatsApp».",
      { project: null, author: "user", sourceTask: null },
    );
    const proposedSkill = { status: "proposed", createdAt: now - 1800 } as const;
    // A new procedure an agent worked out.
    skill(
      "agregar-endpoint",
      "Cómo agregar un endpoint nuevo a la API de citas.",
      "1. Crea la ruta en `api/routes/`.\n2. Valida la entrada con un esquema de pydantic.\n3. Agrega un test en `tests/api/` y corre `pytest -q tests/api`.",
      { ...proposedSkill, author: "Codex" },
    );
    // An update of an approved skill (same name).
    skill(
      "correr-tests",
      "Correr los tests del backend, del frontend y los e2e antes de entregar una tarea.",
      "1. `pytest -q` en la raíz.\n2. `npm test` en `web/`.\n3. `npm run test:e2e` en `web/` (necesita `npm run build` antes).\n4. Si falla un test de fechas, revisa la zona horaria del salón.",
      { ...proposedSkill, author: "Claude" },
    );
    // A week of usage: one record per day and project, busiest on the 4th day.
    const today = new Date();
    today.setHours(12, 0, 0, 0);
    [18, 42, 25, 61, 20, 0, 0].forEach((k, i) => {
      if (k === 0) return;
      const at = Math.floor(today.getTime() / 1000) - (6 - i) * day;
      // Subscriptions: the CLI's cost guess is recorded, never added up as spend.
      api.recordUsage({ project: "/code/hedy", account: "claude-personal", at, tokens: k * 1000, costUsd: k / 40 });
      api.recordUsage({ project: "/code/tienda", account: "claude-trabajo", at, tokens: k * 300, costUsd: null });
    });
    // One theme of the user's own, described with AI.
    void api.saveTheme({
      id: null,
      name: "Café de la tarde",
      base: "dark",
      colors: buildTheme(MOCK_THEME_SEEDS.cafe.seed, "dark").tokens,
      origin: "described",
    });
    return api;
  }

  // --------------------------------------------------------------- themes

  private customThemes: CustomTheme[] = [];
  private nextTheme = 0;
  /** What "Describir con IA" fails with (a limit, for example); null: it answers. */
  describeFailure: string | null = null;

  async listThemes(): Promise<CustomTheme[]> {
    return structuredClone(this.customThemes);
  }

  async saveTheme(input: Parameters<HedyApi["saveTheme"]>[0]): Promise<CustomTheme> {
    const name = input.name.trim();
    if (!name) throw this.english() ? "the theme needs a name" : "el tema necesita un nombre";
    const theme: CustomTheme = { ...input, name, id: input.id ?? `t${++this.nextTheme}` };
    const at = this.customThemes.findIndex((t) => t.id === theme.id);
    if (at >= 0) this.customThemes[at] = theme;
    else this.customThemes.push(theme);
    return structuredClone(theme);
  }

  async deleteTheme(id: string): Promise<void> {
    this.customThemes = this.customThemes.filter((t) => t.id !== id);
    for (const base of ["dark", "light"] as const) {
      const key = base === "dark" ? "themeDark" : "themeLight";
      if (this.state[key] === `custom:${id}`) this.state[key] = base === "dark" ? "hedy" : "hedy-light";
    }
  }

  async describeTheme(account: string, description: string, base: ThemeBase | null): Promise<ThemeDraft> {
    this.account(account);
    if (!description.trim()) throw this.english() ? "describe the theme first" : "describe primero el tema";
    await this.pause();
    if (this.describeFailure) throw this.describeFailure;
    const text = description.toLowerCase();
    const match =
      Object.values(MOCK_THEME_SEEDS).find((s) => s.words.some((w) => text.includes(w)) && (!base || s.base === base)) ??
      (base === "light" || (!base && /claro|light/.test(text)) ? MOCK_THEME_SEEDS.notebook : MOCK_THEME_SEEDS.calm);
    const name = description.trim().replace(/\s+/g, " ");
    return {
      name: name.length > 32 ? `${name.slice(0, 31)}…` : name.charAt(0).toUpperCase() + name.slice(1),
      base: match.base,
      colors: { ...match.seed },
    };
  }

  // --------------------------------------------------------------- usage

  private usage: (UsagePoint & { project: string; account: string })[] = [];

  /** What a finished turn records (sample data and tests). */
  recordUsage(record: UsagePoint & { project: string; account: string }) {
    this.usage.push(record);
  }

  async setProjectClient(path: string, client: string | null): Promise<void> {
    this.project(path).client = client?.trim() || null;
  }

  async setProjectBlockedFiles(path: string, patterns: string[]): Promise<void> {
    const clean = patterns.map((p) => (p.trim().split(/[\\/]/).pop() ?? "").toLowerCase()).filter(Boolean);
    this.project(path).blockedFiles = [...new Set(clean)];
  }

  async suggestedBlockedFiles(): Promise<string[]> {
    return [...SUGGESTED_BLOCKED];
  }

  async setProjectPermissionMode(path: string, mode: PermissionMode): Promise<void> {
    this.project(path).permissionMode = mode;
  }

  async setTaskPermissions(id: string, mode: PermissionMode): Promise<void> {
    const task = this.tasksById.get(id);
    if (!task) throw `no existe la tarea ${id}`;
    if (task.status === "accepted" || task.status === "discarded") throw `la tarea ${id} ya está cerrada`;
    task.permissions = mode;
    this.notifyTask(task);
  }

  async projectStatus(path: string): Promise<ProjectStatus> {
    const project = this.project(path);
    const view = await this.memory(path);
    return {
      branch: "main",
      uncommitted: 3,
      lastTests: { command: "pytest -q", passed: true, at: Math.floor(Date.now() / 1000) - 2 * 3600 },
      agents: [project.account, ...Object.values(project.phases).map((p) => p.account)].filter(
        (id, i, all): id is string => !!id && all.indexOf(id) === i,
      ),
      codeMap: this.state.codeMap.enabled !== null,
      memoryNotes: view.notes.length,
      memoryStale: view.stale.length,
      memoryProposed: view.proposed.length,
    };
  }

  async projectUsage(path: string, days: number): Promise<UsagePoint[]> {
    const since = Date.now() / 1000 - days * 86_400;
    return this.usage
      .filter((u) => u.project === path && u.at >= since)
      .map(({ at, tokens, costUsd }) => ({ at, tokens, costUsd }));
  }

  async usageSummary(days: number): Promise<UsageSummary> {
    const since = Math.floor(Date.now() / 1000 - days * 86_400);
    const add = (a: number | null, b: number | null) => (a === null && b === null ? null : (a ?? 0) + (b ?? 0));
    // Like the backend: only API keys are really paid.
    const paid = new Set(this.state.accounts.filter((a) => a.kind === "apiKey").map((a) => a.id));
    const projects = new Map<string, UsageSummary["projects"][number]>();
    for (const u of this.usage.filter((u) => u.at >= since)) {
      const project = this.state.projects.find((p) => p.path === u.project);
      const entry = projects.get(u.project) ?? {
        path: u.project,
        name: project?.name ?? u.project,
        client: project?.client ?? null,
        tokens: 0,
        costUsd: null,
      };
      entry.tokens += u.tokens;
      entry.costUsd = add(entry.costUsd, paid.has(u.account) ? u.costUsd : null);
      projects.set(u.project, entry);
    }
    const list = [...projects.values()].sort((a, b) => b.tokens - a.tokens);
    const clients = new Map<string | null, UsageSummary["clients"][number]>();
    for (const p of list) {
      const entry = clients.get(p.client) ?? { client: p.client, projects: 0, tokens: 0, costUsd: null };
      entry.projects += 1;
      entry.tokens += p.tokens;
      entry.costUsd = add(entry.costUsd, p.costUsd);
      clients.set(p.client, entry);
    }
    const byClient = [...clients.values()].sort(
      (a, b) => Number(a.client === null) - Number(b.client === null) || b.tokens - a.tokens,
    );
    return { projects: list, clients: byClient };
  }

  // --------------------------------------------------------------- memory

  private notes: Note[] = [];
  private dismissed: string[] = [];
  private usedLast: number[] = [];
  private memoryOff = new Map<string, Set<string>>();

  private inScope(path: string, n: Note): boolean {
    return n.project === path || (n.project === null && !this.project(path).protected);
  }

  async memory(path: string): Promise<MemoryView> {
    const project = this.project(path);
    const scoped = this.notes.filter((n) => this.inScope(path, n));
    const notes = scoped.filter((n) => n.status === "approved");
    const contradictions: [Note, Note][] = [];
    notes.forEach((a, i) =>
      notes.slice(i + 1).forEach((b) => {
        if (!this.dismissed.includes(`${a.id}-${b.id}`) && mockContradicts(a.text, b.text)) contradictions.push([a, b]);
      }),
    );
    const now = Date.now() / 1000;
    const readers = [project.account, ...Object.values(project.phases).map((p) => p.account)].filter(
      (id, i, all): id is string => !!id && all.indexOf(id) === i,
    );
    const off = this.memoryOff.get(path) ?? new Set();
    const proposed = scoped
      .filter((n) => n.status === "proposed")
      .map((note) => {
        const replaces = topicTarget(notes, note);
        const similar = notes
          .filter((n) => n.id !== replaces?.id && mockSimilar(n.text, note.text))
          .slice(0, 3);
        return { note, replaces, similar };
      });
    return structuredClone({
      notes,
      proposed,
      contradictions,
      stale: notes.filter((n) => now - (n.lastUsed ?? n.createdAt) > 60 * 86_400),
      usedLastTask: notes.filter((n) => this.usedLast.includes(n.id)),
      readers: readers.map((account) => ({ account, reads: !off.has(account) })),
      protected: project.protected,
    });
  }

  async searchMemory(path: string, query: string): Promise<Note[]> {
    const words = query.toLowerCase().split(/\s+/).filter(Boolean);
    return this.notes.filter(
      (n) =>
        this.inScope(path, n) &&
        n.status === "approved" &&
        words.some((w) => `${n.text} ${n.topic ?? ""}`.toLowerCase().includes(w)),
    );
  }

  async askMemory(question: string, scope: AskScope, account?: string | null): Promise<MemoryAnswer> {
    if (!question.trim()) throw "escribe una pregunta para la memoria";
    await this.pause();
    const terms = askTerms(question);
    const scored = this.memorySources(scope)
      .map((source) => ({ source, hits: terms.filter((t) => fold(`${source.text} ${source.topic ?? ""}`).includes(t)).length }))
      .sort((a, b) => b.hits - a.hits || b.source.at - a.source.at);
    const sources = scored.map((s) => s.source);
    if (sources.length === 0) return { text: null, sources, cited: [], account: null };
    const reader = this.state.accounts.find((a) => a.id === (account ?? this.state.defaultAccount)) ?? this.state.accounts[0];
    const best = scored.filter((s) => s.hits > 0).slice(0, 2);
    if (best.length === 0) return { text: null, sources, cited: [], account: reader?.name ?? null };
    const cited = best.map((_, i) => i + 1);
    const text = best.map((s, i) => `${s.source.text.replace(/\.$/, "")} [${i + 1}].`).join(" ");
    return { text, sources, cited, account: reader?.name ?? null };
  }

  /**
   * Like the backend's check, judged by two sample rules the sample notes
   * hold: comments in English, and camelCase names in TypeScript.
   */
  async checkTaskRules(task: string, account?: string | null): Promise<RuleCheck> {
    const found = this.tasksById.get(task);
    if (!found) throw `no existe la tarea ${task}`;
    if (found.status === "accepted" || found.status === "discarded") throw `la tarea ${task} ya está cerrada`;
    const files = this.diffs.get(task) ?? [];
    if (files.length === 0) throw "no hay cambios que revisar";
    await this.pause();
    const rank = (c: NoteCategory) => (c === "rule" || c === "convention" ? 0 : c === "decision" ? 1 : 2);
    const rules = this.memorySources({ kind: "project", project: found.project }).sort(
      (a, b) => rank(a.category) - rank(b.category) || b.at - a.at,
    );
    const isProtected = (path: string) => /(^|\/)\.env(\.|$)|\.(pem|key)$/i.test(path);
    const shown = files.filter((f) => !isProtected(f.path) && !f.binary && f.status !== "deleted");
    const added = shown.flatMap((f) =>
      f.hunks.flatMap((h) => h.lines).filter((l) => l.kind === "added" && l.newLine !== null).map((l) => ({ path: f.path, line: l })),
    );
    const base = {
      files: new Set(added.map((a) => a.path)).size,
      protected: files.filter((f) => isProtected(f.path)).map((f) => f.path),
      truncated: false,
    };
    if (rules.length === 0) return { status: "no_rules", breaks: [], rules, ...base, account: null };
    if (added.length === 0) return { status: "nothing_to_check", breaks: [], rules, ...base, account: null };
    const reader =
      this.state.accounts.find((a) => a.id === (account ?? this.project(found.project).account ?? this.state.defaultAccount)) ??
      this.state.accounts[0];
    const english = rules.findIndex((r) => /comentarios.*ingl[eé]s/i.test(r.text)) + 1;
    const camel = rules.findIndex((r) => /camelcase/i.test(r.text)) + 1;
    const breaks: RuleBreak[] = [];
    for (const { path, line } of added) {
      const code = line.text.trim();
      const comment = /^(#|\/\/)\s*(.*)$/.exec(code)?.[2] ?? "";
      if (english && /[áéíóúñ]|\b(el|la|se|por|como|del)\b/i.test(comment)) {
        breaks.push({ path, line: line.newLine ?? 0, rule: english, message: "El comentario está en español.", fix: "Escríbelo en inglés.", code, raw: line.text });
      }
      const snake = /\b(?:const|let|var)\s+([a-z]+_[a-z0-9_]+)/.exec(code)?.[1];
      if (camel && snake && /\.(ts|tsx|js|jsx)$/.test(path)) {
        const fixed = snake.replace(/_([a-z0-9])/g, (_, c: string) => c.toUpperCase());
        breaks.push({ path, line: line.newLine ?? 0, rule: camel, message: `\`${snake}\` no va en camelCase.`, fix: `Llámala \`${fixed}\`.`, code, raw: line.text });
      }
    }
    return { status: "checked", breaks, rules, ...base, account: reader?.name ?? "Claude" };
  }

  async lookInCode(question: string, project: string): Promise<CodeLookup> {
    if (!question.trim()) throw "escribe una pregunta para la memoria";
    await this.pause();
    const reader = this.state.accounts.find((a) => a.id === (this.project(project).account ?? this.state.defaultAccount)) ?? this.state.accounts[0];
    const account = reader?.name ?? "Claude";
    if (/tipograf/i.test(question)) {
      const text = "Los correos usan Inter a 15 px, definida en `emails/base.css`.";
      const proposal = await this.proposeNote(project, "Los correos usan Inter a 15 px (emails/base.css).", account);
      return { text, proposal, account };
    }
    return { text: null, proposal: null, account };
  }

  /** A note an agent proposes (it waits for approval). */
  private async proposeNote(project: string, text: string, author: string): Promise<Note> {
    const note: Note = {
      id: this.notes.length + 1,
      project,
      text,
      category: "convention",
      author,
      status: "proposed",
      createdAt: Date.now() / 1000,
      uses: 0,
      lastUsed: null,
      topic: null,
      updatedAt: null,
    };
    this.notes.push(note);
    return structuredClone(note);
  }

  /** What a question of `scope` reads, as the core picks it (protected projects only in their own). */
  private memorySources(scope: AskScope): MemorySource[] {
    const name = (path: string | null) => (path ? (path.split("/").pop() ?? path) : null);
    const open = (path: string | null) => !path || !this.project(path).protected;
    const local = this.notes
      .filter((n) => n.status === "approved")
      .filter((n) => (scope.kind === "project" ? this.inScope(scope.project, n) : scope.kind === "mine" && open(n.project)))
      .map(
        (n): MemorySource => ({
          text: n.text,
          category: n.category,
          topic: n.topic,
          layer: "mine",
          author: null,
          team: null,
          project: name(n.project),
          at: n.updatedAt ?? n.createdAt,
          note: { kind: "local", id: n.id },
        }),
      );
    const home = scope.kind === "project" ? this.homes.get(scope.project) : undefined;
    const teams = this.teamList.filter((t) =>
      scope.kind === "team" ? t.summary.id === scope.team : scope.kind === "mine" || (home?.kind === "team" && home.team === t.summary.id),
    );
    const team = teams.flatMap((t) =>
      t.notes
        .filter((n) => n.teamLayer || scope.kind !== "project" || n.project === scope.project)
        .map(
          (n): MemorySource => ({
            text: n.text,
            category: n.category,
            topic: n.topic,
            layer: n.teamLayer ? "team" : "project",
            author: n.author,
            team: t.summary.name,
            project: name(n.project),
            at: n.updatedAt ?? n.createdAt,
            note: { kind: "team", team: t.summary.id, uid: n.uid },
          }),
        ),
    );
    return [...team, ...local];
  }

  async memoryGraph(): Promise<MemoryGraph> {
    const nodes: GraphNode[] = [];
    const links: MemoryGraph["links"] = [];
    const seen = new Set<string>();
    const node = (id: string, kind: GraphNode["kind"], label: string, group: string, extra: Partial<GraphNode> = {}) => {
      if (!seen.has(id)) {
        seen.add(id);
        nodes.push({ id, kind, label, group, source: null, layer: null, ...extra });
      }
      return id;
    };
    const link = (a: string, b: string) => links.push({ source: a, target: b });
    const topic = (from: string, name: string | null) => name && link(from, node(`topic:${name}`, "topic", `# ${name}`, "topic"));
    node("you", "you", "Tú", "you");
    for (const p of this.state.projects) {
      node(`p:${p.path}`, "project", p.name, `project:${p.path}`);
      const home = this.homes.get(p.path);
      if (home?.kind === "team") link(`p:${p.path}`, `t:${home.team}`);
    }
    for (const t of this.teamList) node(`t:${t.summary.id}`, "team", t.summary.name, `team:${t.summary.id}`);
    for (const n of this.notes.filter((n) => n.status === "approved")) {
      const owner = n.project && seen.has(`p:${n.project}`) ? `p:${n.project}` : "you";
      const id = node(`n:${n.id}`, "note", n.text, owner === "you" ? "you" : `project:${n.project}`, {
        source: { kind: "local", id: n.id },
        layer: "mine",
      });
      link(id, owner);
      topic(id, n.topic);
    }
    for (const t of this.teamList) {
      const team = `t:${t.summary.id}`;
      for (const n of t.notes) {
        const id = node(`n:${t.summary.id}:${n.uid}`, "note", n.text, `team:${t.summary.id}`, {
          source: { kind: "team", team: t.summary.id, uid: n.uid },
          layer: n.teamLayer ? "team" : "project",
        });
        link(id, !n.teamLayer && n.project && seen.has(`p:${n.project}`) ? `p:${n.project}` : team);
        const person = node(`person:${t.summary.id}:${n.person}`, "person", `@${n.author}`, "person");
        link(id, person);
        link(person, team);
        topic(id, n.topic);
      }
    }
    return structuredClone({ nodes, links, hiddenNotes: 0 });
  }

  async addNote(project: string | null, text: string, category: NoteCategory): Promise<Note> {
    if (!text.trim()) throw "la nota está vacía";
    const note: Note = {
      id: Math.max(0, ...this.notes.map((n) => n.id)) + 1,
      project,
      text: text.trim(),
      category,
      author: "user",
      status: "approved",
      createdAt: Math.floor(Date.now() / 1000),
      uses: 0,
      lastUsed: null,
      topic: null,
      updatedAt: null,
    };
    this.notes.push(note);
    return note;
  }

  private note(id: number): Note {
    const note = this.notes.find((n) => n.id === id);
    if (!note) throw `no existe la nota ${id}`;
    return note;
  }

  async editNote(id: number, text: string, category: NoteCategory): Promise<Note> {
    if (!text.trim()) throw "la nota está vacía";
    const note = this.note(id);
    Object.assign(note, { text: text.trim(), category });
    return note;
  }

  async approveNote(id: number): Promise<void> {
    const note = this.note(id);
    if (note.status === "approved") return;
    const target = topicTarget(
      this.notes.filter((n) => n.status === "approved"),
      note,
    );
    if (target) this.revise(note, target);
    else note.status = "approved";
  }

  async replaceNote(proposal: number, target: number): Promise<void> {
    const note = this.note(proposal);
    if (note.status !== "proposed") throw `la nota ${proposal} no es una propuesta: ya está aprobada`;
    this.revise(note, this.note(target));
  }

  /** The proposal becomes the new version of `target`, which keeps its id and uses. */
  private revise(proposal: Note, target: Note) {
    Object.assign(target, {
      text: proposal.text,
      category: proposal.category,
      author: proposal.author,
      topic: target.topic ?? proposal.topic,
      updatedAt: Math.floor(Date.now() / 1000),
    });
    this.notes = this.notes.filter((n) => n.id !== proposal.id);
  }

  async forgetNote(id: number): Promise<void> {
    this.note(id);
    this.notes = this.notes.filter((n) => n.id !== id);
  }

  async keepNote(_keep: number, forget: number): Promise<void> {
    await this.forgetNote(forget);
  }

  async dismissContradiction(a: number, b: number): Promise<void> {
    this.dismissed.push(`${Math.min(a, b)}-${Math.max(a, b)}`);
  }

  async setMemoryReader(path: string, account: string, reads: boolean): Promise<void> {
    const off = this.memoryOff.get(path) ?? new Set<string>();
    if (reads) off.delete(account);
    else off.add(account);
    this.memoryOff.set(path, off);
  }

  // --------------------------------------------------------------- skills

  private skillList: Skill[] = [];

  private skillInScope(path: string, s: Skill): boolean {
    return s.project === path || (s.project === null && !this.project(path).protected);
  }

  private skillTarget(skill: Skill): Skill | null {
    return (
      this.skillList.find(
        (s) => s.id !== skill.id && s.status === "approved" && s.project === skill.project && s.name === skill.name,
      ) ?? null
    );
  }

  private skillById(id: number): Skill {
    const skill = this.skillList.find((s) => s.id === id);
    if (!skill) throw `no existe la habilidad «${id}» en este proyecto`;
    return skill;
  }

  /** Writes or removes the mock's skill files: two per project skill. */
  /** Like the backend: skills stay inside Hedy, so the sample project gets no file. */
  private skillFiles(): SkillFiles {
    return { written: [], removed: [], skipped: [] };
  }

  private checkDraft(draft: SkillDraft): string {
    const name = draft.name
      .trim()
      .toLowerCase()
      .normalize("NFD")
      .replace(/[\u0300-\u036f]/g, "")
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "");
    if (!name || name.length > 64) throw `nombre de habilidad no válido: «${draft.name.trim()}»`;
    if (!draft.description.trim()) throw "la descripción de la habilidad debe ser una línea";
    if (!draft.body.trim()) throw "el contenido de la habilidad no puede estar vacío";
    return name;
  }

  async skills(path: string): Promise<SkillsView> {
    const project = this.project(path);
    const scoped = this.skillList
      .filter((s) => this.skillInScope(path, s))
      .sort((a, b) => a.name.localeCompare(b.name) || a.id - b.id);
    return structuredClone({
      approved: scoped.filter((s) => s.status === "approved"),
      proposed: scoped.filter((s) => s.status === "proposed").map((skill) => ({ skill, replaces: this.skillTarget(skill) })),
      protected: project.protected,
    });
  }

  async addSkill(path: string, draft: SkillDraft): Promise<SkillChange> {
    const name = this.checkDraft(draft);
    const skill: Skill = {
      id: Math.max(0, ...this.skillList.map((s) => s.id)) + 1,
      name,
      description: draft.description.trim(),
      body: draft.body.trim(),
      project: draft.global ? null : path,
      status: "approved",
      author: "user",
      createdAt: Math.floor(Date.now() / 1000),
      updatedAt: null,
      sourceTask: null,
    };
    if (this.skillTarget(skill)) throw `ya hay otra habilidad aprobada llamada «${name}»`;
    this.skillList.push(skill);
    return { skill: structuredClone(skill), files: this.skillFiles() };
  }

  async editSkill(id: number, path: string, draft: SkillDraft): Promise<SkillChange> {
    const skill = this.skillById(id);
    const name = this.checkDraft(draft);
    Object.assign(skill, {
      name,
      description: draft.description.trim(),
      body: draft.body.trim(),
      project: draft.global ? null : path,
      updatedAt: Math.floor(Date.now() / 1000),
    });
    if (skill.status !== "approved") return { skill: structuredClone(skill), files: { written: [], removed: [], skipped: [] } };
    const removed = this.skillFiles().removed;
    return { skill: structuredClone(skill), files: { ...this.skillFiles(), removed } };
  }

  async approveSkill(id: number): Promise<SkillChange> {
    const proposal = this.skillById(id);
    let approved = proposal;
    const target = proposal.status === "proposed" ? this.skillTarget(proposal) : null;
    if (target) {
      Object.assign(target, {
        description: proposal.description,
        body: proposal.body,
        author: proposal.author,
        updatedAt: Math.floor(Date.now() / 1000),
      });
      this.skillList = this.skillList.filter((s) => s.id !== id);
      approved = target;
    } else {
      proposal.status = "approved";
    }
    return { skill: structuredClone(approved), files: this.skillFiles() };
  }

  async rejectSkill(id: number): Promise<void> {
    if (this.skillById(id).status !== "proposed") throw `la habilidad ${id} no es una propuesta: ya está aprobada`;
    this.skillList = this.skillList.filter((s) => s.id !== id);
  }

  async removeSkill(id: number): Promise<SkillFiles> {
    const skill = this.skillById(id);
    this.skillList = this.skillList.filter((s) => s.id !== id);
    this.outsideSkills.delete(id);
    this.trialResults.delete(id);
    return skill.status === "approved" ? this.skillFiles() : { written: [], removed: [], skipped: [] };
  }

  // ------------------------------------------------ skills section

  private outsideSkills = new Set<number>();
  private trialResults = new Map<number, TrialResult>();
  private trials = new Map<string, { skill: number; tasks: number; polls: number }>();

  /** Like the backend's level 1, in small: size, steps, nothing dangerous. */
  private measureSkill(description: string, body: string): { tokens: number; findings: SkillFinding[]; verdict: SkillVerdict } {
    const tokens = Math.ceil((description.length + body.length) / 4);
    const findings: SkillFinding[] = [];
    if (tokens > 5000) findings.push({ kind: "long", tokens });
    if (/todo sobre|everything about|cualquier tarea/i.test(description)) {
      findings.push({ kind: "broadDescription", phrase: description.split(/[,.]/)[0] ?? description });
    }
    findings.push(/^\s*(\d+\.|-)\s/m.test(body) ? { kind: "concreteSteps" } : { kind: "noSteps" });
    findings.push({ kind: "safe" });
    const warnings = findings.filter((f) => f.kind === "long" || f.kind === "noSteps" || f.kind === "broadDescription").length;
    return { tokens, findings, verdict: warnings >= 2 ? "doubtful" : "useful" };
  }

  private overviewOf(skill: Skill): SkillOverview {
    const measured = this.measureSkill(skill.description, skill.body);
    return {
      skill: structuredClone(skill),
      projectName: skill.project ? (this.state.projects.find((p) => p.path === skill.project)?.name ?? skill.project) : null,
      replaces: skill.status === "proposed" ? structuredClone(this.skillTarget(skill)) : null,
      outside: this.outsideSkills.has(skill.id),
      ...measured,
      trial: structuredClone(this.trialResults.get(skill.id) ?? null),
    };
  }

  async skillsOverview(): Promise<SkillOverview[]> {
    const name = (s: Skill) => (s.project ? (this.state.projects.find((p) => p.path === s.project)?.name ?? s.project) : "");
    return [...this.skillList]
      .sort((a, b) => name(a).localeCompare(name(b)) || a.name.localeCompare(b.name) || a.id - b.id)
      .map((s) => this.overviewOf(s));
  }

  async setSkillOutside(id: number, on: boolean): Promise<SkillFiles> {
    const skill = this.skillById(id);
    if (skill.project === null || skill.status !== "approved") {
      throw "solo una habilidad aprobada de un proyecto se puede usar también fuera de Hedy";
    }
    const paths = [".claude", ".agents"].map((d) => `${skill.project}/${d}/skills/${skill.name}/SKILL.md`);
    if (on) this.outsideSkills.add(id);
    else this.outsideSkills.delete(id);
    return on ? { written: paths, removed: [], skipped: [] } : { written: [], removed: paths, skipped: [] };
  }

  /** The skills a sample download holds: one that helps and one that does not. */
  private importSamples(source: string): { file: string; draft: SkillDraft }[] {
    if (!source.trim()) throw "no hay habilidades que importar ahí";
    const folder = source.replace(/\/SKILL\.md$/, "");
    return [
      {
        file: `${folder}/revisar-pr/SKILL.md`,
        draft: {
          name: "revisar-pr",
          description: "Antes de abrir un pull request.",
          body: "1. Actualiza `CHANGELOG.md`.\n2. Corre `npm test`.\n3. Revisa que el título siga Conventional Commits.",
          global: false,
        },
      },
      {
        file: `${folder}/mega-react-guide/SKILL.md`,
        draft: {
          name: "mega-react-guide",
          description: "Todo sobre React: hooks, estado, estilos, tests y despliegue.",
          body: "Escribe código limpio y bien testeado.\n".repeat(1200),
          global: false,
        },
      },
    ];
  }

  async previewSkillImport(project: string, source: string, global: boolean): Promise<ImportPreview> {
    this.project(project);
    const candidates: ImportCandidate[] = this.importSamples(source).map(({ file, draft }) => {
      const taken = this.skillList.find(
        (s) => s.status === "approved" && s.name === draft.name && s.project === (global ? null : project),
      );
      return {
        source: file,
        name: draft.name,
        description: draft.description,
        body: draft.body,
        droppedKeys: [],
        extraFiles: [],
        replaces: structuredClone(taken ?? null),
        problem: taken ? { kind: "nameTaken", message: "ya tienes una habilidad aprobada con ese nombre" } : null,
        ...this.measureSkill(draft.description, draft.body),
        fingerprint: this.printOf(draft),
      };
    });
    return { candidates, total: candidates.length };
  }

  private sampleReview(): SkillReview {
    return {
      findings: [
        {
          kind: "contradicts",
          rule: 1,
          note: "Los estilos van en CSS modules",
          says: "usa CSS-in-JS",
        },
        { kind: "vague", quote: "Escribe código limpio y bien testeado", why: "el modelo ya lo hace" },
        { kind: "helpful", quote: "Corre `npm test`", why: "un paso concreto que se puede comprobar" },
      ],
      unread: false,
      notes: 3,
      durationMs: 6200,
      verdict: "harmful",
      account: "Claude Personal",
    };
  }

  /** Like the backend's fingerprint, in small: enough to tell a changed text. */
  private printOf(draft: SkillDraft): string {
    return `${draft.name}:${draft.description.length}:${draft.body.length}`;
  }

  async reviewSkillImport(project: string, source: string, _global: boolean, pick: ImportPick): Promise<SkillReview> {
    this.project(project);
    if (!this.importSamples(source).some((s) => s.file === pick.file && this.printOf(s.draft) === pick.fingerprint)) {
      throw "no hay habilidades que importar ahí";
    }
    return this.sampleReview();
  }

  async importSkills(
    project: string,
    source: string,
    global: boolean,
    picks: ImportPick[],
    replace: string[],
  ): Promise<ImportReport> {
    const report: ImportReport = { imported: [], updated: [], skipped: [], files: this.skillFiles() };
    const wanted = (s: { file: string; draft: SkillDraft }) =>
      picks.some((p) => p.file === s.file && p.fingerprint === this.printOf(s.draft));
    for (const { draft } of this.importSamples(source).filter(wanted)) {
      const scope = global ? null : project;
      const taken = this.skillList.find((s) => s.status === "approved" && s.name === draft.name && s.project === scope);
      if (taken && !replace.includes(draft.name)) {
        report.skipped.push({ name: draft.name, problem: { kind: "nameTaken", message: "ya tienes una habilidad aprobada con ese nombre" } });
        continue;
      }
      if (taken) {
        Object.assign(taken, { description: draft.description, body: draft.body, updatedAt: Math.floor(Date.now() / 1000) });
        report.updated.push(structuredClone(taken));
        continue;
      }
      const change = await this.addSkill(project, { ...draft, global });
      report.imported.push(change.skill);
    }
    return report;
  }

  async reviewSkill(id: number, project: string): Promise<SkillReview> {
    this.skillById(id);
    this.project(project);
    return { ...this.sampleReview(), findings: [{ kind: "helpful", quote: "Corre `npm test`", why: "un paso concreto" }], verdict: "useful" };
  }

  async exportSkills(ids: number[], to: string): Promise<ExportReport> {
    const skills = ids.map((id) => this.skillById(id));
    return { written: skills.map((s) => `${to}/${s.name}/SKILL.md`), skipped: [] };
  }

  async proposeTrialTasks(project: string, id: number): Promise<string[]> {
    this.project(project);
    const skill = this.skillById(id);
    return [`Usa «${skill.name}» para preparar el cambio del carrito.`, "Arregla el test de fechas que falla en `web/`."];
  }

  async trialCheck(project: string): Promise<TrialCheck> {
    this.project(project);
    return { check: "npm test", isolated: true };
  }

  async startSkillTrial(project: string, id: number, tasks: string[], check: string | null): Promise<StartedTrial> {
    this.project(project);
    if (check !== null && !check.trim()) throw "el comando de la comprobación está vacío";
    this.skillById(id);
    const prompts = tasks.map((t) => t.trim()).filter(Boolean);
    if (prompts.length < 1 || prompts.length > 3) throw "elige de 1 a 3 tareas para probar la habilidad";
    const trial = `k${this.trials.size + 1}`;
    this.trials.set(trial, { skill: id, tasks: prompts.length, polls: 0 });
    return { id: trial, tasks: prompts.flatMap((_, i) => [`${trial}-${i}-con`, `${trial}-${i}-sin`]) };
  }

  async cancelSkillTrial(_project: string, trial: string): Promise<void> {
    this.trials.delete(trial);
  }

  /** The sample trial ends on the second look. */
  async takeTrialOutcome(trial: string): Promise<TrialOutcome | null> {
    const running = this.trials.get(trial);
    if (!running) return null;
    running.polls += 1;
    if (running.polls < 2) return null;
    this.trials.delete(trial);
    const n = running.tasks;
    const result: TrialResult = {
      verdict: "useful",
      tasks: n,
      passedWith: n,
      passedWithout: Math.max(0, n - 1),
      used: n,
      tokensWith: 12100 * n,
      tokensWithout: 11200 * n,
      costWithMicros: 0,
      costWithoutMicros: 0,
      checked: true,
      restored: 0,
      at: Math.floor(Date.now() / 1000),
    };
    this.trialResults.set(running.skill, result);
    return { judged: structuredClone(result) };
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
    // Like the backend: Antigravity keeps one login per machine, in its default folder.
    const onlyDefault = input.provider === "antigravity";
    if (onlyDefault && this.state.accounts.some((a) => a.provider === "antigravity")) {
      throw "ya hay una cuenta que usa la carpeta por defecto de Antigravity";
    }
    this.state.accounts.push({
      id,
      name: input.name.trim(),
      provider: input.provider,
      providerName: {
        claude: "Claude Code",
        codex: "Codex",
        gemini: "Gemini",
        kimi: "Kimi",
        antigravity: "Antigravity",
      }[input.provider],
      kind: input.apiKey ? "apiKey" : "subscription",
      defaultFolder: input.defaultFolder || onlyDefault,
      color: input.color ?? PALETTE[this.state.accounts.length % PALETTE.length] ?? PALETTE[0],
      login: { state: "loggedOut", plan: null },
      projects: [],
      usage: {
        ...NO_USAGE,
        plan: { ...NO_PLAN, supported: planSupported(input.provider, input.apiKey ? "apiKey" : "subscription") },
      },
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

  async renameAccount(id: string, name: string): Promise<void> {
    if (!name.trim()) throw new Error("El nombre de la cuenta no puede estar vacío");
    this.account(id).name = name.trim();
  }

  async setAccountColor(id: string, color: string): Promise<void> {
    this.account(id).color = color;
  }

  /** Plays the backend's cache: the reading stays, only the attempt time moves. */
  async refreshPlanUsage(id: string): Promise<PlanState> {
    // Like the backend, the answer comes after an overview asked for at the same time.
    await new Promise((resolve) => setTimeout(resolve, 0));
    const account = this.account(id);
    const plan = account.usage.plan;
    if (plan.supported) {
      account.usage = { ...account.usage, plan: { ...plan, attemptedAt: Math.floor(Date.now() / 1000) } };
    }
    return structuredClone(account.usage.plan);
  }

  async useAccount(path: string, account: string, protect?: boolean): Promise<void> {
    let project = this.state.projects.find((p) => p.path === path);
    if (!project) {
      project = { ...emptyProject(path) };
      this.state.projects.push(project);
    }
    // Like the core's `follow_project_account`: required steps on the old
    // project account move with it (keeping the model on the same provider).
    const old = project.account;
    if (old && old !== account) {
      const provider = (id: string) => this.state.accounts.find((a) => a.id === id)?.provider;
      const same = provider(old) !== undefined && provider(old) === provider(account);
      const phases = { ...project.phases };
      for (const phase of ["plan", "implement"] as const) {
        const step = phases[phase];
        if (!step || step.account !== old) continue;
        if (same && step.model) phases[phase] = { account, model: step.model };
        else phases[phase] = undefined;
      }
      project.phases = Object.fromEntries(Object.entries(phases).filter(([, step]) => step)) as typeof project.phases;
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

  async setProjectPhases(path: string, phases: PhasePlan): Promise<void> {
    this.project(path).phases = phases;
  }

  /** Folders that already exist on the pretend disk (besides the projects). */
  private folders = new Set<string>(["/code/notas"]);

  private english(): boolean {
    return resolveLanguage(this.state.language) === "en";
  }

  async defaultProjectParent(): Promise<string | null> {
    const counts = new Map<string, number>();
    for (const p of this.state.projects) {
      const parent = p.path.slice(0, p.path.lastIndexOf("/")) || "/";
      counts.set(parent, (counts.get(parent) ?? 0) + 1);
    }
    const best = [...counts].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))[0];
    return best?.[0] ?? (this.english() ? "/home/demo/Projects" : "/home/demo/Proyectos");
  }

  async newProjectPath(name: string, parent: string): Promise<string> {
    const folder = folderName(name);
    if (!/[a-z0-9]/.test(folder)) {
      if (!name.trim()) throw this.english() ? "give the project a name" : "ponle un nombre al proyecto";
      throw this.english()
        ? `«${name.trim()}» does not work as a folder name: use letters or digits`
        : `«${name.trim()}» no sirve como nombre de carpeta: usa letras o números`;
    }
    if (!parent.startsWith("/")) {
      throw this.english()
        ? `the folder to create the project in must be a full path: ${parent}`
        : `la carpeta donde crear el proyecto debe ser una ruta completa: ${parent}`;
    }
    const path = `${parent.replace(/\/+$/, "")}/${folder}`;
    if (this.folders.has(path) || this.state.projects.some((p) => p.path === path)) {
      throw this.english()
        ? `${path} already exists; pick another name or open it with “Open folder…”`
        : `ya existe ${path}; elige otro nombre o ábrela con «Abrir carpeta…»`;
    }
    return path;
  }

  async newProject(input: NewProjectInput): Promise<string> {
    const path = await this.newProjectPath(input.name, input.parent);
    const account = input.account ?? this.state.defaultAccount;
    if (account && !this.offersFor(path).includes(account)) throw `la cuenta «${account}» está reservada`;
    this.folders.add(path);
    this.state.projects.push({
      ...emptyProject(path),
      name: input.name.trim(),
      account,
      protected: input.protect,
    });
    for (const a of this.state.accounts) {
      a.projects = this.state.projects.filter((p) => p.account === a.id).map((p) => p.path);
    }
    this.recomputeOffers();
    return path;
  }

  async removeProject(path: string): Promise<void> {
    this.state.projects = this.state.projects.filter((p) => p.path !== path);
    for (const a of this.state.accounts) a.projects = a.projects.filter((p) => p !== path);
    this.recomputeOffers();
  }

  async setSetting(key: string, value: string | null): Promise<void> {
    if (key === "ui.theme") this.state.theme = (value ?? "system") as Overview["theme"];
    if (key === "ui.theme.dark") this.state.themeDark = value ?? "hedy";
    if (key === "ui.theme.light") this.state.themeLight = value ?? "hedy-light";
    if (key === "ui.name") this.state.userName = value ?? "Alex";
    if (key === "ui.language") this.state.language = (value ?? "system") as Overview["language"];
    if (key === "ui.onboarded") this.state.onboarded = value === "true";
    if (key === "ui.notifications") this.state.notifications = value !== "off";
    if (key === "context_saving") this.state.contextSaving = value !== "off";
    if (key.startsWith("api_price.")) {
      this.state.apiPrices[key.slice(10) as keyof Overview["apiPrices"]] = value;
    }
    if (key.startsWith("api_url.")) {
      this.state.apiUrls[key.slice(8) as keyof Overview["apiUrls"]] = value;
    }
    if (key.startsWith("binary.")) {
      this.state.binaries[key.slice(7) as keyof Overview["binaries"]] = value;
    }
  }

  async openInEditor(): Promise<void> {}

  async openLink(): Promise<void> {}

  /** Claude logged in, Codex and Kimi Code installed, Antigravity missing. */
  detected: DetectedCli[] = [
    { provider: "claude", installed: true, login: { state: "loggedIn", plan: "max" }, account: null },
    { provider: "codex", installed: true, login: { state: "loggedOut", plan: null }, account: null },
    { provider: "gemini", installed: false, login: { state: "unknown", plan: null }, account: null },
    { provider: "kimi", installed: true, login: { state: "unknown", plan: null }, account: null },
    { provider: "antigravity", installed: false, login: { state: "unknown", plan: null }, account: null },
  ];

  async detectClis(): Promise<DetectedCli[]> {
    return this.detected.map((d) => ({
      ...d,
      account:
        this.state.accounts.find((a) => a.provider === d.provider && a.defaultFolder)?.id ?? d.account,
    }));
  }

  /** One CLI updated since the last check, one missing, and a key in the shell. */
  async doctor(): Promise<DoctorReport> {
    const checks: Check[] = [
      { area: "cli", subject: "Claude Code", level: "ok", message: "2.1.306 (verificada)", fix: null },
      {
        area: "cli",
        subject: "Codex",
        level: "warning",
        message: "0.169.0 → 0.171.0: cambió desde la última revisión. Hedy se verificó con 0.169.0",
        fix: null,
      },
      { area: "cli", subject: "Gemini CLI", level: "info", message: "no está instalada", fix: null },
      {
        area: "cli",
        subject: "Kimi Code",
        level: "info",
        message: "1.4.0 · aún no verificada con Hedy",
        fix: null,
      },
      { area: "cli", subject: "Antigravity", level: "info", message: "no está instalada", fix: null },
      ...this.state.accounts.map(
        (a): Check => ({
          area: "account",
          subject: `${a.name} (${a.id})`,
          level: a.login.state === "loggedOut" ? "warning" : "ok",
          message: a.login.state === "loggedOut" ? "sin sesión" : "sesión iniciada",
          fix: a.login.state === "loggedOut" ? `hedy account login ${a.id}` : null,
        }),
      ),
      { area: "hedy", subject: "Carpeta de datos", level: "ok", message: "~/.local/share/hedy", fix: null },
      { area: "hedy", subject: "Base de datos", level: "ok", message: "íntegra (quick_check)", fix: null },
      { area: "hedy", subject: "git", level: "ok", message: "instalado", fix: null },
      {
        area: "hedy",
        subject: "Memoria y hook de Claude",
        level: "ok",
        message: "los sirve /usr/bin/hedy-app",
        fix: null,
      },
      {
        area: "environment",
        subject: "ANTHROPIC_API_KEY",
        level: "info",
        message: "está en tu shell; Hedy la quita al lanzar las CLIs",
        fix: null,
      },
    ];
    return {
      checks,
      warnings: checks.filter((c) => c.level === "warning").length,
      errors: checks.filter((c) => c.level === "error").length,
    };
  }

  async pickFolder(): Promise<string | null> {
    return "/code/nuevo-proyecto";
  }

  async pickFile(): Promise<string | null> {
    return "/home/tu/Descargas/skills/revisar-pr/SKILL.md";
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

  // ---------------------------------------------------------------- commands

  /** What each account's CLI reported, by account and project ("" outside any). */
  private knownCommands = new Map<string, { commands: AgentCommand[]; seenAt: number }>();

  /** As the backend: Codex and API keys tell Hedy no commands. */
  private reportsCommands(account: string): boolean {
    const found = this.account(account);
    return found.kind === "subscription" && found.provider !== "codex";
  }

  /** Sets what `account` reported (tests and the sample data). */
  setAccountCommands(account: string, project: string | null, commands: AgentCommand[] | null): void {
    const key = `${account}\u0000${project ?? ""}`;
    if (commands) this.knownCommands.set(key, { commands, seenAt: Math.floor(Date.now() / 1000) });
    else this.knownCommands.delete(key);
  }

  /** An account's first answer brings its commands, as the CLI reports them. */
  private learnCommands(account: string, project: string | null): void {
    if (!this.reportsCommands(account)) return;
    const mine = [...this.knownCommands.keys()].some((k) => k.startsWith(`${account}\u0000`));
    if (!mine) this.setAccountCommands(account, project, sampleCommands(this.account(account).provider));
  }

  async accountCommands(account: string, project: string | null): Promise<AccountCommands> {
    const supported = this.reportsCommands(account);
    const prefix = `${account}\u0000`;
    const known =
      (project !== null ? this.knownCommands.get(prefix + project) : undefined) ??
      this.knownCommands.get(prefix) ??
      [...this.knownCommands.entries()]
        .filter(([k]) => k.startsWith(prefix))
        .map(([, v]) => v)
        .sort((a, b) => b.seenAt - a.seenAt)[0];
    return { supported, commands: known?.commands ?? null, seenAt: known?.seenAt ?? null };
  }

  // ---------------------------------------------------------------- templates

  private templateRows: TemplateRow[] = [];
  private nextTemplate = 0;

  private toTemplate(row: TemplateRow): PromptTemplate {
    return {
      id: String(row.id),
      project: row.project,
      name: row.name,
      text: row.text,
      builtin: row.builtin,
      placeholders: placeholders(row.text),
    };
  }

  private visibleTemplates(project: string | null | "all"): PromptTemplate[] {
    const wanted = (p: string | null) => project === "all" || p === null || p === project;
    const own = this.templateRows
      .filter((r) => r.builtin === null && !r.deleted && wanted(r.project))
      .sort((a, b) => Number(a.project === null) - Number(b.project === null))
      .map((r) => this.toTemplate(r));
    const language = resolveLanguage(this.state.language);
    for (const [key, name, text] of BUILTIN_TEMPLATES[language]) {
      const row = this.templateRows.find((r) => r.builtin === key);
      if (row?.deleted) continue;
      own.push(
        row
          ? this.toTemplate(row)
          : { id: `builtin:${key}`, project: null, name, text, builtin: key, placeholders: placeholders(text) },
      );
    }
    return own;
  }

  private checkTemplate(project: string | null, name: string, text: string, except?: string): void {
    if (!name.trim() || name.length > 60 || name.includes("\n")) throw `nombre de plantilla no válido: «${name}»`;
    if (!text.trim()) throw "la plantilla está vacía";
    const taken = this.visibleTemplates("all").some(
      (t) => t.project === project && t.name.toLowerCase() === name.trim().toLowerCase() && t.id !== except,
    );
    if (taken) throw `ya hay una plantilla llamada «${name.trim()}» en ese alcance`;
  }

  async templates(project: string | null): Promise<PromptTemplate[]> {
    return this.visibleTemplates(project);
  }

  async allTemplates(): Promise<PromptTemplate[]> {
    return this.visibleTemplates("all");
  }

  async addTemplate(input: NewTemplateInput): Promise<PromptTemplate> {
    this.checkTemplate(input.project, input.name, input.text);
    const row: TemplateRow = {
      id: ++this.nextTemplate,
      project: input.project,
      name: input.name.trim(),
      text: input.text.trim(),
      builtin: null,
      deleted: false,
    };
    this.templateRows.push(row);
    return this.toTemplate(row);
  }

  async updateTemplate(id: string, name: string, text: string): Promise<PromptTemplate> {
    const current = this.visibleTemplates("all").find((t) => t.id === id);
    if (!current) throw `no existe la plantilla «${id}»`;
    this.checkTemplate(current.project, name, text, id);
    let row = this.templateRows.find((r) => String(r.id) === id);
    if (!row) {
      row = { id: ++this.nextTemplate, project: null, name, text, builtin: current.builtin, deleted: false };
      this.templateRows.push(row);
    }
    row.name = name.trim();
    row.text = text.trim();
    return this.toTemplate(row);
  }

  async removeTemplate(id: string): Promise<void> {
    const current = this.visibleTemplates("all").find((t) => t.id === id);
    if (!current) throw `no existe la plantilla «${id}»`;
    const row = this.templateRows.find((r) => String(r.id) === id);
    if (current.builtin === null) {
      this.templateRows = this.templateRows.filter((r) => r !== row);
    } else if (row) {
      row.deleted = true;
    } else {
      this.templateRows.push({
        id: ++this.nextTemplate,
        project: null,
        name: current.name,
        text: current.text,
        builtin: current.builtin,
        deleted: true,
      });
    }
  }

  async restoreBuiltinTemplates(): Promise<void> {
    this.templateRows = this.templateRows.filter((r) => r.builtin === null);
  }

  /** A couple of the user's own templates, besides the built-in examples. */
  withSampleTemplates(): this {
    void this.addTemplate({
      project: "/code/hedy",
      name: "Revisar accesibilidad",
      text: "Revisa la accesibilidad de {pantalla}: contraste, foco con teclado y lectores de pantalla.",
    });
    void this.addTemplate({
      project: null,
      name: "Preparar entrega",
      text: "Prepara la entrega: actualiza el changelog, revisa que los tests pasen y resume los cambios para {cliente}.",
    });
    return this;
  }

  // ---------------------------------------------------------------- queue
  // Like the backend: messages written during a turn wait and go together
  // when it ends; after a failure, a stop or a limit they wait on hold.

  private queues = new Map<string, QueuedMessage[]>();
  private nextQueued = 0;
  private queueListeners = new Set<(taskId: string, queue: QueuedMessage[]) => void>();

  private emitQueue(id: string): QueuedMessage[] {
    const queue = (this.queues.get(id) ?? []).map((m) => ({ ...m }));
    for (const listener of this.queueListeners) listener(id, queue);
    return queue;
  }

  private busy(task: Task): boolean {
    return task.live && (task.status === "running" || task.status === "waitingApproval");
  }

  private holdQueue(id: string, reason: HoldReason): void {
    const queue = this.queues.get(id) ?? [];
    if (!queue.some((m) => m.held === null)) return;
    for (const m of queue) m.held ??= reason;
    this.emitQueue(id);
  }

  /** At the end of a turn: what was written meanwhile goes as the next turn. */
  private deliverQueue(id: string): void {
    const queue = this.queues.get(id) ?? [];
    const ready = queue.filter((m) => m.held === null);
    if (ready.length === 0) return;
    this.queues.set(
      id,
      queue.filter((m) => m.held !== null),
    );
    this.emitQueue(id);
    void this.sendMessage(id, ready.map((m) => m.text.trim()).join("\n\n"));
  }

  private queuedMessage(message: number): QueuedMessage {
    for (const queue of this.queues.values()) {
      const found = queue.find((m) => m.id === message);
      if (found) return found;
    }
    throw "ese mensaje ya no está en cola: puede que ya se haya enviado";
  }

  async queued(id: string): Promise<QueuedMessage[]> {
    return (this.queues.get(id) ?? []).map((m) => ({ ...m }));
  }

  async queueMessage(id: string, text: string, images?: ImageInput[]): Promise<QueuedMessage[]> {
    const task = this.tasksById.get(id);
    if (!task || task.status === "accepted" || task.status === "discarded") throw "la tarea ya está cerrada";
    if (!text.trim()) throw "escribe qué quieres que haga";
    if (!this.busy(task)) {
      await this.sendMessage(id, text.trim(), images);
      return this.queued(id);
    }
    if (images?.length) throw "las imágenes no pueden esperar en la cola: envíalas cuando el agente termine su turno";
    const queue = this.queues.get(id) ?? [];
    queue.push({ id: ++this.nextQueued, task: id, text: text.trim(), createdAt: Math.floor(Date.now() / 1000), held: null });
    this.queues.set(id, queue);
    return this.emitQueue(id);
  }

  async editQueued(message: number, text: string): Promise<QueuedMessage[]> {
    if (!text.trim()) throw "escribe qué quieres que haga";
    const found = this.queuedMessage(message);
    found.text = text.trim();
    return this.emitQueue(found.task);
  }

  async removeQueued(message: number): Promise<QueuedMessage[]> {
    const found = this.queuedMessage(message);
    this.queues.set(
      found.task,
      (this.queues.get(found.task) ?? []).filter((m) => m.id !== message),
    );
    return this.emitQueue(found.task);
  }

  async sendQueuedNow(id: string): Promise<QueuedMessage[]> {
    const task = this.tasksById.get(id);
    if (!task) throw `no existe la tarea ${id}`;
    for (const m of this.queues.get(id) ?? []) m.held = null;
    if (this.busy(task)) return this.emitQueue(id);
    this.deliverQueue(id);
    return this.queued(id);
  }

  onTaskQueue(listener: (taskId: string, queue: QueuedMessage[]) => void): Unsubscribe {
    this.queueListeners.add(listener);
    return () => this.queueListeners.delete(listener);
  }

  // ---------------------------------------------------------------- tasks
  // A scripted Claude: plan → approval → edits → a test command → done.

  /** Delay between scripted events (0 in tests). */
  delayMs = 250;
  private tasksById = new Map<string, Task>();

  // ---------------------------------------------------------------- flows

  private flowRecords: { id: number; project: string | null; flow: Flow }[] = [];
  private nextFlow = 1;

  async flows(project: string | null): Promise<FlowEntry[]> {
    const mine = (place: string | null): FlowEntry[] =>
      this.flowRecords
        .filter((r) => r.project === place)
        .map((r) => ({ key: `#${r.id}`, scope: place ? "project" : "global", summary: "", flow: structuredClone(r.flow) }));
    const team: FlowEntry[] =
      project && this.teamProjects.has(project)
        ? this.teamFlows.map((f) => ({
            key: `team:${f.uid}`,
            scope: "team",
            summary: "",
            flow: structuredClone(f.flow),
            by: f.by,
            unapproved: this.unapproved(f.flow),
          }))
        : [];
    return [...mockTemplates(), ...(project ? mine(project) : []), ...mine(null), ...team];
  }

  /** The sample team ("Café Aurora", tienda's): its shared flows, and the commands this PC allowed. */
  private teamProjects = new Set<string>();
  private teamFlows: { uid: string; flow: Flow; by: string }[] = [];
  private approvedCommands = new Set<string>();

  private unapproved(flow: Flow): string[] {
    return flow.nodes.flatMap((n) => (n.kind === "condition" && n.check === "tests" && n.command && !this.approvedCommands.has(n.command) ? [n.command] : []));
  }

  async flowSharing(project: string): Promise<TeamShare | null> {
    return this.teamProjects.has(project) ? { team: "aurora", name: "Café Aurora", admin: true } : null;
  }

  async shareFlow(project: string, key: string): Promise<string> {
    if (!this.teamProjects.has(project)) throw "este proyecto no está en un equipo";
    const record = this.flowRecords.find((r) => `#${r.id}` === key);
    if (!record) throw `no hay un flujo «${key}»`;
    const flow = structuredClone(record.flow);
    for (const n of flow.nodes) {
      if (n.kind === "role") delete n.account;
      if (n.kind === "parallel") n.roles.forEach((s) => delete s.account);
    }
    const same = this.teamFlows.find((f) => f.flow.name.toLowerCase() === flow.name.toLowerCase());
    if (same) same.flow = flow;
    else this.teamFlows.push({ uid: `u${this.teamFlows.length + 1}`, flow, by: "Alex" });
    return flow.name;
  }

  async unshareFlow(_project: string, name: string): Promise<void> {
    this.teamFlows = this.teamFlows.filter((f) => f.flow.name !== name);
  }

  async approveFlowCommand(command: string): Promise<void> {
    this.approvedCommands.add(command);
  }

  async flowProblems(_project: string | null, flow: Flow): Promise<FlowIssue[]> {
    return mockProblems(flow, MOCK_ROLES);
  }

  async saveFlow(project: string | null, key: string | null, flow: Flow): Promise<FlowEntry> {
    const problems = mockProblems(flow, MOCK_ROLES);
    if (problems.length) throw problems.map((p) => p.message).join("; ");
    if (mockTemplates().some((t) => t.flow.name.toLowerCase() === flow.name.trim().toLowerCase())) {
      throw `«${flow.name}» es el nombre de una plantilla de Hedy: elige otro`;
    }
    const editing = key ? this.flowRecords.find((r) => `#${r.id}` === key) : undefined;
    if (key && !editing) throw `no hay un flujo «${key}»`;
    const place = editing ? editing.project : project;
    const same = this.flowRecords.find((r) => r.project === place && r.flow.name.toLowerCase() === flow.name.trim().toLowerCase());
    if (same && same !== editing) throw `ya hay un flujo «${flow.name}»: elige otro nombre`;
    const record = editing ?? { id: this.nextFlow++, project: place, flow };
    record.flow = structuredClone(flow);
    if (!editing) this.flowRecords.push(record);
    return { key: `#${record.id}`, scope: place ? "project" : "global", summary: "", flow: structuredClone(flow) };
  }

  async deleteFlow(_project: string | null, key: string): Promise<void> {
    this.flowRecords = this.flowRecords.filter((r) => `#${r.id}` !== key);
  }

  async flowRoles(): Promise<RoleEntry[]> {
    return MOCK_ROLES.map((role) => ({ scope: "hedy", role }));
  }

  /** The sample project's own flow, after a dozen closed tasks: costly for what it solved. */
  flowValueSamples: Record<string, FlowValue[]> = {};

  async flowValues(project: string): Promise<FlowValue[]> {
    return this.flowValueSamples[project] ?? [];
  }

  async answerFlowApproval(id: string, approve: boolean, feedback: string | null): Promise<void> {
    const task = this.tasksById.get(id);
    if (!task || task.status !== "waitingApproval") throw "esta tarea no espera tu aprobación";
    if (!approve && feedback) this.boards.set(id, [...(this.boards.get(id) ?? []), { section: "feedback", role: "", round: 0, text: feedback, byUser: true }]);
    this.push(id, { kind: "flowApprovalAnswered", node: task.flow?.position.node ?? "", approved: approve });
    task.status = approve ? "review" : "running";
    this.notifyTask(task);
  }

  async taskBoard(id: string): Promise<BoardEntry[]> {
    return structuredClone(this.boards.get(id) ?? []);
  }

  private boards = new Map<string, BoardEntry[]>();
  /** What questions wrote in their own folder. */
  private outputs = new Map<string, OutputFile[]>();
  private eventsById = new Map<string, AgentEvent[]>();
  private diffs = new Map<string, FileDiff[]>();
  private eventListeners = new Set<(taskId: string, event: AgentEvent) => void>();
  private taskListeners = new Set<(task: Omit<Task, "live">) => void>();
  private openTaskListeners = new Set<(request: OpenTaskRequest) => void>();
  /** Unix seconds of each task's latest event. */
  private updatedAt = new Map<string, number>();
  private nextTask = 0;

  async startTask(input: NewTaskInput): Promise<Task> {
    const project = this.project(input.project);
    if (!project.account) throw "este proyecto no tiene cuenta";
    if (!input.prompt.trim()) throw "escribe qué quieres que haga";
    this.learnCommands(project.account, project.path);
    const carried = input.carry ? await this.carried(input.carry, project.account, input.images?.length ?? 0) : null;
    const id = `t${++this.nextTask}`;
    const own = await this.storeImages(id, project.account, input.images);
    const images = [...own, ...(carried?.images ?? []).map((image) => this.copyAttachment(carried?.origin.conversation ?? "", id, image))];
    const title = (input.prompt.split("\n")[0] ?? "").slice(0, 60);
    const prompt = carried ? `${carried.origin.context}\n\n${input.prompt.trim()}` : input.prompt;
    const planFirst = input.planFirst && input.mode === "project";
    const flowEntry = input.flow ? (await this.flows(project.path)).find((f) => f.key === input.flow) : undefined;
    if (flowEntry?.unapproved?.length) throw `este flujo del equipo corre «${flowEntry.unapproved[0]}» en tu PC: permítelo antes de ejecutarlo`;
    const stage = (phase: "plan" | "implement") => ({ phase, account: project.account ?? "", model: input.model });
    const task: Task = {
      id,
      project: project.path,
      mode: input.mode,
      title,
      prompt,
      account: project.account,
      model: input.model,
      permissions: input.permissions ?? project.permissionMode,
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
      stages: planFirst ? [stage("plan"), stage("implement")] : [stage("implement")],
      stage: 0,
      live: true,
      ...(images.length ? { images } : {}),
      ...(carried ? { origin: carried.origin } : {}),
      ...(flowEntry
        ? {
            flow: {
              flow: flowEntry.flow,
              roles: Object.fromEntries(MOCK_ROLES.map((r) => [r.id, r])),
              position: { node: flowEntry.flow.edges.find((e) => e.from === flowEntry.flow.nodes.find((n) => n.kind === "start")?.id)?.to ?? "" },
            },
          }
        : {}),
    };
    this.tasksById.set(id, task);
    this.eventsById.set(id, []);
    this.diffs.set(id, []);
    this.push(id, images.length ? { kind: "userMessage", text: prompt, images } : { kind: "userMessage", text: prompt });
    this.script(id, [
      { kind: "phaseStarted", phase: planFirst ? "plan" : "implement", agent: "Claude Opus", account: "Claude personal" },
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
          : this.implementation()),
    ]);
    return { ...task };
  }

  async startConversation(input: NewConversationInput): Promise<Task> {
    if (input.mode === "project") throw "una pregunta rápida solo pregunta o investiga";
    if (!input.prompt.trim()) throw "escribe qué quieres que haga";
    const account = input.account ?? this.state.defaultAccount;
    if (!account) throw "no hay cuenta para esta conversación";
    const name = this.account(account).name;
    this.learnCommands(account, null);
    const id = `t${++this.nextTask}`;
    const images = await this.storeImages(id, account, input.images);
    const task: Task = {
      id,
      project: `${this.state.dataDir}/chats/${id}`,
      mode: input.mode,
      title: (input.prompt.split("\n")[0] ?? "").slice(0, 60),
      prompt: input.prompt,
      account,
      model: input.model,
      workspace: null,
      sessionId: null,
      status: "running",
      createdAt: Math.floor(Date.now() / 1000) + this.nextTask,
      stages: [{ phase: "implement", account, model: input.model }],
      stage: 0,
      live: true,
      conversation: true,
      ...(images.length ? { images } : {}),
    };
    this.tasksById.set(id, task);
    this.eventsById.set(id, []);
    this.diffs.set(id, []);
    this.push(id, images.length ? { kind: "userMessage", text: input.prompt, images } : { kind: "userMessage", text: input.prompt });
    const english = this.english();
    this.script(id, [
      { kind: "phaseStarted", phase: "implement", agent: "Claude Opus", account: name },
      { kind: "sessionStarted", sessionId: `s-${id}`, model: "claude-opus-5-5" },
      ...(input.mode === "research"
        ? [{ kind: "toolUsed", tool: "WebSearch", summary: input.prompt.slice(0, 40) } as AgentEvent]
        : []),
      {
        kind: "message",
        text: english
          ? "Short answer: it depends on what you need. Here are the key differences, with an example of each."
          : "Respuesta corta: depende de lo que necesites. Te dejo las diferencias clave, con un ejemplo de cada una.",
      },
      { kind: "usage", inputTokens: 300, outputTokens: 400, cacheReadTokens: 0, cacheWriteTokens: 0, costUsd: 0.01 },
      { kind: "finished", ok: true, summary: "", durationMs: 2000 },
    ]);
    return { ...task };
  }

  async conversations(): Promise<Task[]> {
    return [...this.tasksById.values()]
      .filter((t) => t.conversation)
      .sort((a, b) => b.createdAt - a.createdAt)
      .map((t) => ({ ...t }));
  }

  /**
   * A conversation another Hedy process stored (`hedy ask` in a terminal):
   * it is in the store, but no event or update reaches this app (test helper).
   */
  storeOutsideConversation(prompt: string, events: AgentEvent[], status: Task["status"] = "review"): Task {
    const account = this.state.defaultAccount ?? this.state.accounts[0]?.id ?? "claude-personal";
    const id = `t${++this.nextTask}`;
    const task: Task = {
      id,
      project: `${this.state.dataDir}/chats/${id}`,
      mode: "ask",
      title: (prompt.split("\n")[0] ?? "").slice(0, 60),
      prompt,
      account,
      model: null,
      workspace: null,
      sessionId: `s-${id}`,
      status,
      createdAt: Math.floor(Date.now() / 1000) + this.nextTask,
      stages: [{ phase: "implement", account, model: null }],
      stage: 0,
      live: false,
      conversation: true,
    };
    this.tasksById.set(id, task);
    this.eventsById.set(id, [{ kind: "userMessage", text: prompt }, ...events]);
    this.diffs.set(id, []);
    return { ...task };
  }

  /** Another process adds to a task's history and status, silently (test helper). */
  updateOutside(id: string, events: AgentEvent[], status?: Task["status"]): void {
    this.eventsById.get(id)?.push(...events);
    const task = this.tasksById.get(id);
    if (task && status) task.status = status;
  }

  // ------------------------------------------------ carrying a conversation
  // Like the backend: questions with their final answers, a delimited context
  // that opens the task's prompt, a summary by the conversation's own account.

  /** What "Resumir con IA" answers next: a summary or an error (test helper). */
  summary: string | { error: string } =
    "- OAuth 2.0 autoriza el acceso de una app a tus datos.\n- OpenID Connect va encima y autentica con un ID token.";

  private conversationTask(id: string): Task {
    const task = this.tasksById.get(id);
    if (!task) throw `no existe la tarea ${id}`;
    if (!task.conversation) throw `la tarea ${id} es de un proyecto, no una conversación`;
    return task;
  }

  private turns(id: string): ContextTurn[] {
    const turns: ContextTurn[] = [];
    let open: { question: string; images: Attachment[]; last: string | null; answer: string | null } | null = null;
    const close = () => {
      const answer = (open?.answer ?? open?.last ?? "").trim();
      if (open && answer && open.question.trim()) turns.push({ question: open.question.trim(), answer, images: open.images });
      open = null;
    };
    for (const event of this.eventsById.get(id) ?? []) {
      if (event.kind === "userMessage") {
        close();
        open = { question: event.text, images: event.images ?? [], last: null, answer: null };
      } else if (event.kind === "message" && open && event.text.trim()) {
        open.last = event.text;
      } else if (event.kind === "finished" && open && event.ok && event.summary.trim()) {
        open.answer = event.summary;
      }
    }
    close();
    return turns;
  }

  private contextSection(title: string, turns: ContextTurn[], summary: string | null, attached: boolean): string {
    const en = this.english();
    const lines = [
      en ? `--- Context: conversation «${title}» ---` : `--- Contexto: conversación «${title}» ---`,
      en
        ? "It comes from a quick question asked in Hedy outside this project. Use it as context; the instruction follows this block."
        : "Viene de una pregunta rápida hecha en Hedy fuera de este proyecto. Úsala como contexto; la instrucción va después de este bloque.",
      "",
    ];
    if (summary) {
      lines.push(en ? "Summary of the conversation (made with AI):" : "Resumen de la conversación (hecho con IA):", summary, "");
    } else {
      turns.forEach((turn, i) => {
        lines.push(en ? `Question ${i + 1}:` : `Pregunta ${i + 1}:`, turn.question);
        if (turn.images.length) {
          const names = turn.images.map((image) => image.name).join(", ");
          lines.push(en ? `(Images: ${names})` : `(Imágenes: ${names})`);
        }
        lines.push("", en ? `Answer ${i + 1}:` : `Respuesta ${i + 1}:`, turn.answer, "");
      });
    }
    if (turns.some((turn) => turn.images.length)) {
      lines.push(
        attached
          ? en
            ? "The conversation's images are attached to this message."
            : "Las imágenes de la conversación van adjuntas a este mensaje."
          : en
            ? "The conversation's images are not attached: they are only named."
            : "Las imágenes de la conversación no van adjuntas: solo se nombran.",
        "",
      );
    }
    lines.push(en ? "--- End of context ---" : "--- Fin del contexto ---");
    return lines.join("\n");
  }

  private async carried(carry: CarryInput, account: string, ownImages: number) {
    const conversation = this.conversationTask(carry.conversation);
    if (this.busy(conversation)) throw "la conversación todavía está respondiendo; espera a que termine para llevarla a un proyecto";
    const kept = this.turns(conversation.id).filter((_, i) => carry.turns.includes(i));
    const summary = carry.summary?.trim() || null;
    if (kept.length === 0 && !summary) throw "no queda nada de la conversación para llevar";
    const wanted = kept.flatMap((turn) => turn.images);
    const attach = wanted.length > 0 && ownImages + wanted.length <= MAX_IMAGES && (await this.imageSupport(account)) !== "no";
    const origin: TaskOrigin = {
      conversation: conversation.id,
      title: conversation.title,
      context: this.contextSection(conversation.title, kept, summary, attach),
    };
    return { origin, images: attach ? wanted : [] };
  }

  private copyAttachment(from: string, to: string, image: Attachment): Attachment {
    const data = this.attachments.get(`${from}/${image.file}`);
    if (data) this.attachments.set(`${to}/${image.file}`, data);
    return { ...image };
  }

  async conversationContext(id: string): Promise<ConversationContext> {
    const task = this.conversationTask(id);
    const slug = task.title
      .normalize("NFD")
      .replace(/[\u0300-\u036f]/g, "")
      .toLowerCase()
      .split(/[^a-z0-9_]+/)
      .filter(Boolean);
    const stop = new Set(["a", "al", "an", "and", "como", "con", "cual", "de", "del", "do", "el", "en", "entre", "es", "hay", "how", "i", "is", "la", "las", "los", "me", "of", "para", "por", "que", "the", "to", "un", "una", "what", "y"]);
    const words = slug.filter((w) => !stop.has(w)).slice(0, 4);
    return {
      conversation: task.id,
      title: task.title,
      turns: this.turns(id),
      suggestedName: (words.length ? words : slug.slice(0, 4)).join("-") || (this.english() ? "project" : "proyecto"),
      longAfter: 12_000,
      busy: this.busy(task),
    };
  }

  async summarizeConversation(id: string, turns: number[]): Promise<string> {
    const task = this.conversationTask(id);
    if (this.busy(task)) throw "la conversación todavía está respondiendo";
    if (!this.turns(id).some((_, i) => turns.includes(i))) throw "no queda nada de la conversación para llevar";
    await this.pause();
    if (typeof this.summary !== "string") throw this.summary.error;
    return this.summary;
  }

  async continuations(id: string): Promise<Task[]> {
    return [...this.tasksById.values()]
      .filter((t) => t.origin?.conversation === id)
      .sort((a, b) => b.createdAt - a.createdAt)
      .map((t) => ({ ...t }));
  }

  async saveConversationNote(
    conversation: string,
    project: string,
    title: string,
    text: string,
    category: NoteCategory,
  ): Promise<Note> {
    this.conversationTask(conversation);
    this.project(project);
    const body = [title.trim(), text.trim()].filter(Boolean).join("\n\n");
    return this.addNote(project, body, category);
  }

  async conversationAccounts(): Promise<string[]> {
    return this.offersFor("");
  }

  async deleteConversation(id: string): Promise<void> {
    const task = this.tasksById.get(id);
    if (!task?.conversation) throw `la tarea ${id} es de un proyecto, no una conversación`;
    task.live = false;
    this.tasksById.delete(id);
    this.eventsById.delete(id);
    this.queues.delete(id);
    this.updatedAt.delete(id);
  }

  private implementation(): AgentEvent[] {
    const steps = (done: number): AgentEvent => ({
      kind: "steps",
      steps: ["Buscar dónde se crean las citas", "Tarea programada y plantilla del mensaje", "Correr los tests"].map(
        (text, i) => ({ text, status: i < done ? "done" : i === done ? "inProgress" : "pending" }),
      ),
    });
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
    this.updatedAt.set(id, Math.floor(Date.now() / 1000));
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
      if (event.kind === "phaseStarted") {
        task.stage = Math.max(0, task.stages.findIndex((s) => s.phase === event.phase));
      }
      if (status !== task.status) {
        task.status = status;
        this.notifyTask(task);
      }
    }
    for (const listener of this.eventListeners) listener(id, event);
    if (event.kind === "finished") {
      if (event.ok) this.deliverQueue(id);
      else this.holdQueue(id, "failed");
    }
    if (event.kind === "limitWarning" && event.level === "reached") this.holdQueue(id, "limit");
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

  async activity(): Promise<ActivityItem[]> {
    const since = Date.now() / 1000 - 24 * 3600;
    const items = [...this.tasksById.values()].map((task): ActivityItem => {
      const events = this.eventsById.get(task.id) ?? [];
      const started = [...events].reverse().find((e) => e.kind === "phaseStarted");
      const project = this.state.projects.find((p) => p.path === task.project);
      const account = this.state.accounts.find((a) => a.id === task.account);
      return {
        task: { ...task },
        state: taskState(task, undefined, events),
        phase: task.stages[task.stage]?.phase ?? "implement",
        agent:
          started?.kind === "phaseStarted" ? started.agent : (account?.providerName.split(" ")[0] ?? task.account),
        projectName: task.conversation
          ? this.english()
            ? "Conversation"
            : "Conversación"
          : (project?.name ?? task.project.split("/").pop() ?? task.project),
        updatedAt: this.updatedAt.get(task.id) ?? task.createdAt,
        queued: this.queues.get(task.id)?.length ?? 0,
      };
    });
    let finished = 0;
    return items
      .filter((item) => ACTIVE.includes(item.state) || item.updatedAt >= since)
      .sort((a, b) => BOARD_RANK[a.state] - BOARD_RANK[b.state] || b.updatedAt - a.updatedAt)
      .filter((item) => ACTIVE.includes(item.state) || ++finished <= 30);
  }

  /**
   * A task of tienda following «Pago con revisión», at its approval: the
   * tests failed once and passed, the review approved, the board is full.
   */
  withSampleFlowRun(): this {
    const now = Math.floor(Date.now() / 1000);
    const flow = mockProjectFlow();
    const id = "f1";
    const roles = Object.fromEntries(MOCK_ROLES.map((r) => [r.id, r]));
    this.tasksById.set(id, {
      id,
      project: "/code/tienda",
      mode: "project",
      title: "Agrega el pago con tarjeta en la caja",
      prompt: "Agrega el pago con tarjeta en la caja",
      account: "claude-trabajo",
      model: "sonnet",
      workspace: { worktree: "/code/tienda/.hedy/worktrees/f1", branch: "hedy/pago-tarjeta-f1", baseBranch: "main", baseCommit: "abc1234" },
      sessionId: "s-f1",
      status: "waitingApproval",
      createdAt: now - 25 * 60,
      stages: [
        { phase: "review", account: "claude-trabajo", model: "opus" },
        { phase: "implement", account: "claude-trabajo", model: "sonnet" },
        { phase: "review", account: "claude-trabajo", model: null },
      ],
      stage: 2,
      live: true,
      flow: {
        flow,
        roles,
        position: { node: "n7", rounds: { n4: 1 } },
        spent: {
          n2: { inputTokens: 20, outputTokens: 1500, cacheReadTokens: 40_000, cacheWriteTokens: 8000, secs: 45 },
          n3: { inputTokens: 60, outputTokens: 6000, cacheReadTokens: 130_000, cacheWriteTokens: 12_000, secs: 160 },
          n5: { inputTokens: 10, outputTokens: 900, cacheReadTokens: 26_000, cacheWriteTokens: 3000, secs: 50 },
        },
      },
    });
    const phase = (p: "review" | "implement", agent: string): AgentEvent => ({ kind: "phaseStarted", phase: p, agent, account: "Claude trabajo" });
    this.eventsById.set(id, [
      { kind: "userMessage", text: "Agrega el pago con tarjeta en la caja" },
      phase("review", "Claude Opus"),
      { kind: "message", text: "Plan: 1. Cliente de pagos en `src/pagos/`. 2. Botón en la caja. 3. Tests del cobro." },
      { kind: "finished", ok: true, summary: "", durationMs: 41000 },
      phase("implement", "Claude Sonnet"),
      { kind: "fileEdited", path: "src/pagos/cliente.ts" },
      { kind: "fileEdited", path: "src/caja/Cobro.tsx" },
      { kind: "finished", ok: true, summary: "", durationMs: 96000 },
      { kind: "flowCheck", node: "n4", what: "npm test", passed: false },
      phase("implement", "Claude Sonnet"),
      { kind: "fileEdited", path: "src/pagos/cliente.ts" },
      { kind: "finished", ok: true, summary: "", durationMs: 30000 },
      { kind: "flowCheck", node: "n4", what: "npm test", passed: true },
      phase("review", "Claude Sonnet"),
      { kind: "message", text: "Los cambios siguen el plan y los tests cubren el cobro rechazado.\n\nVEREDICTO: APRUEBA" },
      { kind: "finished", ok: true, summary: "", durationMs: 28000 },
      { kind: "flowCheck", node: "n6", what: "el veredicto de la revisión", passed: true },
      { kind: "flowApproval", node: "n7" },
    ]);
    this.boards.set(id, [
      { section: "frame", role: "", round: 1, text: "Objetivo: pasarela de pagos. Reglas: nada de claves en el código." },
      { section: "architect", role: "Arquitecto", round: 1, text: "1. Cliente de pagos en src/pagos/.\n2. Botón «Pagar con tarjeta» en la caja.\n3. Tests del cobro aceptado y rechazado." },
      { section: "programmer", role: "Programador", round: 2, text: "Cliente y botón listos; arreglé el test del cobro rechazado." },
      { section: "tests", role: "Hedy", round: 2, text: "npm test: 18 pasan." },
      { section: "reviewer", role: "Revisor", round: 2, text: "Los cambios siguen el plan.\n\nVEREDICTO: APRUEBA" },
    ]);
    this.diffs.set(id, []);
    this.updatedAt.set(id, now - 60);
    return this;
  }

  /** The sample flow task after its review loop ran out of rounds: in review, with the notice. */
  withSampleFlowOutOfRounds(): this {
    this.withSampleFlowRun();
    const task = this.tasksById.get("f1");
    if (!task) return this;
    this.tasksById.set("f1", { ...task, status: "review", live: false, flow: task.flow && { ...task.flow, position: { node: "n6", rounds: { n6: 2 } } } });
    this.eventsById.set("f1", [
      ...(this.eventsById.get("f1") ?? []),
      { kind: "flowOutOfRounds", node: "n6", rounds: 2, asked: "Falta validar la firma del webhook.\n\nVEREDICTO: CAMBIOS" },
    ]);
    return this;
  }

  /**
   * Sample tasks for the task board, in both projects: two waiting for an
   * answer, one at its limit, one working and a few that already finished.
   */
  withSampleActivity(): this {
    const now = Math.floor(Date.now() / 1000);
    const min = 60;
    const add = (
      id: string,
      project: string,
      account: string,
      title: string,
      extra: Partial<Task>,
      ago: { created: number; updated: number },
      events: AgentEvent[],
    ) => {
      const phase = extra.stages?.[extra.stage ?? 0]?.phase ?? "implement";
      this.tasksById.set(id, {
        id,
        project,
        mode: "project",
        title,
        prompt: title,
        account,
        model: "opus",
        workspace: null,
        sessionId: `s-${id}`,
        status: "running",
        createdAt: now - ago.created,
        stages: [{ phase, account, model: "opus" }],
        stage: 0,
        live: false,
        ...extra,
      });
      this.eventsById.set(id, [{ kind: "userMessage", text: title }, ...events]);
      this.diffs.set(id, []);
      this.updatedAt.set(id, now - ago.updated);
    };
    const opus = (account: string, phase: "plan" | "implement" = "implement"): AgentEvent => ({
      kind: "phaseStarted",
      phase,
      agent: "Claude Opus",
      account,
    });
    add(
      "s1",
      "/code/hedy",
      "claude-personal",
      "Agrega recordatorios por WhatsApp antes de cada cita",
      { status: "waitingApproval", live: true, stages: [{ phase: "plan", account: "claude-personal", model: "opus" }] },
      { created: 6 * min, updated: 1 * min },
      [opus("Claude personal", "plan"), { kind: "plan", text: "1. Buscar\n2. Programar\n3. Tests", requestId: "plan-s1" }],
    );
    add(
      "s2",
      "/code/tienda",
      "claude-trabajo",
      "Migra el reporte de comisiones a la API nueva",
      { status: "waitingApproval", live: true },
      { created: 14 * min, updated: 2 * min },
      [
        opus("Claude trabajo"),
        { kind: "permissionRequest", requestId: "req-s2", tool: "Bash", summary: "npm test", input: { command: "npm test" } },
      ],
    );
    // At its limit; the user chose to wait: it resumes by itself at the reset.
    const resetsAt = now + 47 * min;
    add(
      "s3",
      "/code/hedy",
      "claude-personal",
      "Traduce los correos de confirmación",
      {
        live: true,
        wait: { account: "claude-personal", until: resetsAt, since: now - 4 * min, ready: false },
      },
      { created: 38 * min, updated: 5 * min },
      [
        opus("Claude personal"),
        { kind: "limitWarning", account: "claude-personal", level: "reached", percent: 100, resetsAt },
      ],
    );
    add(
      "s4",
      "/code/tienda",
      "claude-trabajo",
      "Revisa los tests del carrito",
      { live: true },
      { created: 3 * min, updated: 0 },
      [opus("Claude trabajo"), { kind: "toolUsed", tool: "Grep", summary: "carrito" }],
    );
    // Written while the agents work: two wait for the end of the turn, one
    // is on hold because the account reached its limit.
    const queued = (task: string, text: string, held: HoldReason | null, ago: number) =>
      this.queues.set(task, [
        ...(this.queues.get(task) ?? []),
        { id: ++this.nextQueued, task, text, createdAt: now - ago, held },
      ]);
    queued("s4", "Cuando termines, corre también los tests del checkout.", null, 60);
    queued("s4", "Y deja un resumen corto de lo que cambiaste.", null, 30);
    queued("s3", "Revisa también el asunto del correo.", "limit", 4 * min);
    add(
      "s5",
      "/code/hedy",
      "claude-personal",
      "Arregla el cálculo de impuestos en las facturas",
      {
        status: "review",
        workspace: {
          worktree: "/code/hedy/.hedy/worktrees/s5",
          branch: "hedy/arregla-el-calculo-de-impuestos-s5",
          baseBranch: "main",
          baseCommit: "a3f9c1e",
        },
      },
      { created: 70 * min, updated: 40 * min },
      [
        opus("Claude personal"),
        { kind: "fileEdited", path: "facturas/impuestos.py" },
        { kind: "testsRun", command: "pytest tests/test_facturas.py", passed: false },
        { kind: "finished", ok: true, summary: "El IVA se redondea por línea y no al total.", durationMs: 1_200_000 },
      ],
    );
    // The same changes the code views read from the sample repository.
    this.diffs.set("s5", this.code.taskDiff("/code/hedy", "s5"));
    // A question that left a document and a page, outside the project.
    this.outputs.set("c1", [
      { path: "oauth-vs-oidc.md", bytes: 4_812, opens: true },
      { path: "diagrama/index.html", bytes: 12_340, opens: true },
      { path: "instalar.sh", bytes: 640, opens: false },
    ]);
    // A question outside any project, already answered.
    add(
      "c1",
      `${this.state.dataDir}/chats/c1`,
      "claude-personal",
      "¿Qué diferencia hay entre OAuth y OpenID Connect?",
      { status: "review", mode: "ask", workspace: null, conversation: true },
      { created: 50 * min, updated: 48 * min },
      [
        opus("Claude personal"),
        {
          kind: "message",
          text: "OAuth 2.0 autoriza: da a una app acceso a algo tuyo. OpenID Connect va encima y autentica: dice quién eres, con un ID token.",
        },
        { kind: "finished", ok: true, summary: "", durationMs: 9000 },
      ],
    );
    add(
      "s6",
      "/code/hedy",
      "claude-personal",
      "¿Dónde se calculan las comisiones?",
      { status: "review", mode: "ask", stages: [] },
      { created: 130 * min, updated: 125 * min },
      [{ kind: "finished", ok: true, summary: "", durationMs: 30_000 }],
    );
    add(
      "s7",
      "/code/tienda",
      "claude-trabajo",
      "Actualiza las dependencias del panel",
      { status: "failed" },
      { created: 320 * min, updated: 300 * min },
      [opus("Claude trabajo"), { kind: "error", message: "npm install falló" }],
    );
    add(
      "s8",
      "/code/hedy",
      "claude-personal",
      "Cambia el color del botón de reservar",
      { status: "discarded" },
      { created: 20 * 60 * min, updated: 19 * 60 * min },
      [opus("Claude personal"), { kind: "finished", ok: true, summary: "Listo", durationMs: 60_000 }],
    );
    return this;
  }

  /** Test helper: the user clicks the desktop notification of a task. */
  simulateNotificationClick(id: string): void {
    const task = this.tasksById.get(id);
    if (!task) return;
    for (const listener of this.openTaskListeners) listener({ taskId: id, project: task.project });
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
        ? [{ kind: "phaseStarted", phase: "implement", agent: "Claude Opus", account: "Claude personal" }, ...this.implementation()]
        : [{ kind: "plan", text: `1. Buscar\n2. Cambiar (${feedback ?? "revisado"})\n3. Tests`, requestId: `${requestId}-b` }],
    );
  }

  async sendMessage(id: string, text: string, images?: ImageInput[]): Promise<void> {
    const task = this.tasksById.get(id);
    if (!task || task.status === "accepted" || task.status === "discarded") throw "la tarea ya está cerrada";
    const attached = await this.storeImages(id, task.account, images);
    // A flow at its approval answers and goes on waiting, like the backend.
    if (task.flow && task.status === "waitingApproval") {
      this.push(id, { kind: "userMessage", text });
      this.push(id, { kind: "message", text: "Porque ahí viven los demás clientes externos." });
      task.status = "waitingApproval";
      this.notifyTask(task);
      return;
    }
    task.live = true;
    // Writing to the agent ends a wait for its limit, like the backend.
    if (task.wait) {
      clearTimeout(this.waitTimers.get(id));
      task.wait = null;
      this.notifyTask(task);
    }
    this.push(id, attached.length ? { kind: "userMessage", text, images: attached } : { kind: "userMessage", text });
    this.script(id, [{ kind: "message", text: `Hecho: ${text}` }, { kind: "finished", ok: true, summary: "", durationMs: 1000 }]);
  }

  async stopTask(id: string): Promise<void> {
    const task = this.tasksById.get(id);
    if (!task) return;
    task.live = false;
    this.holdQueue(id, "stopped");
    // Like the backend: `task-updated` only when the status changes.
    if (task.status === "running" || task.status === "waitingApproval") {
      task.status = "review";
      this.notifyTask(task);
    }
  }

  async taskDiff(id: string): Promise<FileDiff[]> {
    return [...(this.diffs.get(id) ?? [])];
  }

  async taskOutputs(id: string): Promise<OutputFile[]> {
    if (!this.tasksById.has(id)) throw `no existe la tarea ${id}`;
    return [...(this.outputs.get(id) ?? [])];
  }

  async openOutput(id: string, path: string): Promise<void> {
    const file = (this.outputs.get(id) ?? []).find((f) => f.path === path);
    if (!file) throw `«${path}» no es un archivo que escribió la tarea`;
    if (!file.opens) throw `Hedy no abre «${path}»: es un tipo de archivo que podría ejecutar algo.`;
  }

  async saveOutput(id: string, path: string, folder: string): Promise<string> {
    if (!(this.outputs.get(id) ?? []).some((f) => f.path === path)) throw `«${path}» no es un archivo que escribió la tarea`;
    return `${folder}/${path.split("/").pop()}`;
  }

  async acceptPreview(id: string): Promise<AcceptPreview> {
    const task = this.tasksById.get(id);
    if (!task) throw `no existe la tarea ${id}`;
    if (task.status === "accepted" || task.status === "discarded") throw `la tarea ${id} ya está cerrada`;
    if (!task.workspace) throw `la tarea ${id} no cambia archivos (es una pregunta)`;
    const files = this.diffs.get(id) ?? [];
    const events = this.eventsById.get(id) ?? [];
    const tests = [...events].reverse().find((e) => e.kind === "testsRun");
    return {
      branch: task.workspace.branch,
      targetBranch: task.workspace.baseBranch,
      checkoutBranch: task.workspace.baseBranch,
      files: files.length,
      additions: files.reduce((n, f) => n + f.additions, 0),
      deletions: files.reduce((n, f) => n + f.deletions, 0),
      message: this.proposedMessage(task),
      tests:
        tests?.kind === "testsRun"
          ? { command: tests.command, passed: tests.passed, at: this.updatedAt.get(id) ?? task.createdAt }
          : null,
    };
  }

  /** Like the backend: the title and the first paragraph of the agent's summary. */
  private proposedMessage(task: Task): string {
    const finished = [...(this.eventsById.get(task.id) ?? [])]
      .reverse()
      .find((e) => e.kind === "finished" && e.ok);
    const summary = finished?.kind === "finished" ? finished.summary : "";
    const paragraph = summary.split("\n\n").map((p) => p.trim()).find((p) => p) ?? "";
    return paragraph && paragraph !== task.title ? `${task.title}\n\n${paragraph}` : task.title;
  }

  /** Commit messages of accepted tasks (test helper). */
  readonly commitMessages = new Map<string, string>();
  private acceptedDiffs = new Map<string, FileDiff[]>();
  /** When set, undo is refused with this message, as when the branch moved on. */
  undoRefusal: string | null = null;

  async acceptTask(id: string, message?: string | null, mode: AcceptMode = "merge"): Promise<void> {
    const task = this.tasksById.get(id);
    if (!task) throw `no existe la tarea ${id}`;
    if (!task.workspace) throw `la tarea ${id} no cambia archivos (es una pregunta)`;
    const used = message?.trim() || this.proposedMessage(task);
    this.commitMessages.set(id, used);
    this.acceptModes.set(id, mode);
    const files = this.diffs.get(id) ?? [];
    this.acceptedDiffs.set(id, files);
    const kept = mode === "branch";
    const previous = task.ship?.kept && kept ? task.ship : null;
    task.ship = {
      branch: kept ? task.workspace.branch : task.workspace.baseBranch,
      kept,
      message: used,
      files: files.map((f) => ({ path: f.path, additions: f.additions, deletions: f.deletions })),
      pushedTo: previous?.pushedTo ?? null,
      pullRequest: previous?.pullRequest ?? null,
    };
    task.merge = kept ? null : { before: "a3f9c1e", after: "5d0b7e2", tip: "9c4e1aa", at: Math.floor(Date.now() / 1000) };
    // A merge is one more commit on the folder's branch, not published yet.
    if (!kept) this.folder(task.project).ahead += 1;
    task.wait = null;
    this.close(id, "accepted");
  }

  async undoAccept(id: string): Promise<void> {
    const task = this.tasksById.get(id);
    if (!task) throw `no existe la tarea ${id}`;
    const english = this.english();
    if (task.status === "accepted" && task.ship?.kept) {
      task.status = "review";
      this.diffs.set(id, this.acceptedDiffs.get(id) ?? []);
      this.notifyTask(task);
      return;
    }
    if (task.status !== "accepted" || !task.merge) {
      throw english
        ? "It cannot be undone safely: the task is not merged. To take the changes out, use git revert."
        : "No se puede deshacer sin riesgo: la tarea no está integrada. Si quieres quitar los cambios, usa git revert.";
    }
    if (this.undoRefusal) throw this.undoRefusal;
    if (task.ship?.pushedTo) {
      throw english
        ? `It cannot be undone safely: the changes were already pushed to ${task.ship.pushedTo}. To take the changes out, use git revert.`
        : `No se puede deshacer sin riesgo: los cambios ya se subieron a ${task.ship.pushedTo}. Si quieres quitar los cambios, usa git revert.`;
    }
    task.merge = null;
    task.ship = null;
    task.status = "review";
    const folder = this.folder(task.project);
    folder.ahead = Math.max(0, folder.ahead - 1);
    this.diffs.set(id, this.acceptedDiffs.get(id) ?? []);
    this.notifyTask(task);
  }

  // ---------------------------------------------------------------- shipping
  // The folder's git state and its remote, like the backend: push sets an
  // upstream when missing, pull only fast-forwards, failures come back as data.

  /** How each accepted task was accepted (test helper). */
  readonly acceptModes = new Map<string, AcceptMode>();
  private gitFolders = new Map<string, Omit<FolderStatus, "changes" | "remote" | "remoteHost" | "gh">>();
  /** Folders without a remote (test helper). */
  readonly noRemote = new Set<string>();
  /** What `gh` can do for every folder's remote. */
  gh: GhState = "ready";
  /** The next push fails like this. */
  pushFailure: ShipFailure | null = null;
  /** The next pull fails like this. */
  pullFailure: ShipFailure | null = null;
  /** The next `createPullRequest` fails with this message. */
  prFailure: string | null = null;
  /** What "Sugerir con IA" answers; `{ error }` makes it fail. */
  suggestion: string | { error: string } = "fix(facturas): round VAT per line, not on the total";
  /** Folder pushes and pulls, and task pushes, in order (test helper). */
  readonly shipLog: string[] = [];
  readonly pullRequests: { task: string; input: NewPullRequestInput }[] = [];

  private folder(path: string) {
    let folder = this.gitFolders.get(path);
    if (!folder) {
      folder = { branch: "main", upstream: "origin/main", ahead: 0, behind: 2, staged: 1, unstaged: 1, untracked: 1, conflicted: 0 };
      this.gitFolders.set(path, folder);
    }
    return folder;
  }

  async folderStatus(path: string): Promise<FolderStatus> {
    const folder = this.folder(path);
    const remote = !this.noRemote.has(path);
    return {
      ...folder,
      upstream: remote ? folder.upstream : null,
      changes: folder.staged + folder.unstaged + folder.untracked + folder.conflicted,
      remote: remote ? "origin" : null,
      remoteHost: remote ? "github.com" : null,
      gh: remote ? this.gh : null,
    };
  }

  private noRemoteFailure(): ShipFailure {
    return {
      kind: "noRemote",
      message: this.english()
        ? "this folder has no remote to publish to: add one with `git remote add origin <url>`"
        : "esta carpeta no tiene un remoto al que publicar: añade uno con `git remote add origin <url>`",
    };
  }

  private async pushed(path: string, branch: string, upstream: string | null): Promise<PushOutcome> {
    await this.pause();
    if (this.noRemote.has(path)) return { pushed: null, failure: this.noRemoteFailure() };
    const failure = this.pushFailure;
    this.pushFailure = null;
    if (failure) return { pushed: null, failure };
    return { pushed: { branch, upstream: upstream ?? `origin/${branch}`, upstreamSet: !upstream }, failure: null };
  }

  async pushFolder(path: string): Promise<PushOutcome> {
    const folder = this.folder(path);
    this.shipLog.push(`push ${path}`);
    const outcome = await this.pushed(path, folder.branch ?? "main", folder.upstream);
    if (outcome.pushed) {
      folder.ahead = 0;
      folder.upstream = outcome.pushed.upstream;
    }
    return outcome;
  }

  async pullFolder(path: string): Promise<PullOutcome> {
    const folder = this.folder(path);
    this.shipLog.push(`pull ${path}`);
    await this.pause();
    const failure = this.pullFailure;
    this.pullFailure = null;
    if (failure) return { updated: null, failure };
    const updated = folder.behind > 0;
    folder.behind = 0;
    return { updated, failure: null };
  }

  private shipped(taskId: string): { task: Task; ship: Shipment } {
    const task = this.tasksById.get(taskId);
    if (!task?.ship || task.status !== "accepted") {
      throw this.english() ? "accept the task before publishing it" : "acepta la tarea antes de publicarla";
    }
    return { task, ship: task.ship };
  }

  async shipState(taskId: string): Promise<ShipState> {
    const { task, ship } = this.shipped(taskId);
    const remote = !this.noRemote.has(task.project);
    const folder = this.folder(task.project);
    const upstream = ship.kept ? ship.pushedTo : remote ? folder.upstream : null;
    const ahead = ship.kept ? (ship.pushedTo ? 0 : 1) : folder.ahead;
    const gh = remote ? this.gh : null;
    return {
      branch: ship.branch,
      kept: ship.kept,
      remote: remote ? "origin" : null,
      remoteHost: remote ? "github.com" : null,
      upstream,
      ahead,
      behind: ship.kept ? 0 : folder.behind,
      pushed: !!upstream && ahead === 0,
      gh,
      defaultBranch: remote ? "main" : null,
      pullRequest: ship.pullRequest,
      canOpenPr: gh === "ready" && ship.branch !== "main",
    };
  }

  async pushTask(taskId: string): Promise<PushOutcome> {
    const { task, ship } = this.shipped(taskId);
    this.shipLog.push(`push task ${taskId}`);
    const state = await this.shipState(taskId);
    const outcome = await this.pushed(task.project, ship.branch, state.upstream);
    if (outcome.pushed) {
      ship.pushedTo = outcome.pushed.upstream;
      if (!ship.kept) {
        const folder = this.folder(task.project);
        folder.ahead = 0;
        folder.upstream = outcome.pushed.upstream;
      }
      this.notifyTask(task);
    }
    return outcome;
  }

  async pullRequestDraft(taskId: string): Promise<PullRequestDraft> {
    const { task, ship } = this.shipped(taskId);
    const english = this.english();
    const events = this.eventsById.get(taskId) ?? [];
    const finished = [...events].reverse().find((e) => e.kind === "finished" && e.ok);
    const summary = finished?.kind === "finished" ? finished.summary.trim() : "";
    const tests = new Map<string, boolean>();
    for (const e of events) if (e.kind === "testsRun") tests.set(e.command, e.passed);
    const lines = [
      english ? "## What was asked" : "## Qué se pidió",
      "",
      ...task.prompt.split("\n").map((l) => `> ${l}`),
      ...(summary ? ["", english ? "## Summary" : "## Resumen", "", summary] : []),
      "",
      english ? `## Files changed (${ship.files.length})` : `## Archivos cambiados (${ship.files.length})`,
      "",
      ...ship.files.map((f) => `- \`${f.path}\` (+${f.additions} −${f.deletions})`),
      "",
      "## Tests",
      "",
      ...(tests.size === 0
        ? [english ? "No tests were run in this task." : "No se ejecutaron tests en esta tarea."]
        : [...tests].map(([command, passed]) =>
            `- ${passed ? "✅" : "❌"} \`${command}\`: ${passed ? (english ? "passed" : "pasaron") : english ? "failed" : "fallaron"}`,
          )),
    ];
    return { title: ship.message.split("\n")[0]?.trim() ?? task.title, body: `${lines.join("\n")}\n`, base: "main", head: ship.branch };
  }

  async createPullRequest(taskId: string, input: NewPullRequestInput): Promise<PullRequest> {
    const { task, ship } = this.shipped(taskId);
    await this.pause();
    const failure = this.prFailure;
    this.prFailure = null;
    if (failure) throw failure;
    this.pullRequests.push({ task: taskId, input });
    const pr: PullRequest = { url: "https://github.com/alex/hedy/pull/42", number: 42, draft: input.draft };
    ship.pullRequest = pr;
    this.notifyTask(task);
    return pr;
  }

  async suggestCommitMessage(taskId: string): Promise<string> {
    if (!this.tasksById.has(taskId)) throw `no existe la tarea ${taskId}`;
    await this.pause();
    if (typeof this.suggestion !== "string") throw this.suggestion.error;
    return this.suggestion;
  }

  /** A short wait so progress shows in the browser (none in tests). */
  private pause(): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, this.delayMs > 0 ? 600 : 0));
  }

  // ---------------------------------------------------------------- limit waits
  // Like the backend: the wait is stored in the task; at the reset the task
  // resumes on the same account with its held queue or a "continue" turn.

  private waitTimers = new Map<string, ReturnType<typeof setTimeout>>();

  async waitForLimit(id: string): Promise<void> {
    const task = this.tasksById.get(id);
    if (!task) throw `no existe la tarea ${id}`;
    if (task.status === "accepted" || task.status === "discarded") throw `la tarea ${id} ya está cerrada`;
    const now = Math.floor(Date.now() / 1000);
    const reported = [...(this.eventsById.get(id) ?? [])]
      .reverse()
      .find((e) => e.kind === "limitWarning" && e.level === "reached");
    const resetsAt = reported?.kind === "limitWarning" ? reported.resetsAt : null;
    const until = resetsAt && resetsAt > now ? resetsAt : null;
    task.wait = { account: task.account, until, since: now, ready: false };
    this.notifyTask(task);
    this.scheduleResume(id);
  }

  private scheduleResume(id: string): void {
    clearTimeout(this.waitTimers.get(id));
    const until = this.tasksById.get(id)?.wait?.until;
    if (!until) return;
    this.waitTimers.set(
      id,
      setTimeout(() => void this.resumeDue(id), Math.max(0, until * 1000 - Date.now())),
    );
  }

  /** The reset time came (or a test says so): resume on the same account. */
  async resumeDue(id: string): Promise<void> {
    const task = this.tasksById.get(id);
    if (!task?.wait || task.wait.ready) return;
    await this.resumeAfterLimit(id);
  }

  /**
   * Test helper: Hedy was closed when the limit renewed. Like the backend at
   * start-up, the wait becomes "Ya puedes continuar" and nothing is sent.
   */
  simulateRestartAfterReset(id: string): void {
    const task = this.tasksById.get(id);
    if (!task?.wait) return;
    clearTimeout(this.waitTimers.get(id));
    task.wait = { ...task.wait, until: Math.floor(Date.now() / 1000) - 60, ready: true };
    this.notifyTask(task);
  }

  async cancelWait(id: string): Promise<void> {
    const task = this.tasksById.get(id);
    if (!task) throw `no existe la tarea ${id}`;
    clearTimeout(this.waitTimers.get(id));
    task.wait = null;
    this.notifyTask(task);
  }

  async resumeAfterLimit(id: string): Promise<void> {
    const task = this.tasksById.get(id);
    if (!task) throw `no existe la tarea ${id}`;
    if (task.status === "accepted" || task.status === "discarded") throw `la tarea ${id} ya está cerrada`;
    clearTimeout(this.waitTimers.get(id));
    task.wait = null;
    const queue = this.queues.get(id) ?? [];
    this.queues.delete(id);
    if (queue.length) this.emitQueue(id);
    const english = resolveLanguage(this.state.language) === "en";
    const text = queue.length
      ? queue.map((m) => m.text.trim()).join("\n\n")
      : english
        ? "Your limit renewed. Continue the task where you left off."
        : "Tu límite se renovó. Continúa con la tarea donde la dejaste.";
    this.notifyTask(task);
    await this.sendMessage(id, text);
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
    if (this.queues.get(id)?.length) {
      this.queues.delete(id);
      this.emitQueue(id);
    }
    this.notifyTask(task);
  }

  /**
   * For `npm run dev`: the running sample task ("s4", in tienda) goes through
   * every kind of tool, then stays silent a while, so its mascot shows each
   * working pose. A few rounds, every `stepMs`.
   */
  withLiveTools(stepMs = 4000, rounds = 6): this {
    const round: (AgentEvent | null)[] = [
      { kind: "message", text: "Voy a revisar cómo se calcula el envío del carrito." },
      { kind: "toolUsed", tool: "Read", summary: "src/carrito/total.ts" },
      { kind: "toolUsed", tool: "Grep", summary: "calcularEnvio" },
      { kind: "toolUsed", tool: "Task", summary: "Revisar los casos de borde del envío" },
      { kind: "fileEdited", path: "src/carrito/total.ts" },
      { kind: "commandRun", command: "npm run build", tests: false },
      { kind: "testsRun", command: "npm test -- carrito", passed: true },
      { kind: "toolUsed", tool: "WebFetch", summary: "docs de la librería de pagos" },
      // Silence: after 20 s the mascot waits.
      ...Array.from({ length: Math.ceil(24_000 / stepMs) }, () => null),
    ];
    const events = Array.from({ length: rounds }, () => round).flat();
    let step = 0;
    const timer = setInterval(() => {
      const event = events[step++];
      if (step > events.length || !this.tasksById.get("s4")?.live) {
        clearInterval(timer);
        return;
      }
      if (event) this.push("s4", event);
    }, stepMs);
    return this;
  }

  /** Test helper: the agent reports its account is close to the limit. */
  simulateLimit(id: string, level: "warning" | "reached" = "warning"): void {
    this.push(id, { kind: "limitWarning", account: this.tasksById.get(id)?.account ?? "", level, percent: 92, resetsAt: null });
  }

  /** Test helper: the agent of a task reports any event (a new card, the end...). */
  simulateEvent(id: string, event: AgentEvent): void {
    this.push(id, event);
  }

  async handoffOptions(id: string): Promise<string[]> {
    const task = this.tasksById.get(id);
    const current = this.state.accounts.find((a) => a.id === task?.account);
    return this.state.accounts
      .filter((a) => a.id !== current?.id && (a.provider !== current?.provider || a.kind === "apiKey"))
      .map((a) => a.id);
  }

  async handoffTask(id: string, account: string): Promise<void> {
    const task = this.tasksById.get(id);
    if (!task) throw `no existe la tarea ${id}`;
    const target = this.account(account);
    clearTimeout(this.waitTimers.get(id));
    task.wait = null;
    task.account = account;
    task.live = true;
    this.push(id, { kind: "phaseStarted", phase: "implement", agent: target.providerName.split(" ")[0] ?? "IA", account: target.name });
    this.script(id, this.finish());
  }

  async recentCommits(): Promise<Commit[]> {
    return [
      { hash: "a3f9c1e", subject: "Arreglo del envío" },
      { hash: "7bd20aa", subject: "Plantillas de correo" },
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

  onOpenTask(listener: (request: OpenTaskRequest) => void): Unsubscribe {
    this.openTaskListeners.add(listener);
    return () => this.openTaskListeners.delete(listener);
  }

  // ---------------------------------------------------------------- personal sync

  /** Tests: the sync's state (`null`: not set up). */
  sync: SyncStatus | null = null;
  syncConflictList: SyncConflict[] = [];
  /** Tests: the repo of each project, as its owner key. */
  repoOwners = new Map<string, string>();
  /** Tests: the next sync fails with this message. */
  syncFailure: string | null = null;
  /** Tests: setup waits for this before answering (a slow repo). */
  syncGate: Promise<void> | null = null;
  private homes = new Map<string, MemoryHomeKind>();
  private owners = { mine: new Set<string>(), shared: new Set<string>() };
  private syncListeners = new Set<() => void>();
  static readonly SYNC_WORDS = "árbol nube lápiz río mesa farol cobre pasto vuelo menta ancla ritmo";

  private checkSyncUrl(url: string) {
    if (!url.trim()) throw "falta la dirección del repo";
    if (/^https?:\/\/[^/]*@/i.test(url.trim())) {
      throw "esa dirección lleva una contraseña o un token, y Hedy no guarda ninguno: usa la dirección sin él (con SSH, o con el gestor de credenciales de git)";
    }
  }

  private now(): number {
    return Math.floor(Date.now() / 1000);
  }

  async syncStatus(): Promise<SyncStatus | null> {
    return this.sync && { ...this.sync, conflicts: this.syncConflictList.length };
  }

  async syncDefaultName(): Promise<string> {
    return "Casa";
  }

  async syncRepoIsPublic(url: string): Promise<boolean> {
    this.checkSyncUrl(url);
    return url.includes("public");
  }

  async syncSetup(url: string, name: string): Promise<string> {
    this.checkSyncUrl(url);
    if (this.sync) throw "la sincronización ya está configurada en esta PC";
    await this.syncGate;
    this.sync = {
      url: url.trim(),
      lastSync: this.now(),
      lastError: null,
      devices: [{ id: "pc-1", name, lastSync: this.now(), thisPc: true }],
      conflicts: 0,
    };
    return MockApi.SYNC_WORDS;
  }

  async syncJoin(url: string, words: string, name: string): Promise<SyncReport> {
    this.checkSyncUrl(url);
    if (this.sync) throw "la sincronización ya está configurada en esta PC";
    if (words.trim().split(/\s+/).length !== 12) throw "esas palabras no son una clave de recuperación de 12 palabras";
    this.sync = {
      url: url.trim(),
      lastSync: this.now(),
      lastError: null,
      devices: [
        { id: "pc-2", name, lastSync: this.now(), thisPc: true },
        { id: "pc-1", name: "Casa", lastSync: this.now() - 3600, thisPc: false },
      ],
      conflicts: 0,
    };
    return { sent: 0, received: 8, conflicts: 0 };
  }

  async syncNow(): Promise<SyncReport> {
    if (!this.sync) throw "la sincronización no está configurada en esta PC";
    if (this.syncFailure) {
      this.sync.lastError = this.syncFailure;
      throw this.syncFailure;
    }
    this.sync.lastSync = this.now();
    this.sync.lastError = null;
    const me = this.sync.devices.find((d) => d.thisPc);
    if (me) me.lastSync = this.sync.lastSync;
    return { sent: 0, received: 0, conflicts: 0 };
  }

  async syncRemoveDevice(id: string): Promise<SyncReport> {
    if (!this.sync) throw "la sincronización no está configurada en esta PC";
    const device = this.sync.devices.find((d) => d.id === id);
    if (!device) throw `no hay ninguna PC «${id}» en la sincronización`;
    if (device.thisPc) throw "esta PC no se quita a sí misma: deja de sincronizar en ella, y quítala desde otra";
    this.sync.devices = this.sync.devices.filter((d) => d.id !== id);
    return { sent: 1, received: 0, conflicts: 0 };
  }

  async syncLeave(): Promise<boolean> {
    const was = this.sync !== null;
    this.sync = null;
    this.syncConflictList = [];
    return was;
  }

  async syncConflicts(): Promise<SyncConflict[]> {
    return [...this.syncConflictList];
  }

  /** Tests: which side each settled conflict used. */
  resolved: { id: number; useOther: boolean }[] = [];

  async resolveSyncConflict(id: number, useOther: boolean): Promise<void> {
    if (!this.syncConflictList.some((c) => c.id === id)) throw "ese conflicto ya no existe";
    this.syncConflictList = this.syncConflictList.filter((c) => c.id !== id);
    this.resolved.push({ id, useOther });
  }

  /** The owner of each project's `origin` stands for all its remotes here. */
  private homeOf(path: string): MemoryHome {
    const owner = this.repoOwners.get(path) ?? null;
    const status = (home: MemoryHomeKind | null, chosen: boolean, ask: string | null): MemoryHome => ({
      path,
      home,
      chosen,
      ask,
      group: owner,
      syncs: home?.kind === "personal" && owner !== null,
    });
    const chosen = this.homes.get(path);
    if (chosen) return status(chosen, true, null);
    if (!owner) return status(null, false, null);
    if (this.owners.shared.has(owner)) return status({ kind: "shared", group: owner }, false, null);
    if (this.owners.mine.has(owner)) return status({ kind: "personal" }, false, null);
    return status(null, false, owner);
  }

  async memoryHome(project: string): Promise<MemoryHome> {
    return this.homeOf(project);
  }

  async memoryHomes(): Promise<MemoryHome[]> {
    return this.state.projects.map((p) => this.homeOf(p.path));
  }

  async setMemoryHome(project: string, home: MemoryHomeKind | null): Promise<void> {
    if (home) this.homes.set(project, home);
    else this.homes.delete(project);
  }

  async answerOwner(owner: string, mine: boolean): Promise<void> {
    (mine ? this.owners.mine : this.owners.shared).add(owner);
    (mine ? this.owners.shared : this.owners.mine).delete(owner);
  }

  async forgetOwner(owner: string): Promise<void> {
    this.owners.mine.delete(owner);
    this.owners.shared.delete(owner);
  }

  onSyncUpdated(listener: () => void): Unsubscribe {
    this.syncListeners.add(listener);
    return () => this.syncListeners.delete(listener);
  }

  /** Tests: the sync that runs on its own finished. */
  emitSyncUpdated(): void {
    for (const listener of this.syncListeners) listener();
  }

  // ---------------------------------------------------------------- team memory

  /** Tests: the teams this PC is in, each with what the core would hold. */
  teamList: MockTeam[] = [];
  static readonly TEAM_WORDS = "faro trigo nube cobre ancla menta río lápiz vuelo mesa ritmo pasto";
  private teamSeq = 0;

  private team(id: string): MockTeam {
    const team = this.teamList.find((t) => t.summary.id === id || t.summary.name === id);
    if (!team) throw `esta PC no está en el equipo «${id}»`;
    return team;
  }

  private teamMe(team: MockTeam): TeamPerson {
    const me = team.people.find((p) => p.me);
    if (!me) throw "esta PC ya no está en el equipo";
    return me;
  }

  private teamUid(): string {
    this.teamSeq += 1;
    return `u${String(this.teamSeq).padStart(8, "0")}`;
  }

  async teams(): Promise<TeamSummary[]> {
    return this.teamList.map((t) => ({ ...t.summary }));
  }

  async teamStatus(id: string): Promise<TeamStatus> {
    const team = this.team(id);
    const layer = team.notes.filter((n) => n.teamLayer);
    const layerTokens = Math.ceil(layer.reduce((sum, n) => sum + n.text.length + n.author.length + 20, 0) / 4);
    const people = [...team.people].sort(
      (a, b) => Number(a.role !== "admin") - Number(b.role !== "admin") || a.name.localeCompare(b.name),
    );
    return {
      summary: { ...team.summary },
      device: "pc-1",
      person: this.teamMe(team).person,
      people,
      suggestions: team.suggestions.length,
      toReview: team.suggestions.filter((s) => s.canReview).length,
      conflicts: 0,
      invites: [...team.invites],
      failedInvites: [...team.failedInvites],
      layerTokens,
      layerMax: 1500,
    };
  }

  async teamCreate(url: string, name: string, me: string, pc: string): Promise<TeamCreated> {
    this.checkSyncUrl(url);
    if (!name.trim()) throw "falta el nombre del equipo";
    if (!me.trim()) throw "falta tu nombre en el equipo";
    const id = `team-${this.teamList.length + 1}`;
    this.teamList.push({
      summary: { id, name: name.trim(), url: url.trim(), waiting: false, leaving: false, role: "admin", lastSync: this.now(), lastError: null },
      people: [
        { person: "p-me", name: me.trim(), role: "admin", me: true, devices: [{ id: "pc-1", name: pc, lastSync: this.now(), thisPc: true }] },
      ],
      invites: [],
      failedInvites: [],
      notes: [],
      suggestions: [],
    });
    return { id, words: MockApi.TEAM_WORDS };
  }

  async teamInvite(id: string, admin: boolean, mine: boolean): Promise<TeamInvite> {
    const team = this.team(id);
    if (!mine && this.teamMe(team).role !== "admin") throw "tu rol en el equipo no lo permite: pídeselo a un admin";
    const number = 7 + team.invites.length;
    const invite: TeamInvite = {
      number,
      code: `${number}-tigre-lima-nube-faro`,
      expiresAt: this.now() + 48 * 3600,
      ownPc: mine,
      role: mine ? this.teamMe(team).role : admin ? "admin" : "member",
      answered: false,
    };
    team.invites.push(invite);
    return invite;
  }

  /** Tests: an agent's proposal in `project`, as the memory server would add it. */
  addProposal(project: string, text: string, author = "Claude"): Note {
    const note: Note = {
      id: Math.max(0, ...this.notes.map((n) => n.id)) + 1,
      project,
      text,
      category: "decision",
      author,
      status: "proposed",
      createdAt: this.now(),
      uses: 0,
      lastUsed: null,
      topic: null,
      updatedAt: null,
    };
    this.notes.push(note);
    return note;
  }

  async teamCancelInvite(id: string, number: number): Promise<void> {
    const team = this.team(id);
    team.invites = team.invites.filter((i) => i.number !== number);
  }

  async teamDismissFailedInvite(id: string, number: number): Promise<void> {
    const team = this.team(id);
    team.failedInvites = team.failedInvites.filter((n) => n !== number);
  }

  async teamJoin(url: string, code: string, me: string, pc: string): Promise<TeamJoin> {
    this.checkSyncUrl(url);
    if (!/^\d+[-\s]+\p{L}+[-\s]+\p{L}+[-\s]+\p{L}+[-\s]+\p{L}+$/u.test(code.trim())) {
      throw "eso no es un código de invitación: revisa el número y las cuatro palabras";
    }
    const id = `team-${this.teamList.length + 1}`;
    this.teamList.push({
      summary: { id, name: "", url: url.trim(), waiting: true, leaving: false, role: null, lastSync: null, lastError: null },
      people: [{ person: "p-me", name: me.trim(), role: "member", me: true, devices: [{ id: "pc-1", name: pc, lastSync: null, thisPc: true }] }],
      invites: [],
      failedInvites: [],
      notes: [],
      suggestions: [],
    });
    return { id, joined: false };
  }

  async teamSync(id: string): Promise<SyncReport> {
    const team = this.team(id);
    if (team.summary.waiting) {
      // The inviting PC answered meanwhile.
      team.summary = { ...team.summary, waiting: false, name: "Plataforma", role: "member", lastSync: this.now() };
      team.people.push({ person: "p-ana", name: "Ana", role: "admin", me: false, devices: [{ id: "pc-a", name: "Portátil de Ana", lastSync: this.now(), thisPc: false }] });
      return { sent: 0, received: 3, conflicts: 0 };
    }
    if (team.summary.leaving) {
      this.teamList = this.teamList.filter((t) => t !== team);
      return { sent: 0, received: 0, conflicts: 0 };
    }
    team.summary.lastSync = this.now();
    return { sent: 0, received: 0, conflicts: 0 };
  }

  async teamLinkProject(id: string, project: string): Promise<void> {
    this.team(id);
    this.homes.set(project, { kind: "team", team: id });
  }

  async teamNotes(id: string): Promise<TeamNote[]> {
    return [...this.team(id).notes].sort((a, b) => Number(!a.teamLayer) - Number(!b.teamLayer) || a.createdAt - b.createdAt);
  }

  private teamWrite(team: MockTeam, project: string | null, text: string, category: NoteCategory, author?: TeamPerson): TeamWrite {
    if (/[\u200b-\u200f\u202a-\u202e\u2066-\u2069]/.test(text)) {
      throw "la nota tiene caracteres ocultos que el agente leería y tú no ves: escríbela de nuevo sin ellos";
    }
    const me = author ?? this.teamMe(team);
    const uid = this.teamUid();
    if (project === null && this.teamMe(team).role !== "admin") {
      team.suggestions.push({
        uid, note: null, before: null, after: text, shown: text, hidden: false, category, topic: null,
        teamLayer: true, project: null, person: me.person, author: me.name, mine: true, canReview: false, createdAt: this.now(),
      });
      return { published: false, uid };
    }
    team.notes.push({
      uid, text, category, topic: null, teamLayer: project === null, project, person: me.person, author: me.name,
      mine: me.me, canEdit: true, createdAt: this.now(), updatedAt: null,
    });
    return { published: true, uid };
  }

  async teamAddNote(id: string, project: string | null, text: string, category: NoteCategory): Promise<TeamWrite> {
    const team = this.team(id);
    if (project !== null) {
      const home = this.homes.get(project);
      if (home?.kind !== "team" || home.team !== team.summary.id) {
        throw "la memoria de este proyecto no es del equipo: conéctalo al equipo primero";
      }
    }
    return this.teamWrite(team, project, text.trim(), category);
  }

  async teamEditNote(id: string, uid: string, text: string | null): Promise<TeamWrite> {
    const team = this.team(id);
    const note = team.notes.find((n) => n.uid === uid);
    if (!note) throw `el equipo no tiene la nota o sugerencia ${uid}`;
    const me = this.teamMe(team);
    if (!note.canEdit) {
      const sid = this.teamUid();
      team.suggestions.push({
        uid: sid, note: uid, before: note.text, after: text, shown: text, hidden: false, category: note.category, topic: null,
        teamLayer: note.teamLayer, project: note.project, person: me.person, author: me.name, mine: true, canReview: false,
        createdAt: this.now(),
      });
      return { published: false, uid: sid };
    }
    if (text === null) team.notes = team.notes.filter((n) => n !== note);
    else Object.assign(note, { text, updatedAt: this.now() });
    return { published: true, uid };
  }

  async teamPublishNote(id: string, noteId: number, everyone: boolean): Promise<TeamWrite> {
    const team = this.team(id);
    const local = this.note(noteId);
    if (local.project) {
      const home = this.homes.get(local.project);
      if (home?.kind !== "team" || home.team !== team.summary.id) {
        throw "la memoria de este proyecto no es del equipo: conéctalo al equipo primero";
      }
    }
    const write = this.teamWrite(team, everyone ? null : local.project, local.text, local.category);
    if (write.published) this.notes = this.notes.filter((n) => n.id !== noteId);
    return write;
  }

  async teamSuggestions(id: string): Promise<TeamSuggestion[]> {
    return [...this.team(id).suggestions];
  }

  async teamAcceptSuggestion(id: string, uid: string): Promise<void> {
    const team = this.team(id);
    const suggestion = team.suggestions.find((s) => s.uid === uid);
    if (!suggestion) throw `el equipo no tiene la nota o sugerencia ${uid}`;
    if (!suggestion.canReview) throw "tu rol en el equipo no lo permite: pídeselo a un admin";
    if (suggestion.hidden) throw "la nota tiene caracteres ocultos que el agente leería y tú no ves";
    team.suggestions = team.suggestions.filter((s) => s !== suggestion);
    const note = suggestion.note && team.notes.find((n) => n.uid === suggestion.note);
    if (note) {
      if (suggestion.after === null) team.notes = team.notes.filter((n) => n !== note);
      else Object.assign(note, { text: suggestion.after, updatedAt: this.now() });
    } else if (suggestion.after !== null) {
      team.notes.push({
        uid: this.teamUid(), text: suggestion.after, category: suggestion.category, topic: null, teamLayer: suggestion.teamLayer,
        project: suggestion.project, person: suggestion.person, author: suggestion.author, mine: false, canEdit: true,
        createdAt: this.now(), updatedAt: null,
      });
    }
  }

  async teamRejectSuggestion(id: string, uid: string): Promise<void> {
    const team = this.team(id);
    team.suggestions = team.suggestions.filter((s) => s.uid !== uid);
  }

  private adminOf(team: MockTeam) {
    if (this.teamMe(team).role !== "admin") throw "tu rol en el equipo no lo permite: pídeselo a un admin";
  }

  private lastAdmin(team: MockTeam, people: TeamPerson[]) {
    if (people.length > 0 && !people.some((p) => p.role === "admin")) {
      throw "el equipo se quedaría sin admin: haz admin a otra persona primero";
    }
    void team;
  }

  async teamSetRole(id: string, person: string, admin: boolean): Promise<void> {
    const team = this.team(id);
    this.adminOf(team);
    const after = team.people.map((p) => (p.person === person ? { ...p, role: (admin ? "admin" : "member") as TeamRole } : p));
    this.lastAdmin(team, after);
    team.people = after;
  }

  async teamRemovePerson(id: string, person: string): Promise<void> {
    const team = this.team(id);
    this.adminOf(team);
    if (this.teamMe(team).person === person) throw "esta PC no se quita a sí misma: deja de sincronizar en ella, y quítala desde otra";
    team.people = team.people.filter((p) => p.person !== person);
  }

  async teamRemovePc(id: string, device: string): Promise<void> {
    const team = this.team(id);
    team.people = team.people.map((p) => ({ ...p, devices: p.devices.filter((d) => d.id !== device) })).filter((p) => p.devices.length > 0);
  }

  async teamRename(id: string, name: string): Promise<void> {
    const team = this.team(id);
    this.adminOf(team);
    team.summary.name = name.trim();
  }

  async teamRenamePerson(id: string, person: string, name: string): Promise<void> {
    const team = this.team(id);
    this.adminOf(team);
    team.people = team.people.map((p) => (p.person === person ? { ...p, name: name.trim() } : p));
  }

  async teamLeave(id: string, here: boolean): Promise<void> {
    const team = this.team(id);
    const me = team.people.find((p) => p.me);
    if (!here && me?.role === "admin") {
      this.lastAdmin(team, team.people.filter((p) => !p.me));
      if (team.people.length > 1) {
        team.summary.leaving = true;
        return;
      }
    }
    this.teamList = this.teamList.filter((t) => t !== team);
    for (const [project, home] of this.homes) {
      if (home.kind === "team" && home.team === team.summary.id) this.homes.delete(project);
    }
  }

  // ---------------------------------------------------------------- files

  /** Tests: the files `projectFiles` answers for a project. */
  files = new Map<string, string[]>([
    [
      "/code/hedy",
      [
        "README.md",
        "citas/__init__.py",
        "citas/modelos.py",
        "citas/servicio.py",
        "citas/vistas.py",
        "notificaciones/recordatorios.py",
        "notificaciones/whatsapp.py",
        "pyproject.toml",
        "tests/test_citas.py",
        "tests/test_recordatorios.py",
      ],
    ],
  ]);

  async projectFiles(project: string): Promise<string[]> {
    this.project(project);
    return [...(this.files.get(project) ?? [])];
  }

  // ---------------------------------------------------------------- code

  /** Code navigation over the sample repositories (`mockCodeSamples.ts`). */
  code = new MockCode({
    task: (id) => this.tasksById.get(id),
    tasks: (project) => [...this.tasksById.values()].filter((t) => t.project === project),
    checkProject: (path) => void this.project(path),
  });

  async codeCopy(copy: CodeCopy): Promise<CopyInfo> {
    return this.code.copy(copy);
  }

  async codeTree(copy: CodeCopy, options?: { ignored?: boolean }): Promise<CodeTree> {
    return this.code.tree(copy, options?.ignored ?? false);
  }

  async readFile(copy: CodeCopy, path: string, rev?: Rev): Promise<FileContent> {
    return this.code.readFile(copy, path, rev);
  }

  async fileDiff(copy: CodeCopy, path: string, options?: DiffOptions): Promise<CodeDiff> {
    return this.code.fileDiff(copy, path, options);
  }

  async searchText(copy: CodeCopy, query: string, options?: SearchOptions): Promise<SearchResults> {
    return this.code.searchText(copy, query, options);
  }

  async fileHistory(copy: CodeCopy, path: string, lines?: LineRange): Promise<FileHistory> {
    return this.code.fileHistory(copy, path, lines);
  }

  async blame(copy: CodeCopy, path: string, options?: { rev?: Rev; lines?: LineRange }): Promise<Blame> {
    return this.code.blame(copy, path, options);
  }

  async fileSymbols(copy: CodeCopy, path: string): Promise<CodeSymbol[]> {
    return this.code.fileSymbols(copy, path);
  }

  async openFileInEditor(copy: CodeCopy, path: string, line?: number): Promise<void> {
    this.code.openFileInEditor(copy, path, line);
  }

  async revealInFileManager(copy: CodeCopy, path: string): Promise<void> {
    this.code.revealInFileManager(copy, path);
  }

  /** A task's review reads its worktree: after an edit there, its diff is computed again. */
  private edited<T>(copy: CodeCopy, result: T): T {
    if (copy.kind === "task") {
      const task = this.tasksById.get(copy.task);
      if (task && this.code.hasTaskChanges(task.project, task.id)) {
        this.diffs.set(task.id, this.code.taskDiff(task.project, task.id));
      }
    }
    return result;
  }

  async writeFile(copy: CodeCopy, path: string, text: string, expected: string): Promise<WriteResult> {
    return this.edited(copy, this.code.writeFile(copy, path, text, expected));
  }

  async createFile(copy: CodeCopy, path: string): Promise<{ version: string; size: number }> {
    return this.edited(copy, this.code.createFile(copy, path));
  }

  async createFolder(copy: CodeCopy, path: string): Promise<void> {
    this.code.createFolder(copy, path);
  }

  async renamePath(copy: CodeCopy, from: string, to: string): Promise<void> {
    this.edited(copy, this.code.renamePath(copy, from, to));
  }

  async deletePath(copy: CodeCopy, path: string): Promise<void> {
    this.edited(copy, this.code.deletePath(copy, path));
  }

  async duplicateFile(copy: CodeCopy, path: string): Promise<string> {
    return this.edited(copy, this.code.duplicateFile(copy, path));
  }

  private closeGuards = new Set<() => Promise<boolean>>();

  onCloseRequested(confirm: () => Promise<boolean>): Unsubscribe {
    this.closeGuards.add(confirm);
    return () => this.closeGuards.delete(confirm);
  }

  /** Tests: the user closes the window; resolves to whether it closed. */
  async requestClose(): Promise<boolean> {
    for (const confirm of this.closeGuards) if (!(await confirm())) return false;
    return true;
  }

  // ---------------------------------------------------------------- images

  private attachments = new Map<string, string>();
  private imageSupportFor = new Map<string, ImageSupport>();
  private pdfSupportFor = new Map<string, ImageSupport>();
  private nextAttachment = 0;

  /** Tests: what `account` says about images. */
  setImageSupport(account: string, support: ImageSupport): void {
    this.imageSupportFor.set(account, support);
  }

  async imageSupport(account: string): Promise<ImageSupport> {
    const set = this.imageSupportFor.get(account);
    if (set) return set;
    const found = this.account(account);
    // Like the backend: an API account without its key cannot work yet.
    if (found.kind === "apiKey") return found.login.state === "loggedIn" ? "yes" : "no";
    return found.provider === "claude" || found.provider === "codex" ? "yes" : "ifAgentAllows";
  }

  /** Tests: what `account` says about PDFs. */
  setPdfSupport(account: string, support: ImageSupport): void {
    this.pdfSupportFor.set(account, support);
  }

  async pdfSupport(account: string): Promise<ImageSupport> {
    const set = this.pdfSupportFor.get(account);
    if (set) return set;
    const found = this.account(account);
    if (found.kind === "apiKey") return "no";
    return found.provider === "claude" ? "yes" : found.provider === "gemini" ? "ifAgentAllows" : "no";
  }

  async attachment(taskId: string, file: string): Promise<string | null> {
    return this.attachments.get(`${taskId}/${file}`) ?? null;
  }

  /** Checks and keeps the images of a message, like the backend. */
  private async storeImages(taskId: string, account: string, images: ImageInput[] | undefined): Promise<Attachment[]> {
    if (!images?.length) return [];
    if (images.length > MAX_IMAGES) throw `como mucho ${MAX_IMAGES} adjuntos por mensaje`;
    const typed = images.map((image) => {
      const mediaType = IMAGE_PREFIX.find(([prefix]) => image.data.startsWith(prefix))?.[1];
      if (!mediaType) throw `«${image.name}» no es una imagen PNG, JPEG, GIF o WebP ni un PDF`;
      if ((image.data.length * 3) / 4 > MAX_IMAGE_BYTES) throw `«${image.name}» pesa más de 5 MB`;
      return { image, mediaType };
    });
    if ((await this.imageSupport(account)) === "no") {
      throw `${this.account(account).providerName} no acepta imágenes con esta cuenta`;
    }
    if (typed.some((t) => t.mediaType === "application/pdf") && (await this.pdfSupport(account)) === "no") {
      throw `${this.account(account).providerName} no lee PDF: quítalo, pega el texto o usa otra cuenta`;
    }
    return typed.map(({ image, mediaType }) => {
      const file = `${(++this.nextAttachment).toString(16)}-0.${mediaType.split("/")[1]}`;
      this.attachments.set(`${taskId}/${file}`, `data:${mediaType};base64,${image.data}`);
      return { name: image.name.split(/[/\\]/).pop() || "imagen", file, mediaType };
    });
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

/** A rough stand-in for the backend heuristic: same words, other numbers or a negation. */
/** The approved note a proposal would change: same topic, same scope. */
/** Lowercase, without accents: how the mock compares words. */
function fold(text: string): string {
  return text.toLowerCase().normalize("NFD").replace(/[\u0300-\u036f]/g, "");
}

/** The words of a question that say what it is about (four letters or more, not the asking ones). */
function askTerms(question: string): string[] {
  const asking = new Set(["como", "cual", "cuando", "donde", "quien", "usamos", "usan", "hacemos", "tenemos", "este", "esta", "equipo", "proyecto", "what", "when", "where", "which", "does"]);
  return fold(question)
    .split(/[^a-z0-9#]+/)
    .filter((w) => w.length >= 4 && !asking.has(w))
    .map((w) => (w.length > 5 && w.endsWith("s") ? w.slice(0, -1) : w));
}

function topicTarget(approved: Note[], proposal: Note): Note | null {
  if (!proposal.topic) return null;
  return approved.find((n) => n.id !== proposal.id && n.project === proposal.project && n.topic === proposal.topic) ?? null;
}

/** Like the app: they share at least two meaningful words, and half of the shorter note's. */
function mockSimilar(a: string, b: string): boolean {
  const words = (t: string) =>
    new Set(
      (t.toLowerCase().normalize("NFD").replace(/\p{M}/gu, "").match(/[a-z0-9]+/g) ?? [])
        .filter((w) => w.length > 3)
        .map((w) => w.replace(/s$/, "")),
    );
  const [wa, wb] = [words(a), words(b)];
  const common = [...wa].filter((w) => wb.has(w)).length;
  return common >= 2 && common * 2 >= Math.min(wa.size, wb.size);
}

function mockContradicts(a: string, b: string): boolean {
  const strip = (t: string) => t.toLowerCase().replace(/\b(no|nunca)\b|\d+/g, "").replace(/\s+/g, " ").trim();
  return a !== b && strip(a) === strip(b);
}

function emptyProject(path: string): Project {
  return {
    path,
    name: path.split("/").filter(Boolean).pop() ?? path,
    account: null,
    protected: false,
    color: null,
    offerableAccounts: [],
    phases: {},
    client: null,
    blockedFiles: [],
    permissionMode: "manual",
  };
}

/** A sample project: like the sample code's locked files, it keeps the usual ones from the AI. */
function project(path: string, account: string, extra: Partial<Project>): Project {
  return { ...emptyProject(path), account, blockedFiles: [...SUGGESTED_BLOCKED], ...extra };
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

/** What the mock holds of one team. */
export interface MockTeam {
  summary: TeamSummary;
  people: TeamPerson[];
  invites: TeamInvite[];
  failedInvites: number[];
  notes: TeamNote[];
  suggestions: TeamSuggestion[];
}

/** The sample team ("Plataforma", the canvas's): two admins, three members. */
function sampleTeam(now: number): MockTeam {
  const pc = (id: string, name: string, ago: number, thisPc = false) => ({ id, name, lastSync: now - ago, thisPc });
  const note = (
    uid: string,
    text: string,
    teamLayer: boolean,
    author: string,
    person: string,
    extra: Partial<TeamNote> = {},
  ): TeamNote => ({
    uid,
    text,
    category: "convention",
    topic: null,
    teamLayer,
    project: teamLayer ? null : "/code/tienda",
    person,
    author,
    mine: person === "p-me",
    canEdit: person === "p-me" || teamLayer,
    createdAt: now - 86_400,
    updatedAt: null,
    ...extra,
  });
  return {
    summary: {
      id: "team-1",
      name: "Plataforma",
      url: "git@github.com:empresa/hedy-team.git",
      waiting: false,
      leaving: false,
      role: "admin",
      lastSync: now - 300,
      lastError: null,
    },
    people: [
      { person: "p-me", name: "Alex", role: "admin", me: true, devices: [pc("pc-1", "Casa", 0, true), pc("pc-9", "Oficina", 7200)] },
      { person: "p-marta", name: "Marta", role: "admin", me: false, devices: [pc("pc-m1", "Oficina", 1200), pc("pc-m2", "Portátil", 34 * 86_400)] },
      { person: "p-ana", name: "Ana", role: "member", me: false, devices: [pc("pc-a", "Portátil de Ana", 3600)] },
      { person: "p-luis", name: "Luis", role: "member", me: false, devices: [pc("pc-l", "Escritorio", 86_400)] },
    ],
    invites: [{ number: 7, code: "7-tigre-lima-nube-faro", expiresAt: now + 47 * 3600, ownPc: false, role: "member", answered: false }],
    failedInvites: [],
    notes: [
      note("n1", "Los commits siguen Conventional Commits.", true, "Marta", "p-marta", { category: "rule" }),
      note("n2", "Nunca se usan datos reales de clientes en pruebas.", true, "Alex", "p-me", { category: "rule" }),
      note("n3", "Los webhooks del banco reintentan 3 veces.", false, "Ana", "p-ana", { category: "decision" }),
      note("n4", "Los tests de integración necesitan la base en docker.", false, "Luis", "p-luis"),
      note("n5", "Los importes van en céntimos.", false, "Alex", "p-me"),
    ],
    suggestions: [
      {
        uid: "s1",
        note: "n1",
        before: "Los commits siguen Conventional Commits.",
        after: "Los commits siguen Conventional Commits, en una línea y sin cuerpo.",
        shown: "Los commits siguen Conventional Commits, en una línea y sin cuerpo.",
        hidden: false,
        category: "rule",
        topic: null,
        teamLayer: true,
        project: null,
        person: "p-luis",
        author: "Luis",
        mine: false,
        canReview: true,
        createdAt: now - 3600,
      },
      {
        uid: "s2",
        note: null,
        before: null,
        after: "Las migraciones se revisan en pareja.",
        shown: "Las migraciones se revisan en pareja.",
        hidden: false,
        category: "convention",
        topic: null,
        teamLayer: true,
        project: null,
        person: "p-ana",
        author: "Ana",
        mine: false,
        canReview: true,
        createdAt: now - 1800,
      },
    ],
  };
}
