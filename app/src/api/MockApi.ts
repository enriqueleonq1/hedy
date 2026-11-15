import type { HedyApi, Unsubscribe } from "./HedyApi";
import { resolveLanguage } from "../i18n";
import { placeholders } from "../templates/placeholders";
import type {
  AcceptPreview,
  Account,
  Attachment,
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

/** Like the backend's `task_state`: what a task shows on the board. */
function boardState(task: Task, events: AgentEvent[]): BoardState {
  switch (task.status) {
    case "waitingApproval": {
      const open = new Set<string>();
      for (const e of events) {
        if (e.kind === "plan" && e.requestId) open.add(e.requestId);
        if (e.kind === "permissionResolved") open.delete(e.requestId);
      }
      return open.size > 0 ? "plan" : "permission";
    }
    case "accepted":
      return "accepted";
    case "discarded":
      return "cancelled";
    case "review":
      return task.mode === "project" ? "review" : "answered";
    case "running":
    case "failed": {
      let limited = false;
      for (const e of events) {
        if (e.kind === "limitWarning" && e.level === "reached") limited = true;
        if (e.kind === "userMessage" || e.kind === "phaseStarted") limited = false;
        if (e.kind === "rateLimit" && e.status !== "rejected") limited = false;
      }
      if (limited) return "limited";
      return task.status === "running" ? "working" : "failed";
    }
  }
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
export class MockApi implements HedyApi {
  private state: Overview;

  constructor(initial?: Partial<Overview>) {
    this.state = {
      userName: "Enrique",
      theme: "system",
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
        projects: ["/code/homming"],
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
    api.state.projects = [
      project("/code/hedy", "claude-personal", { color: "#73DACA", client: "León Blanco" }),
      project("/code/homming", "claude-trabajo", { protected: true, color: "#FF9E64", client: "Homming" }),
    ];
    api.recomputeOffers();
    const day = 86_400;
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
      api.usage.push({ project: "/code/hedy", at, tokens: k * 1000, costUsd: k / 40 });
      api.usage.push({ project: "/code/homming", at, tokens: k * 300, costUsd: null });
    });
    return api;
  }

  // --------------------------------------------------------------- usage

  private usage: (UsagePoint & { project: string })[] = [];

  async setProjectClient(path: string, client: string | null): Promise<void> {
    this.project(path).client = client?.trim() || null;
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
      entry.costUsd = add(entry.costUsd, u.costUsd);
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
  private skillFiles(skill: Skill, kind: "written" | "removed"): SkillFiles {
    const files: SkillFiles = { written: [], removed: [], skipped: [] };
    if (skill.project) {
      files[kind] = [".claude/skills", ".agents/skills"].map((dir) => `${skill.project}/${dir}/${skill.name}/SKILL.md`);
    }
    return files;
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
    return { skill: structuredClone(skill), files: this.skillFiles(skill, "written") };
  }

  async editSkill(id: number, path: string, draft: SkillDraft): Promise<SkillChange> {
    const skill = this.skillById(id);
    const name = this.checkDraft(draft);
    const before = structuredClone(skill);
    Object.assign(skill, {
      name,
      description: draft.description.trim(),
      body: draft.body.trim(),
      project: draft.global ? null : path,
      updatedAt: Math.floor(Date.now() / 1000),
    });
    if (skill.status !== "approved") return { skill: structuredClone(skill), files: { written: [], removed: [], skipped: [] } };
    const removed = this.skillFiles(before, "removed").removed;
    return { skill: structuredClone(skill), files: { ...this.skillFiles(skill, "written"), removed } };
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
    return { skill: structuredClone(approved), files: this.skillFiles(approved, "written") };
  }

  async rejectSkill(id: number): Promise<void> {
    if (this.skillById(id).status !== "proposed") throw `la habilidad ${id} no es una propuesta: ya está aprobada`;
    this.skillList = this.skillList.filter((s) => s.id !== id);
  }

  async removeSkill(id: number): Promise<SkillFiles> {
    const skill = this.skillById(id);
    this.skillList = this.skillList.filter((s) => s.id !== id);
    return skill.status === "approved" ? this.skillFiles(skill, "removed") : { written: [], removed: [], skipped: [] };
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
      client: input.client?.trim() || null,
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
    if (key === "ui.name") this.state.userName = value ?? "Enrique";
    if (key === "ui.language") this.state.language = (value ?? "system") as Overview["language"];
    if (key === "ui.onboarded") this.state.onboarded = value === "true";
    if (key === "ui.notifications") this.state.notifications = value !== "off";
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
    const id = `t${++this.nextTask}`;
    const images = await this.storeImages(id, project.account, input.images);
    const title = (input.prompt.split("\n")[0] ?? "").slice(0, 60);
    const planFirst = input.planFirst && input.mode === "project";
    const stage = (phase: "plan" | "implement") => ({ phase, account: project.account ?? "", model: input.model });
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
      stages: planFirst ? [stage("plan"), stage("implement")] : [stage("implement")],
      stage: 0,
      live: true,
      ...(images.length ? { images } : {}),
    };
    this.tasksById.set(id, task);
    this.eventsById.set(id, []);
    this.diffs.set(id, []);
    this.push(id, images.length ? { kind: "userMessage", text: input.prompt, images } : { kind: "userMessage", text: input.prompt });
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
        state: boardState(task, events),
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
      "/code/homming",
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
      "/code/homming",
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
    this.diffs.set("s5", ["facturas/impuestos.py", "facturas/totales.py", "tests/test_facturas.py"].map(sampleDiff));
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
      "/code/homming",
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

  async acceptTask(id: string, message?: string | null): Promise<void> {
    const task = this.tasksById.get(id);
    if (!task) throw `no existe la tarea ${id}`;
    this.commitMessages.set(id, message?.trim() || this.proposedMessage(task));
    this.acceptedDiffs.set(id, this.diffs.get(id) ?? []);
    task.merge = { before: "a3f9c1e", after: "5d0b7e2", tip: "9c4e1aa", at: Math.floor(Date.now() / 1000) };
    task.wait = null;
    this.close(id, "accepted");
  }

  async undoAccept(id: string): Promise<void> {
    const task = this.tasksById.get(id);
    if (!task) throw `no existe la tarea ${id}`;
    if (task.status !== "accepted" || !task.merge) {
      throw "No se puede deshacer sin riesgo: la tarea no está integrada. Si quieres quitar los cambios, usa git revert.";
    }
    if (this.undoRefusal) throw this.undoRefusal;
    task.merge = null;
    task.status = "review";
    this.diffs.set(id, this.acceptedDiffs.get(id) ?? []);
    this.notifyTask(task);
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
   * For `npm run dev`: the running sample task ("s4", in homming) goes through
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

  // ---------------------------------------------------------------- images

  private attachments = new Map<string, string>();
  private imageSupportFor = new Map<string, ImageSupport>();
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

  async attachment(taskId: string, file: string): Promise<string | null> {
    return this.attachments.get(`${taskId}/${file}`) ?? null;
  }

  /** Checks and keeps the images of a message, like the backend. */
  private async storeImages(taskId: string, account: string, images: ImageInput[] | undefined): Promise<Attachment[]> {
    if (!images?.length) return [];
    if (images.length > MAX_IMAGES) throw `como mucho ${MAX_IMAGES} imágenes por mensaje`;
    const typed = images.map((image) => {
      const mediaType = IMAGE_PREFIX.find(([prefix]) => image.data.startsWith(prefix))?.[1];
      if (!mediaType) throw `«${image.name}» no es una imagen PNG, JPEG, GIF o WebP`;
      if ((image.data.length * 3) / 4 > MAX_IMAGE_BYTES) throw `«${image.name}» pesa más de 5 MB`;
      return { image, mediaType };
    });
    if ((await this.imageSupport(account)) === "no") {
      throw `${this.account(account).providerName} no acepta imágenes con esta cuenta`;
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
