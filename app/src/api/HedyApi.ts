import type {
  ActivityItem,
  AgentEvent,
  Commit,
  Decision,
  DetectedCli,
  FileDiff,
  MemoryView,
  Note,
  NoteCategory,
  PhasePlan,
  ProjectStatus,
  UsagePoint,
  UsageSummary,
  NewAccountInput,
  NewTaskInput,
  OpenTaskRequest,
  Overview,
  SettingKey,
  Task,
  TerminalEvent,
  TerminalRequest,
} from "./types";

export type Unsubscribe = () => void;

/**
 * Everything the UI needs from the backend. Components depend on this
 * interface only; `tauriApi` implements it for the app and `MockApi` for tests
 * and for running the UI in a plain browser.
 */
export interface HedyApi {
  overview(): Promise<Overview>;

  addAccount(input: NewAccountInput): Promise<string>;
  setApiKey(id: string, key: string): Promise<void>;
  removeAccount(id: string): Promise<void>;
  setDefaultAccount(id: string | null): Promise<void>;
  setAccountColor(id: string, color: string): Promise<void>;

  useAccount(path: string, account: string, protect?: boolean): Promise<void>;
  offerableAccounts(path: string): Promise<string[]>;
  setProjectColor(path: string, color: string | null): Promise<void>;
  setProjectPhases(path: string, phases: PhasePlan): Promise<void>;
  /** Empty or null clears it. */
  setProjectClient(path: string, client: string | null): Promise<void>;
  projectStatus(path: string): Promise<ProjectStatus>;
  /** Usage records of the last `days` days, oldest first. */
  projectUsage(path: string, days: number): Promise<UsagePoint[]>;
  usageSummary(days: number): Promise<UsageSummary>;
  removeProject(path: string): Promise<void>;

  setSetting(key: SettingKey, value: string | null): Promise<void>;
  openInEditor(path: string): Promise<void>;
  /** Opens an https page in the browser. */
  openLink(url: string): Promise<void>;
  /** The official CLIs installed here and their default logins. */
  detectClis(): Promise<DetectedCli[]>;
  pickFolder(): Promise<string | null>;

  terminalOpen(
    request: TerminalRequest,
    size: { cols: number; rows: number },
    onEvent: (event: TerminalEvent) => void,
  ): Promise<number>;
  terminalWrite(id: number, data: string): Promise<void>;
  terminalResize(id: number, size: { cols: number; rows: number }): Promise<void>;
  terminalClose(id: number): Promise<void>;

  startTask(input: NewTaskInput): Promise<Task>;
  /** Newest first. */
  tasks(project: string): Promise<Task[]>;
  taskEvents(id: string): Promise<AgentEvent[]>;
  /** The task board: every project's running and recent tasks, what needs the user first. */
  activity(): Promise<ActivityItem[]>;
  answerPermission(id: string, requestId: string, decision: Decision): Promise<void>;
  answerPlan(id: string, requestId: string, approve: boolean, feedback: string | null): Promise<void>;
  sendMessage(id: string, text: string): Promise<void>;
  stopTask(id: string): Promise<void>;
  taskDiff(id: string): Promise<FileDiff[]>;
  acceptTask(id: string): Promise<void>;
  discardTask(id: string): Promise<void>;
  recentCommits(project: string): Promise<Commit[]>;
  /** Accounts the task may move to: another provider or an API key. */
  handoffOptions(id: string): Promise<string[]>;
  handoffTask(id: string, account: string, model: string | null): Promise<void>;
  memory(project: string): Promise<MemoryView>;
  searchMemory(project: string, query: string): Promise<Note[]>;
  /** A note written by the user; `project: null` makes it global. */
  addNote(project: string | null, text: string, category: NoteCategory): Promise<Note>;
  editNote(id: number, text: string, category: NoteCategory): Promise<Note>;
  approveNote(id: number): Promise<void>;
  /** Forgets a note, or rejects a proposal. */
  forgetNote(id: number): Promise<void>;
  /** Resolves a contradiction. */
  keepNote(keep: number, forget: number): Promise<void>;
  dismissContradiction(a: number, b: number): Promise<void>;
  setMemoryReader(project: string, account: string, reads: boolean): Promise<void>;

  /** Live events of every task. */
  onTaskEvent(listener: (taskId: string, event: AgentEvent) => void): Unsubscribe;
  onTaskUpdated(listener: (task: Omit<Task, "live">) => void): Unsubscribe;
  /** The user clicked one of Hedy's desktop notifications. */
  onOpenTask(listener: (request: OpenTaskRequest) => void): Unsubscribe;
}
