// Mirrors the DTOs in `src-tauri/src/dto.rs`.

export type ProviderId = "claude" | "codex" | "gemini" | "kimi";
export type CliProviderId = Exclude<ProviderId, "kimi">;

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
}

export interface MemoryView {
  notes: Note[];
  proposed: Note[];
  contradictions: [Note, Note][];
  stale: Note[];
  usedLastTask: Note[];
  readers: { account: string; reads: boolean }[];
  protected: boolean;
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
  | "binary.claude"
  | "binary.codex"
  | "binary.gemini"
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
}

// ---------------------------------------------------------------- tasks

export type TaskMode = "project" | "ask" | "research";
type TaskStatus ="running" | "waitingApproval" | "review" | "accepted" | "discarded" | "failed";

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
}

export interface NewTaskInput {
  project: string;
  prompt: string;
  mode: TaskMode;
  model: string | null;
  planFirst: boolean;
}

export type Decision = "allow" | "allowForTask" | "deny";
export type StepStatus = "pending" | "inProgress" | "done";

/** The common event language (mirrors `hedy_core::domain::AgentEvent`). */
export type AgentEvent =
  | { kind: "userMessage"; text: string }
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
