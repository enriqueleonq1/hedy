import type { LanguageSetting } from "../i18n";

// Mirrors the DTOs in `src-tauri/src/dto.rs`.

export type ProviderId = "claude" | "codex" | "gemini" | "kimi" | "antigravity";
/** Every provider has an official CLI (Kimi: Kimi Code). */
export type CliProviderId = ProviderId;

export interface Login {
  state: "loggedIn" | "loggedOut" | "unknown";
  plan: string | null;
}

export interface Account {
  id: string;
  name: string;
  provider: ProviderId;
  providerName: string;
  kind: "subscription" | "apiKey";
  defaultFolder: boolean;
  color: string;
  login: Login;
  /** Paths of the projects assigned to this account. */
  projects: string[];
  usage: Usage;
}

export interface Project {
  path: string;
  name: string;
  account: string | null;
  protected: boolean;
  color: string | null;
  /** Accounts that may be assigned here (protection rules applied). */
  offerableAccounts: string[];
  /** Who plans, implements, writes tests and reviews. */
  phases: PhasePlan;
  /** Who the work is for, to add up cost per client. */
  client: string | null;
}

/** Tokens and estimated cost (CLI estimate or configured API price). */
export interface Spend {
  tokens: number;
  costUsd: number | null;
}

export interface UsagePoint extends Spend {
  /** Unix seconds. */
  at: number;
}

export interface UsageSummary {
  /** Busiest first. */
  projects: (Spend & { path: string; name: string; client: string | null })[];
  /** Busiest first; projects without a client last. */
  clients: (Spend & { client: string | null; projects: number })[];
}

export interface ProjectStatus {
  branch: string | null;
  uncommitted: number | null;
  lastTests: { command: string; passed: boolean; at: number } | null;
  /** Ids of the accounts that work in the project. */
  agents: string[];
  codeMap: boolean;
  memoryNotes: number;
  memoryStale: number;
  memoryProposed: number;
}

export type Phase = "plan" | "implement" | "tests" | "review";
export interface Assignment {
  account: string;
  model: string | null;
}
export type PhasePlan = Partial<Record<Phase, Assignment>>;

export type ThemeSetting = "system" | "light" | "dark";

export interface Overview {
  userName: string;
  theme: ThemeSetting;
  language: LanguageSetting;
  /** The first-run guide was finished or skipped. */
  onboarded: boolean;
  /** Desktop notifications about tasks (`ui.notifications`, on by default). */
  notifications: boolean;
  dataDir: string;
  defaultAccount: string | null;
  binaries: Record<CliProviderId, string | null>;
  apiUrls: Record<"kimi" | "gemini", string | null>;
  /** `input/output` USD per million tokens. */
  apiPrices: Record<"kimi" | "gemini", string | null>;
  accounts: Account[];
  projects: Project[];
  codeMap: CodeMap;
}

/** The code-map MCP server (codebase-memory-mcp). */
export interface CodeMap {
  /** Program registered in every run; null: off. */
  enabled: string | null;
  /** Where it is installed, if found. */
  found: string | null;
}

export type NoteCategory = "decision" | "convention" | "rule" | "preference";

export interface Note {
  id: number;
  /** null: a global note (every non-protected project). */
  project: string | null;
  text: string;
  category: NoteCategory;
  /** "user" or the AI that proposed it ("Claude", "Codex"…). */
  author: string;
  status: "proposed" | "approved";
  /** Unix seconds. */
  createdAt: number;
  uses: number;
  lastUsed: number | null;
  /** Stable subject, a lowercase path such as `arquitectura/pagos`. */
  topic: string | null;
  /** Unix seconds: when an approved note was last replaced by a newer version. */
  updatedAt: number | null;
}

/** A note an AI proposed, with what the user needs to decide on it. */
export interface Proposal {
  note: Note;
  /** The approved note with the same topic: approving replaces its text. */
  replaces: Note | null;
  /** Approved notes that look like this one (local full-text search), best first. */
  similar: Note[];
}

export interface MemoryView {
  notes: Note[];
  proposed: Proposal[];
  contradictions: [Note, Note][];
  stale: Note[];
  usedLastTask: Note[];
  readers: { account: string; reads: boolean }[];
  protected: boolean;
}

