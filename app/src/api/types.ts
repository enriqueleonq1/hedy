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
  /** Hedy-managed folder; null for API keys and the CLI's default folder. */
  folder: string | null;
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
}

export type ThemeSetting = "system" | "light" | "dark";

export interface Overview {
  userName: string;
  theme: ThemeSetting;
  dataDir: string;
  defaultAccount: string | null;
  binaries: Record<CliProviderId, string | null>;
  accounts: Account[];
  projects: Project[];
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
  | "binary.gemini";

export type TerminalRequest =
  | { kind: "login"; account: string }
  | { kind: "run"; path: string }
  | { kind: "shell"; path: string | null };

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
