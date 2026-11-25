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
import type {
  AcceptMode,
  AcceptPreview,
  FolderStatus,
  NewPullRequestInput,
  PullOutcome,
  PullRequest,
  PullRequestDraft,
  PushOutcome,
  ShipState,
  ImageInput,
  ImageSupport,
  AskScope,
  CodeLookup,
  MemoryAnswer,
  RuleCheck,
  MemoryGraph,
  MemoryHome,
  MemoryHomeKind,
  SyncConflict,
  SyncReport,
  SyncStatus,
  TeamCreated,
  TeamInvite,
  TeamJoin,
  TeamNote,
  TeamStatus,
  TeamSuggestion,
  TeamSummary,
  TeamWrite,
  ActivityItem,
  AgentEvent,
  CustomTheme,
  ThemeBase,
  ThemeDraft,
  ConversationContext,
  Commit,
  Decision,
  DetectedCli,
  DoctorReport,
  FileDiff,
  OutputFile,
  PermissionMode,
  WorkingStyle,
  MemoryView,
  ExportReport,
  ImportPick,
  ImportPreview,
  ImportReport,
  SkillChange,
  SkillDraft,
  SkillFiles,
  SkillOverview,
  SkillReview,
  SkillsView,
  StartedTrial,
  TrialCheck,
  TrialOutcome,
  Note,
  NoteCategory,
  PhasePlan,
  ProjectStatus,
  UsagePoint,
  UsageSummary,
  NewAccountInput,
  NewConversationInput,
  NewProjectInput,
  NewTaskInput,
  NewTemplateInput,
  BoardEntry,
  Flow,
  FlowDefaults,
  FlowEntry,
  FlowIssue,
  FlowValue,
  RoleEntry,
  TeamShare,
  OpenTaskRequest,
  PromptTemplate,
  AccountCommands,
  QueuedMessage,
  Overview,
  PlanState,
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
  /** Display name only; the id stays. */
  renameAccount(id: string, name: string): Promise<void>;
  setDefaultAccount(id: string | null): Promise<void>;
  setAccountColor(id: string, color: string): Promise<void>;
  /**
   * Asks the provider for a new plan-usage reading (at most every few minutes
   * per account; otherwise the cached one). Failures come back as `problem`.
   */
  refreshPlanUsage(id: string): Promise<PlanState>;

  useAccount(path: string, account: string, protect?: boolean): Promise<void>;
  offerableAccounts(path: string): Promise<string[]>;
  setProjectColor(path: string, color: string | null): Promise<void>;
  setProjectPhases(path: string, phases: PhasePlan): Promise<void>;
  /** Empty or null clears it. */
  setProjectClient(path: string, client: string | null): Promise<void>;
  /** "Archivos que la IA no lee": the project's whole list. */
  setProjectBlockedFiles(path: string, patterns: string[]): Promise<void>;
  /** What "Agregar los de siempre" adds (.env, keys, credentials). */
  suggestedBlockedFiles(): Promise<string[]>;
  /** The permission mode a project's new tasks start with. */
  setProjectPermissionMode(path: string, mode: PermissionMode): Promise<void>;
  /** How a project's «Cambiar código» tasks work with one AI. */
  setProjectWorking(path: string, working: WorkingStyle): Promise<void>;
  /** The tests' command Hedy finds in a project (`null`: none it knows). */
  detectedTestsCommand(path: string): Promise<string | null>;
  /** A task's permission mode, changed from the composer. */
  setTaskPermissions(id: string, mode: PermissionMode): Promise<void>;
  projectStatus(path: string): Promise<ProjectStatus>;
  /** Usage records of the last `days` days, oldest first. */
  projectUsage(path: string, days: number): Promise<UsagePoint[]>;
  usageSummary(days: number): Promise<UsageSummary>;
  removeProject(path: string): Promise<void>;
  /** Where "Nuevo proyecto" proposes to create it. */
  defaultProjectParent(): Promise<string | null>;
  /** The folder a new project would get; rejects with why it cannot be created there. */
  newProjectPath(name: string, parent: string): Promise<string>;
  /** Creates the folder (never an existing one) with git and registers it. Returns its path. */
  newProject(input: NewProjectInput): Promise<string>;

  setSetting(key: SettingKey, value: string | null): Promise<void>;

  /** The user's own themes ("Tuyos"). */
  listThemes(): Promise<CustomTheme[]>;
  /** Saves a theme (`id: null` makes a new one; an id renames or replaces it). */
  saveTheme(theme: {
    id: string | null;
    name: string;
    base: ThemeBase;
    colors: Record<string, string>;
    origin: CustomTheme["origin"];
  }): Promise<CustomTheme>;
  deleteTheme(id: string): Promise<void>;
  /**
   * "Describir con IA": a short question to the user's own `account` (a few
   * tokens) that turns a description into a seed. `base: null` lets it choose.
   * Never another account: a limit comes back as an error.
   */
  describeTheme(account: string, description: string, base: ThemeBase | null): Promise<ThemeDraft>;

  openInEditor(path: string): Promise<void>;
  /** Opens an https page, or a local dev server (the preview), in the browser. */
  openLink(url: string): Promise<void>;
  /** The official CLIs installed here and their default logins. */
  detectClis(): Promise<DetectedCli[]>;
  /** Health check of the CLIs, the accounts, Hedy and the environment. */
  doctor(): Promise<DoctorReport>;
  pickFolder(): Promise<string | null>;
  /** A Markdown file (a skill's SKILL.md), or null when cancelled. */
  pickFile(): Promise<string | null>;

  terminalOpen(
    request: TerminalRequest,
    size: { cols: number; rows: number },
    onEvent: (event: TerminalEvent) => void,
  ): Promise<number>;
  terminalWrite(id: number, data: string): Promise<void>;
  terminalResize(id: number, size: { cols: number; rows: number }): Promise<void>;
  terminalClose(id: number): Promise<void>;

  startTask(input: NewTaskInput): Promise<Task>;

  /** Hedy's templates, then the project's flows and the global ones. */
  flows(project: string | null): Promise<FlowEntry[]>;
  /** The flow a project's «Cambiar código» tasks start with, and the one that follows their end. */
  flowDefaults(project: string): Promise<FlowDefaults>;
  /** Sets the flow they start with, by key; `null`: one AI. */
  setDefaultFlow(project: string, flow: string | null): Promise<void>;
  /** Why a flow being edited could not run yet (empty: it can). */
  flowProblems(project: string | null, flow: Flow): Promise<FlowIssue[]>;
  /** Updates flow `key` (renamed or not, where it was saved); null: a new one for `project`. */
  saveFlow(project: string | null, key: string | null, flow: Flow): Promise<FlowEntry>;
  deleteFlow(project: string | null, key: string): Promise<void>;
  /** The roles a flow can place: Hedy's and the user's. */
  flowRoles(project: string | null): Promise<RoleEntry[]>;
  /** The project's flows with enough closed tasks, against its tasks with one agent. */
  flowValues(project: string): Promise<FlowValue[]>;
  /** The user's answer at a flow's approval step: no goes back with `feedback`. */
  answerFlowApproval(id: string, approve: boolean, feedback: string | null): Promise<void>;
  /** A flow task's board, oldest first. */
  taskBoard(id: string): Promise<BoardEntry[]>;
  /** The team the project's flows can be shared with (null: none). */
  flowSharing(project: string): Promise<TeamShare | null>;
  /** Shares one of the user's flows with the project's team (admins only); returns its name. */
  shareFlow(project: string, key: string): Promise<string>;
  /** Stops sharing the team's flow named `name`. */
  unshareFlow(project: string, name: string): Promise<void>;
  /** Allows a team flow's test command on this PC. */
  approveFlowCommand(command: string): Promise<void>;
  /** Newest first. */
  tasks(project: string): Promise<Task[]>;
  taskEvents(id: string): Promise<AgentEvent[]>;
  /** A question outside any project, in its own empty folder. */
  startConversation(input: NewConversationInput): Promise<Task>;
  /** Every conversation outside a project, newest first. */
  conversations(): Promise<Task[]>;
  /** Accounts a conversation may use (none reserved by a protected project). */
  conversationAccounts(): Promise<string[]>;
  /** Deletes a conversation, its history and its folder. */
  deleteConversation(id: string): Promise<void>;
  /** Its questions with their final answers, a project name, and whether it is still answering. */
  conversationContext(id: string): Promise<ConversationContext>;
  /** "Resumir con IA": the conversation's own account sums up the kept turns (spends a few tokens). */
  summarizeConversation(id: string, turns: number[]): Promise<string>;
  /** The tasks that continue a conversation, newest first. */
  continuations(id: string): Promise<Task[]>;
  /** "Guardar como nota del proyecto": a note of the user's (approved), title first. */
  saveConversationNote(
    conversation: string,
    project: string,
    title: string,
    text: string,
    category: NoteCategory,
  ): Promise<Note>;
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
  queueMessage(id: string, text: string, images?: ImageInput[]): Promise<QueuedMessage[]>;
  /**
   * A question ("Solo preguntar", "Investigar") goes on to change code in the
   * same task, starting with `text`, with what was said as context.
   */
  changeCode(id: string, text: string, images: ImageInput[], planFirst: boolean): Promise<Task>;
  /** The project's files for `@` mentions, relative and sorted (never `.env` or keys). */
  projectFiles(project: string): Promise<string[]>;
  /** Whether the agent of `account` takes images with a prompt. */
  imageSupport(account: string): Promise<ImageSupport>;
  /** Whether the agent of `account` reads PDFs with a prompt. */
  pdfSupport(account: string): Promise<ImageSupport>;
  /** An image of a task's history as a data URL; `null` when it was deleted. */
  attachment(taskId: string, file: string): Promise<string | null>;
  editQueued(message: number, text: string): Promise<QueuedMessage[]>;
  removeQueued(message: number): Promise<QueuedMessage[]>;
  /** Releases a queue on hold and sends it now, unless the agent is mid-turn. */
  sendQueuedNow(id: string): Promise<QueuedMessage[]>;

  /**
   * The slash commands `account` offers in `project` (`null`: outside any), as
   * its CLI last reported them; unknown until it answers once in Hedy.
   */
  accountCommands(account: string, project: string | null): Promise<AccountCommands>;
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
  /** What a question wrote in its own folder, outside the project. */
  taskOutputs(id: string): Promise<OutputFile[]>;
  /** Opens one of those files with the system's app (a page in the browser). */
  openOutput(id: string, path: string): Promise<void>;
  /** Copies one into `folder` without overwriting anything; where it went. */
  saveOutput(id: string, path: string, folder: string): Promise<string>;
  /** What accepting would do (branch, files, lines, message, tests); changes nothing. */
  acceptPreview(id: string): Promise<AcceptPreview>;
  /**
   * Commits with `message` (the proposed one when empty) and merges into the
   * base branch, or (`branch`) keeps it on the task's branch for a PR.
   */
  acceptTask(id: string, message?: string | null, mode?: AcceptMode): Promise<void>;
  /**
   * Takes the merge back out only when that is safe; otherwise rejects with
   * why. A task kept on its branch is reopened on that branch.
   */
  undoAccept(id: string): Promise<void>;
  /** Branch, changes, ahead/behind, remote host and `gh` of a folder (local; `gh` cached). */
  folderStatus(path: string): Promise<FolderStatus>;
  /** Pushes the folder's branch (sets its upstream when missing). Never forced. */
  pushFolder(path: string): Promise<PushOutcome>;
  /** `git pull --ff-only`; a divergence comes back as a failure. */
  pullFolder(path: string): Promise<PullOutcome>;
  shipState(taskId: string): Promise<ShipState>;
  /** Pushes the branch that carries an accepted task's work. */
  pushTask(taskId: string): Promise<PushOutcome>;
  /** The PR Hedy proposes, from local facts (request, summary, files, tests). */
  pullRequestDraft(taskId: string): Promise<PullRequestDraft>;
  createPullRequest(taskId: string, input: NewPullRequestInput): Promise<PullRequest>;
  /** A one-line commit message from the task's own account and model (spends a few tokens). */
  suggestCommitMessage(taskId: string): Promise<string>;
  /** "Esperar": Hedy resumes the task on the same account when its limit renews. */
  waitForLimit(id: string): Promise<void>;
  cancelWait(id: string): Promise<void>;
  /** "Continuar": resume a task that waited for its limit, on the same account. */
  resumeAfterLimit(id: string): Promise<void>;
  discardTask(id: string): Promise<void>;
  recentCommits(project: string): Promise<Commit[]>;
  /** Accounts the task may move to: another provider or an API key. */
  handoffOptions(id: string): Promise<string[]>;
  handoffTask(id: string, account: string, model: string | null): Promise<void>;
  memory(project: string): Promise<MemoryView>;
  searchMemory(project: string, query: string): Promise<Note[]>;
  /** A question answered only from the notes, each claim citing its own (docs/decisions.md §36). */
  askMemory(question: string, scope: AskScope, account?: string | null): Promise<MemoryAnswer>;
  /** Every note this person can read here, as a graph (only shown on this PC). */
  memoryGraph(): Promise<MemoryGraph>;
  /**
   * "Revisar con tus reglas": a task's changes against the written rules of
   * its project, each break on an added line citing its note (docs/decisions.md §37).
   * Spends a few tokens of the project's account, only with rules and lines to check.
   */
  checkTaskRules(task: string, account?: string | null): Promise<RuleCheck>;
  /** "Buscar en el código": the project's files answer, and a note is proposed (never approved by itself). */
  lookInCode(question: string, project: string, account?: string | null): Promise<CodeLookup>;
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
  /** Every skill (global ones first, then each project's), measured, with its last trial. */
  skillsOverview(): Promise<SkillOverview[]>;
  /** "Usar también fuera de Hedy" for an approved project skill. */
  setSkillOutside(id: number, on: boolean): Promise<SkillFiles>;
  /** The skills in `source` (a SKILL.md, a folder or a project), ready to import: nothing is saved. */
  previewSkillImport(project: string, source: string, global: boolean): Promise<ImportPreview>;
  /** What the AI thinks of one skill to import (by its file), against the project's notes. */
  reviewSkillImport(project: string, source: string, global: boolean, pick: ImportPick): Promise<SkillReview>;
  /** Imports the picked files of `source`, read again (one whose text changed stays out); `replace` names approved skills to update. */
  importSkills(project: string, source: string, global: boolean, picks: ImportPick[], replace: string[]): Promise<ImportReport>;
  /** What the AI thinks of a skill Hedy has, against `project`'s notes. */
  reviewSkill(id: number, project: string): Promise<SkillReview>;
  /** Writes approved skills as `<to>/<name>/SKILL.md`; nothing is replaced. */
  exportSkills(ids: number[], to: string): Promise<ExportReport>;
  /** Up to three test tasks the project's account proposes from the skill alone. */
  proposeTrialTasks(project: string, id: number): Promise<string[]>;
  /** The command Hedy would run in each copy, and whether it runs isolated. */
  trialCheck(project: string): Promise<TrialCheck>;
  /** Each task twice, with and without the skill; `check: null` runs none. */
  startSkillTrial(project: string, id: number, tasks: string[], check: string | null): Promise<StartedTrial>;
  /** Discards a trial's runs; no result is kept. */
  cancelSkillTrial(project: string, trial: string): Promise<void>;
  /** How a trial ended, once; null while it runs. */
  takeTrialOutcome(trial: string): Promise<TrialOutcome | null>;

  // ------------------------------------------------ code navigation (read-only)
  // Every call names its copy (`CodeCopy`); paths are relative to the copy and
  // rejected when they would leave it (absolute, `..`, `.git`). See codeTypes.ts.

  /** What a copy is (for the header of a code view), without reading files. */
  codeCopy(copy: CodeCopy): Promise<CopyInfo>;
  /** Every file with its status against the copy's base; `ignored` adds git-ignored ones. */
  codeTree(copy: CodeCopy, options?: { ignored?: boolean }): Promise<CodeTree>;
  /** A file of the copy, or of `rev` (`"base"` = before the AI / `HEAD`). */
  readFile(copy: CodeCopy, path: string, rev?: Rev): Promise<FileContent>;
  /** One file's diff from `against` (default `"base"`) to the copy. */
  fileDiff(copy: CodeCopy, path: string, options?: DiffOptions): Promise<CodeDiff>;
  /** Text search (`git grep`) over the copy, untracked files included; empty query → no results. */
  searchText(copy: CodeCopy, query: string, options?: SearchOptions): Promise<SearchResults>;
  /** Commits (following renames) and the project's tasks that touched the file; `lines` narrows to a range. */
  fileHistory(copy: CodeCopy, path: string, lines?: LineRange): Promise<FileHistory>;
  /** Who wrote each line, of the copy's file or of `rev`; `lines` narrows to a range. Rejects for locked files. */
  blame(copy: CodeCopy, path: string, options?: { rev?: Rev; lines?: LineRange }): Promise<Blame>;
  /** Functions, types and headings of a file (for `@` in the palette); empty when not readable text. */
  fileSymbols(copy: CodeCopy, path: string): Promise<CodeSymbol[]>;
  /** Opens the file in VS Code at `line` (`code -g`), or with the system's default app. */
  openFileInEditor(copy: CodeCopy, path: string, line?: number): Promise<void>;
  /** Opens the file's folder in the system file manager (selects it on macOS). */
  revealInFileManager(copy: CodeCopy, path: string): Promise<void>;

  // ------------------------------------------------ light editing
  // Only live copies (the folder, an open task's worktree), never while the
  // task's agent works on it, never protected files or `.git`.

  /** Saves `text` when the file is still the `expected` version (else a conflict, nothing written). */
  writeFile(copy: CodeCopy, path: string, text: string, expected: string): Promise<WriteResult>;
  /** Creates an empty file (and the folders above it); rejects when something is there. */
  createFile(copy: CodeCopy, path: string): Promise<{ version: string; size: number }>;
  createFolder(copy: CodeCopy, path: string): Promise<void>;
  /** Renames or moves a file or folder; rejects when `to` exists. */
  renamePath(copy: CodeCopy, from: string, to: string): Promise<void>;
  /** Moves a file or folder to the system's trash. */
  deletePath(copy: CodeCopy, path: string): Promise<void>;
  /** Copies a file next to itself (`x copia.py`); resolves to the new path. */
  duplicateFile(copy: CodeCopy, path: string): Promise<string>;
  /**
   * The window is about to close: `confirm` decides (e.g. after asking about
   * unsaved changes); resolving `false` keeps it open.
   */
  onCloseRequested(confirm: () => Promise<boolean>): Unsubscribe;

  // ------------------------------------------------ personal sync (§34)

  /** `null` while the sync is not set up. */
  syncStatus(): Promise<SyncStatus | null>;
  /** The name this PC takes by default (the machine's). */
  syncDefaultName(): Promise<string>;
  /** Whether anyone can read the repo without credentials; rejects a URL with a token. */
  syncRepoIsPublic(url: string): Promise<boolean>;
  /** Starts the sync with an empty repo; resolves to the 12 recovery words, shown once. */
  syncSetup(url: string, name: string): Promise<string>;
  /** Joins with the recovery words and brings everything in. */
  syncJoin(url: string, words: string, name: string): Promise<SyncReport>;
  syncNow(): Promise<SyncReport>;
  /** Removes another PC: it cannot read what is written from now on. Its last changes are taken over here. */
  syncRemoveDevice(id: string): Promise<SyncReport>;
  /** This PC stops syncing (its notes stay). Resolves whether it was set up. */
  syncLeave(): Promise<boolean>;
  syncConflicts(): Promise<SyncConflict[]>;
  /** Keeps the version in use, or (`useOther`) the other one; it goes out with the next sync. */
  resolveSyncConflict(id: number, useOther: boolean): Promise<void>;
  memoryHome(project: string): Promise<MemoryHome>;
  /** Every project's. */
  memoryHomes(): Promise<MemoryHome[]>;
  /** `null`: follow the repo's owner again. */
  setMemoryHome(project: string, home: MemoryHomeKind | null): Promise<void>;
  /** "¿La memoria de … es solo tuya?": one answer per owner, on this PC. */
  answerOwner(owner: string, mine: boolean): Promise<void>;
  forgetOwner(owner: string): Promise<void>;
  /** The sync that runs on its own finished (it may have brought things in). */
  onSyncUpdated(listener: () => void): Unsubscribe;

  // ------------------------------------------------ team memory (§35)

  teams(): Promise<TeamSummary[]>;
  teamStatus(team: string): Promise<TeamStatus>;
  /** A team in an empty repo, with this PC as its first admin. */
  teamCreate(url: string, name: string, me: string, pc: string): Promise<TeamCreated>;
  /** A code that works once, for 48 h: a new person (`admin` for an admin), or another PC of this person (`mine`). */
  teamInvite(team: string, admin: boolean, mine: boolean): Promise<TeamInvite>;
  teamCancelInvite(team: string, number: number): Promise<void>;
  teamDismissFailedInvite(team: string, number: number): Promise<void>;
  teamJoin(url: string, code: string, me: string, pc: string): Promise<TeamJoin>;
  /** One sync of the team (or a look, while joining). */
  teamSync(team: string): Promise<SyncReport>;
  /** The project's memory becomes the team's. */
  teamLinkProject(team: string, project: string): Promise<void>;
  teamNotes(team: string): Promise<TeamNote[]>;
  /** To a project's notes, or (`project: null`) to the team layer, where a member's becomes a suggestion. */
  teamAddNote(team: string, project: string | null, text: string, category: NoteCategory): Promise<TeamWrite>;
  /** `text: null` removes it; someone else's note gets a suggestion instead. */
  teamEditNote(team: string, uid: string, text: string | null): Promise<TeamWrite>;
  /** Moves a note of this PC to the team (its project's notes, or the team layer). */
  teamPublishNote(team: string, noteId: number, everyone: boolean): Promise<TeamWrite>;
  teamSuggestions(team: string): Promise<TeamSuggestion[]>;
  teamAcceptSuggestion(team: string, uid: string): Promise<void>;
  teamRejectSuggestion(team: string, uid: string): Promise<void>;
  teamSetRole(team: string, person: string, admin: boolean): Promise<void>;
  /** Their PCs read nothing new; their notes stay with their name. */
  teamRemovePerson(team: string, person: string): Promise<void>;
  teamRemovePc(team: string, device: string): Promise<void>;
  teamRename(team: string, name: string): Promise<void>;
  teamRenamePerson(team: string, person: string, name: string): Promise<void>;
  /** Asks to be taken out (an admin's PC waits), or (`here`) only forgets it on this PC. */
  teamLeave(team: string, here: boolean): Promise<void>;

  /** Live events of every task. */
  onTaskEvent(listener: (taskId: string, event: AgentEvent) => void): Unsubscribe;
  onTaskUpdated(listener: (task: Omit<Task, "live">) => void): Unsubscribe;
  /** A task's queue changed: what is left, oldest first. */
  onTaskQueue(listener: (taskId: string, queue: QueuedMessage[]) => void): Unsubscribe;
  /** The user clicked one of Hedy's desktop notifications. */
  onOpenTask(listener: (request: OpenTaskRequest) => void): Unsubscribe;
}