/** A learned skill: a reusable procedure an AI proposed and the user approves. */
export interface Skill {
  id: number;
  /** A slug, such as `run-e2e-tests`: its folder name in each CLI. */
  name: string;
  /** One line: when to use it. */
  description: string;
  /** The steps, in Markdown. */
  body: string;
  /** null: a global skill (every non-protected project, only through Hedy's tools). */
  project: string | null;
  status: "proposed" | "approved";
  /** "user" or the AI that proposed it. */
  author: string;
  /** Unix seconds. */
  createdAt: number;
  updatedAt: number | null;
  /** The task it came from. */
  sourceTask: string | null;
}

export interface SkillProposal {
  skill: Skill;
  /** The approved skill with the same name: approving replaces its text. */
  replaces: Skill | null;
}

export interface SkillsView {
  approved: Skill[];
  proposed: SkillProposal[];
  protected: boolean;
}

/** What happened to the CLIs' skill files after a change. */
export interface SkillFiles {
  written: string[];
  removed: string[];
  /** The user's own files with that name, left alone. */
  skipped: string[];
}

export interface SkillChange {
  skill: Skill;
  files: SkillFiles;
}

export interface SkillDraft {
  name: string;
  description: string;
  body: string;
  global: boolean;
}

export interface NewAccountInput {
  name: string;
  provider: ProviderId;
  apiKey: boolean;
  defaultFolder: boolean;
  color: string | null;
}

export type SettingKey =
  | "ui.theme"
  | "ui.name"
  | "ui.language"
  | "ui.onboarded"
  | "ui.notifications"
  | "binary.claude"
  | "binary.codex"
  | "binary.gemini"
  | "binary.kimi"
  | "binary.antigravity"
  | "api_url.kimi"
  | "api_url.gemini"
  | "api_price.kimi"
  | "api_price.gemini"
  | "code_map.binary";

export type TerminalRequest =
  | { kind: "login"; account: string }
  | { kind: "run"; path: string }
  | { kind: "shell"; path: string | null }
  | { kind: "installCodeMap" };

export type TerminalEvent =
  | { event: "data"; data: string }
  | { event: "exit"; code: number | null };

// ---------------------------------------------------------------- usage

export interface Usage {
  weekTokens: number;
  /** The CLI's estimate; not what a subscription bills. */
  weekCostUsd: number | null;
  limit: { status: string; percent: number | null; resetsAt: number | null } | null;
  /** The plan's limit windows, as the provider reports them (cached reading). */
  plan: PlanState;
}

/** session = the rolling 5 hours. */
export type WindowKind = "session" | "week" | "day" | "other";

export interface UsageWindow {
  kind: WindowKind;
  /** Length in minutes, when the source says it. */
  minutes: number | null;
  /** The model this window is limited to; null: every model. */
  model: string | null;
  /** 0–100. */
  percent: number;
  /** When it renews, as the source wrote it. */
  resetsText: string | null;
  /** When it renews, in Unix seconds, when it could be worked out. */
  resetsAt: number | null;
}

export type UsageSource = "claudeUsage" | "codexRateLimits" | "taskReport" | "moonshotBalance";

export interface PlanUsage {
  windows: UsageWindow[];
  /** Unix seconds. */
  checkedAt: number;
  source: UsageSource;
  /** Prepaid credit (Kimi API keys). */
  balance: { available: number; currency: string } | null;
}

export interface PlanState {
  usage: PlanUsage | null;
  /** When Hedy last asked (Unix seconds). */
  attemptedAt: number | null;
  /** Why the last attempt gave nothing. */
  problem: "unsupported" | "unavailable" | "spendsTokens" | null;
  /** Whether Hedy has a usage source for this account. */
  supported: boolean;
}

// ---------------------------------------------------------------- tasks

export type TaskMode = "project" | "ask" | "research";
export type TaskStatus = "running" | "waitingApproval" | "review" | "accepted" | "discarded" | "failed";

