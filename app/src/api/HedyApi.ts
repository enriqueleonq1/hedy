import type {
  ActivityItem,
  AgentEvent,
  Commit,
  Decision,
  DetectedCli,
  DoctorReport,
  FileDiff,
  MemoryView,
  SkillChange,
  SkillDraft,
  SkillFiles,
  SkillsView,
  Note,
  NoteCategory,
  PhasePlan,
  ProjectStatus,
  UsagePoint,
  UsageSummary,
  NewAccountInput,
  NewTaskInput,
  NewTemplateInput,
  OpenTaskRequest,
  PromptTemplate,
  QueuedMessage,
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
  /** Health check of the CLIs, the accounts, Hedy and the environment. */
  doctor(): Promise<DoctorReport>;
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
  /** The messages waiting for the end of the agent's turn, oldest first. */
  queued(id: string): Promise<QueuedMessage[]>;
  /**
   * A follow-up: queued while the agent's turn runs (or a card waits), sent
   * now when it is idle. Returns what is left in the queue.
   */
  queueMessage(id: string, text: string): Promise<QueuedMessage[]>;
  editQueued(message: number, text: string): Promise<QueuedMessage[]>;
  removeQueued(message: number): Promise<QueuedMessage[]>;
  /** Releases a queue on hold and sends it now, unless the agent is mid-turn. */
  sendQueuedNow(id: string): Promise<QueuedMessage[]>;

  /** What the composer of `project` offers: its templates, then the global ones. */
  templates(project: string | null): Promise<PromptTemplate[]>;
  /** Every template, of every project. */
  allTemplates(): Promise<PromptTemplate[]>;
  addTemplate(input: NewTemplateInput): Promise<PromptTemplate>;
  updateTemplate(id: string, name: string, text: string): Promise<PromptTemplate>;
  removeTemplate(id: string): Promise<void>;
  /** Brings back the built-in examples as they came. */
  restoreBuiltinTemplates(): Promise<void>;
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
  /** Approves a proposal; under the topic of an approved note, it replaces that note. */
  approveNote(id: number): Promise<void>;
  /** "Reemplazar esa": the proposal becomes the new text of an approved note. */
  replaceNote(proposal: number, target: number): Promise<void>;
  /** Forgets a note, or rejects a proposal. */
  forgetNote(id: number): Promise<void>;
  /** Resolves a contradiction. */
  keepNote(keep: number, forget: number): Promise<void>;
  dismissContradiction(a: number, b: number): Promise<void>;
  setMemoryReader(project: string, account: string, reads: boolean): Promise<void>;
  skills(project: string): Promise<SkillsView>;
  /** A skill written by the user: approved right away. */
  addSkill(project: string, draft: SkillDraft): Promise<SkillChange>;
  /** Edits a proposal (before approving it) or an approved skill seen from `project`. */
  editSkill(id: number, project: string, draft: SkillDraft): Promise<SkillChange>;
  /** Approves a proposal; with the name of an approved skill, it updates that one. */
  approveSkill(id: number): Promise<SkillChange>;
  rejectSkill(id: number): Promise<void>;
  /** Deletes a skill and the files Hedy wrote for it. */
  removeSkill(id: number): Promise<SkillFiles>;

  /** Live events of every task. */
  onTaskEvent(listener: (taskId: string, event: AgentEvent) => void): Unsubscribe;
  onTaskUpdated(listener: (task: Omit<Task, "live">) => void): Unsubscribe;
  /** A task's queue changed: what is left, oldest first. */
  onTaskQueue(listener: (taskId: string, queue: QueuedMessage[]) => void): Unsubscribe;
  /** The user clicked one of Hedy's desktop notifications. */
  onOpenTask(listener: (request: OpenTaskRequest) => void): Unsubscribe;
}
