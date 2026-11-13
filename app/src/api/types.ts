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