export interface Workspace {
  worktree: string;
  branch: string;
  baseBranch: string;
  baseCommit: string;
}

export interface Task {
  id: string;
  project: string;
  mode: TaskMode;
  title: string;
  prompt: string;
  account: string;
  model: string | null;
  workspace: Workspace | null;
  sessionId: string | null;
  status: TaskStatus;
  createdAt: number;
  /** The phases this task runs through, and the index of the current one. */
  stages: { phase: Phase; account: string; model: string | null }[];
  stage: number;
  /** Whether its agent process is running right now. */
  live: boolean;
  /** What accepting did to the base branch, to undo it. */
  merge?: MergeRecord | null;
  /** The user chose to wait for the account's limit to renew. */
  wait?: LimitWait | null;
  /**
   * A question outside any project ("Pregunta rápida"): `project` is then its
   * own private scratch folder, and it never changes files.
   */
  conversation?: boolean;
  /** Images attached to the request itself. */
  images?: Attachment[];
  /** Where accepting put the work and what Hedy published of it. */
  ship?: Shipment | null;
  /** The conversation this task continues: its prompt opens with `origin.context`. */
  origin?: TaskOrigin | null;
}

// ------------------------------------------------- carrying a conversation

/** Where a task came from: the conversation it continues ("Llevar a un proyecto"). */
export interface TaskOrigin {
  conversation: string;
  /** The conversation's title when it was carried (kept after it is deleted). */
  title: string;
  /** The delimited context section that opens the task's prompt. */
  context: string;
}

/** A question of a conversation with the agent's final answer. */
export interface ContextTurn {
  question: string;
  answer: string;
  images: Attachment[];
}

/** What a conversation offers to carry into a project. */
export interface ConversationContext {
  conversation: string;
  title: string;
  turns: ContextTurn[];
  /** A safe folder name from the title, for "Proyecto nuevo". */
  suggestedName: string;
  /** From this many characters the context is long: suggest "Resumir con IA". */
  longAfter: number;
  /** The agent is still answering: carrying waits for it. */
  busy: boolean;
}

/** What of a conversation goes with a new task. */
export interface CarryInput {
  conversation: string;
  /** Indexes of the kept turns. */
  turns: number[];
  /** A summary instead of the turns' text (their images still go). */
  summary: string | null;
}

// ---------------------------------------------------------------- shipping

/** How "Aceptar" lands the work: merged into the base branch, or kept on the task's branch for a PR. */
export type AcceptMode = "merge" | "branch";

/** Whether GitHub CLI can open pull requests for the folder's remote. */
export type GhState = "missing" | "loggedOut" | "ready";

/** The main folder's git state, for its header. */
export interface FolderStatus {
  /** `null` on a detached HEAD. */
  branch: string | null;
  /** "origin/main"; `null` when the branch tracks nothing. */
  upstream: string | null;
  ahead: number;
  behind: number;
  staged: number;
  unstaged: number;
  untracked: number;
  conflicted: number;
  /** Files with any uncommitted change. */
  changes: number;
  remote: string | null;
  /** Only the host ("github.com"), never the URL. */
  remoteHost: string | null;
  /** `null` without a remote. */
  gh: GhState | null;
}

export type ShipFailureKind =
  | "auth"
  | "rejected"
  | "protected"
  | "offline"
  | "diverged"
  | "noUpstream"
  | "localChanges"
  | "noRemote"
  | "detached"
  | "other";

/** Why a push or pull failed; the message is already in the user's language. */
export interface ShipFailure {
  kind: ShipFailureKind;
  message: string;
}

export interface PushOutcome {
  pushed: { branch: string; upstream: string; upstreamSet: boolean } | null;
  failure: ShipFailure | null;
}

export interface PullOutcome {
  /** `false`: already up to date. */
  updated: boolean | null;
  failure: ShipFailure | null;
}

export interface PullRequest {
  /** Always https. */
  url: string;
  number: number | null;
  draft: boolean;
}

export interface Shipment {
  /** The base branch after a merge, or the task's own branch when kept. */
  branch: string;
  kept: boolean;
  message: string;
  files: { path: string; additions: number; deletions: number }[];
  pushedTo: string | null;
  pullRequest: PullRequest | null;
}

/** Where an accepted task stands on the way to its remote. */
export interface ShipState {
  branch: string;
  kept: boolean;
  remote: string | null;
  remoteHost: string | null;
  /** `null`: a push sets it (`-u`). */
  upstream: string | null;
  ahead: number;
  behind: number;
  pushed: boolean;
  gh: GhState | null;
  defaultBranch: string | null;
  pullRequest: PullRequest | null;
  canOpenPr: boolean;
}

export interface PullRequestDraft {
  title: string;
  body: string;
  base: string;
  head: string;
}

export interface NewPullRequestInput {
  title: string;
  body: string;
  base: string;
  draft: boolean;
}

/** What "Aceptar" changed in the main checkout (commits), kept to undo it. */
export interface MergeRecord {
  before: string;
  after: string;
  tip: string;
  /** Unix seconds. */
  at: number;
}

/**
 * "Esperar" on a limit card: Hedy resumes the task on the same account when
 * the limit renews (never another account).
 */
export interface LimitWait {
  account: string;
  /** Unix seconds when the limit renews; `null` when unknown. */
  until: number | null;
  /** Unix seconds when the user chose to wait. */
  since: number;
  /** The reset passed while Hedy was closed: the user resumes ("Continuar"). */
  ready: boolean;
}

/** What "Aceptar e integrar" is about to do, before anything changes. */
export interface AcceptPreview {
  branch: string;
  targetBranch: string;
  /** The branch the project's folder is on now; the merge needs the target. */
  checkoutBranch: string | null;
  files: number;
  additions: number;
  deletions: number;
  /** The proposed commit message. */
  message: string;
  /** The task's latest test run; `null` when it never ran tests. */
  tests: { command: string; passed: boolean; at: number } | null;
}

export interface NewTaskInput {
  project: string;
  prompt: string;
  mode: TaskMode;
  model: string | null;
  planFirst: boolean;
  images?: ImageInput[];
  /** A conversation carried into the task: its context opens the prompt. */
  carry?: CarryInput;
}

/** The image formats every supported CLI takes. */
export type ImageMediaType = "image/png" | "image/jpeg" | "image/gif" | "image/webp";

/** An image attached in the composer: the backend checks the real type from the bytes. */
export interface ImageInput {
  name: string;
  /** The file's bytes, base64. */
  data: string;
}

/** An image kept with a message of the history (in Hedy's data folder). */
export interface Attachment {
  /** The user's file name, for the alt text. */
  name: string;
  /** The stored file, to load it with `attachment()`. */
  file: string;
  mediaType: ImageMediaType;
}

/** Whether an account's agent takes images: ACP agents say so when they start. */
export type ImageSupport = "yes" | "ifAgentAllows" | "no";

/** "Nuevo proyecto": a new folder with git, registered with its account. */
export interface NewProjectInput {
  name: string;
  /** Absolute; created when missing. */
  parent: string;
  /** null: the default account. */
  account: string | null;
  protect: boolean;
  client: string | null;
}

/** "Pregunta rápida": a question or research outside any project. */
export interface NewConversationInput {
  prompt: string;
  /** Only `ask` or `research`. */
  mode: TaskMode;
  /** null: the default account. */
  account: string | null;
  model: string | null;
  images?: ImageInput[];
}

/** What a task shows on the task board ("Actividad"); derived by the backend. */
export type BoardState =
  | "working"
  | "plan"
  | "permission"
  | "limited"
  | "review"
  | "answered"
  | "accepted"
  | "failed"
  | "cancelled";

/** A row of the task board: running and recent tasks of every project. */
export interface ActivityItem {
  task: Task;
  state: BoardState;
  /** The phase the task is on (or ended on). */
  phase: Phase;
  /** Who works on it: "Claude Opus", "Codex". */
  agent: string;
  projectName: string;
  /** Unix seconds of its latest event. */
  updatedAt: number;
  /** Messages waiting for the end of the agent's turn. */
  queued: number;
}

/** Why queued messages wait for the user instead of going by themselves. */
export type HoldReason = "failed" | "stopped" | "limit";

/**
 * A message written while the agent works. It goes, with the rest of the
 * queue, as the next user turn when the agent's turn ends.
 */
export interface QueuedMessage {
  id: number;
  task: string;
  text: string;
  /** Unix seconds. */
  createdAt: number;
  held: HoldReason | null;
}

/**
 * A reusable request. `{name}` in the text is a placeholder the user fills
 * before sending. `id` is `builtin:<key>` for an untouched built-in example.
 */
export interface PromptTemplate {
  id: string;
  /** `null`: every project. */
  project: string | null;
  name: string;
  text: string;
  /** The built-in example this is (edited or not). */
  builtin: string | null;
  placeholders: string[];
}

export interface NewTemplateInput {
  project: string | null;
  name: string;
  text: string;
}

/** A click on a desktop notification: open this task. */
export interface OpenTaskRequest {
  taskId: string;
  project: string;
}

export type Decision = "allow" | "allowForTask" | "deny";
export type StepStatus = "pending" | "inProgress" | "done";

/** The common event language (mirrors `hedy_core::domain::AgentEvent`). */
export type AgentEvent =
  | { kind: "userMessage"; text: string; images?: Attachment[] }
  | { kind: "sessionStarted"; sessionId: string; model: string }
  | { kind: "message"; text: string }
  | { kind: "plan"; text: string; requestId: string | null }
  | { kind: "steps"; steps: { text: string; status: StepStatus }[] }
  | { kind: "permissionRequest"; requestId: string; tool: string; summary: string; input: unknown }
  | { kind: "permissionResolved"; requestId: string; decision: Decision; automatic: boolean; reason: string | null }
  | { kind: "fileEdited"; path: string }
  | { kind: "commandRun"; command: string; tests: boolean }
  | { kind: "testsRun"; command: string; passed: boolean }
  | { kind: "toolUsed"; tool: string; summary: string }
  /** Live only, never in the stored history: what the mascot shows while a tool runs. */
  | { kind: "toolStarted"; tool: string; detail: string }
  | {
      kind: "usage";
      inputTokens: number;
      outputTokens: number;
      cacheReadTokens: number;
      cacheWriteTokens: number;
      costUsd: number | null;
    }
  | { kind: "rateLimit"; status: string; utilization: number | null; resetsAt: number | null }
  | { kind: "phaseStarted"; phase: Phase; agent: string; account: string }
  | {
      kind: "limitWarning";
      account: string;
      level: "warning" | "reached";
      percent: number | null;
      resetsAt: number | null;
    }
  | { kind: "finished"; ok: boolean; summary: string; durationMs: number | null }
  | { kind: "error"; message: string };

export interface DiffLine {
  kind: "context" | "added" | "removed";
  oldLine: number | null;
  newLine: number | null;
  text: string;
}

export interface FileDiff {
  path: string;
  oldPath: string | null;
  status: "added" | "modified" | "deleted" | "renamed";
  additions: number;
  deletions: number;
  binary: boolean;
  hunks: { header: string; lines: DiffLine[] }[];
}

export interface Commit {
  hash: string;
  subject: string;
}

export type CheckArea = "cli" | "account" | "hedy" | "environment";
export type CheckLevel = "ok" | "info" | "warning" | "error";

/** One finding of the health check (`hedy doctor`). Texts come translated. */
export interface Check {
  area: CheckArea;
  /** What was checked: "Claude Code", an account, "git", a variable name. */
  subject: string;
  level: CheckLevel;
  message: string;
  /** A command (or a short instruction) that fixes it. */
  fix: string | null;
}

export interface DoctorReport {
  /** Grouped by area: cli, account, hedy, environment. */
  checks: Check[];
  warnings: number;
  errors: number;
}

/** An official CLI on this computer, for the onboarding. */
export interface DetectedCli {
  provider: ProviderId;
  installed: boolean;
  /** The login of its default folder (`unknown` when the CLI cannot tell). */
  login: Login;
  /** The Hedy account already using its default folder. */
  account: string | null;
}

// Code navigation and review: see codeTypes.ts.
export type * from "./codeTypes";
