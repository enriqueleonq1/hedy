import { Channel, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";

import type { HedyApi } from "./HedyApi";
import type { AgentEvent, OpenTaskRequest, QueuedMessage, Task, TerminalEvent } from "./types";

/** Tauri's `listen` is async; this returns a synchronous unsubscribe. */
function subscribe<T>(event: string, handler: (payload: T) => void): () => void {
  let unlisten: UnlistenFn | null = null;
  let cancelled = false;
  void listen<T>(event, (e) => handler(e.payload)).then((fn) => {
    if (cancelled) fn();
    else unlisten = fn;
  });
  return () => {
    cancelled = true;
    unlisten?.();
  };
}

/**
 * What may keep the window open. Tauri closes it after each of its listeners
 * unless that listener's own event was prevented, so one listener asks every
 * guard: any `false` keeps it open.
 */
const closeGuards = new Set<() => Promise<boolean>>();
let listeningForClose = false;

function listenForClose() {
  if (listeningForClose) return;
  listeningForClose = true;
  void getCurrentWindow().onCloseRequested(async (event) => {
    for (const confirm of [...closeGuards]) {
      if (!(await confirm())) return event.preventDefault();
    }
  });
}

/** `HedyApi` over Tauri commands (see `src-tauri/src/commands.rs`). */
export const tauriApi: HedyApi = {
  overview: () => invoke("overview"),

  addAccount: (input) => invoke("add_account", { input }),
  setApiKey: (id, key) => invoke("set_api_key", { id, key }),
  removeAccount: (id) => invoke("remove_account", { id }),
  renameAccount: (id, name) => invoke("rename_account", { id, name }),
  setDefaultAccount: (id) => invoke("set_default_account", { id }),
  setAccountColor: (id, color) => invoke("set_account_color", { id, color }),
  refreshPlanUsage: (id) => invoke("refresh_plan_usage", { id }),

  useAccount: (path, account, protect) =>
    invoke("use_account", { path, account, protect: protect ?? null }),
  offerableAccounts: (path) => invoke("offerable_accounts", { path }),
  setProjectColor: (path, color) => invoke("set_project_color", { path, color }),
  setProjectPhases: (path, phases) => invoke("set_project_phases", { path, phases }),
  setProjectClient: (path, client) => invoke("set_project_client", { path, client }),
  setProjectBlockedFiles: (path, patterns) => invoke("set_project_blocked_files", { path, patterns }),
  suggestedBlockedFiles: () => invoke("suggested_blocked_files"),
  setProjectPermissionMode: (path, mode) => invoke("set_project_permission_mode", { path, mode }),
  setTaskPermissions: (id, mode) => invoke("set_task_permissions", { id, mode }),
  projectStatus: (path) => invoke("project_status", { path }),
  projectUsage: (path, days) => invoke("project_usage", { path, days }),
  usageSummary: (days) => invoke("usage_summary", { days }),
  removeProject: (path) => invoke("remove_project", { path }),
  defaultProjectParent: () => invoke("default_project_parent"),
  newProjectPath: (name, parent) => invoke("new_project_path", { name, parent }),
  newProject: (input) => invoke("new_project", { input }),

  setSetting: (key, value) => invoke("set_setting", { key, value }),
  listThemes: () => invoke("list_themes"),
  saveTheme: (theme) => invoke("save_theme", { theme }),
  deleteTheme: (id) => invoke("delete_theme", { id }),
  describeTheme: (account, description, base) => invoke("describe_theme", { account, description, base }),
  openInEditor: (path) => invoke("open_in_editor", { path }),
  openLink: (url) => invoke("open_link", { url }),
  detectClis: () => invoke("detect_clis"),
  doctor: () => invoke("doctor"),
  pickFolder: async () => {
    const folder = await open({ directory: true, multiple: false });
    return typeof folder === "string" ? folder : null;
  },

  pickFile: async () => {
    const file = await open({ directory: false, multiple: false, filters: [{ name: "Markdown", extensions: ["md"] }] });
    return typeof file === "string" ? file : null;
  },

  terminalOpen: (request, { cols, rows }, onEvent) => {
    const channel = new Channel<TerminalEvent>();
    channel.onmessage = onEvent;
    return invoke("terminal_open", { request, cols, rows, onEvent: channel });
  },
  terminalWrite: (id, data) => invoke("terminal_write", { id, data }),
  terminalResize: (id, { cols, rows }) => invoke("terminal_resize", { id, cols, rows }),
  terminalClose: (id) => invoke("terminal_close", { id }),

  startTask: (input) => invoke("start_task", { input }),
  flows: (project) => invoke("flows", { project }),
  flowProblems: (project, flow) => invoke("flow_problems", { project, flow }),
  saveFlow: (project, key, flow) => invoke("save_flow", { project, key, flow }),
  deleteFlow: (project, key) => invoke("delete_flow", { project, key }),
  flowRoles: (project) => invoke("flow_roles", { project }),
  flowValues: (project) => invoke("flow_values", { project }),
  answerFlowApproval: (id, approve, feedback) => invoke("answer_flow_approval", { id, approve, feedback }),
  taskBoard: (id) => invoke("task_board", { id }),
  flowSharing: (project) => invoke("flow_sharing", { project }),
  shareFlow: (project, key) => invoke("share_flow", { project, key }),
  unshareFlow: (project, name) => invoke("unshare_flow", { project, name }),
  approveFlowCommand: (command) => invoke("approve_flow_command", { command }),
  tasks: (project) => invoke("tasks", { project }),
  taskEvents: (id) => invoke("task_events", { id }),
  startConversation: (input) => invoke("start_conversation", { input }),
  conversations: () => invoke("conversations"),
  conversationContext: (id) => invoke("conversation_context", { id }),
  summarizeConversation: (id, turns) => invoke("summarize_conversation", { id, turns }),
  continuations: (id) => invoke("continuations", { id }),
  saveConversationNote: (conversation, project, title, text, category) =>
    invoke("save_conversation_note", { conversation, project, title, text, category }),
  conversationAccounts: () => invoke("conversation_accounts"),
  deleteConversation: (id) => invoke("delete_conversation", { id }),
  activity: () => invoke("activity"),
  answerPermission: (id, requestId, decision) => invoke("answer_permission", { id, requestId, decision }),
  answerPlan: (id, requestId, approve, feedback) => invoke("answer_plan", { id, requestId, approve, feedback }),
  memory: (path) => invoke("memory", { path }),
  searchMemory: (path, query) => invoke("search_memory", { path, query }),
  askMemory: (question, scope, account) => invoke("ask_memory", { question, scope, account: account ?? null }),
  checkTaskRules: (task, account) => invoke("check_task_rules", { task, account: account ?? null }),
  memoryGraph: () => invoke("memory_graph"),
  lookInCode: (question, project, account) => invoke("look_in_code", { question, project, account: account ?? null }),
  addNote: (project, text, category) => invoke("add_note", { project, text, category }),
  editNote: (id, text, category) => invoke("edit_note", { id, text, category }),
  approveNote: (id) => invoke("approve_note", { id }),
  replaceNote: (proposal, target) => invoke("replace_note", { proposal, target }),
  forgetNote: (id) => invoke("forget_note", { id }),
  keepNote: (keep, forget) => invoke("keep_note", { keep, forget }),
  dismissContradiction: (a, b) => invoke("dismiss_contradiction", { a, b }),
  setMemoryReader: (path, account, reads) => invoke("set_memory_reader", { path, account, reads }),
  skills: (path) => invoke("skills", { path }),
  addSkill: (path, draft) => invoke("add_skill", { path, draft }),
  editSkill: (id, path, draft) => invoke("edit_skill", { id, path, draft }),
  approveSkill: (id) => invoke("approve_skill", { id }),
  rejectSkill: (id) => invoke("reject_skill", { id }),
  removeSkill: (id) => invoke("remove_skill", { id }),
  skillsOverview: () => invoke("skills_overview"),
  setSkillOutside: (id, on) => invoke("set_skill_outside", { id, on }),
  previewSkillImport: (path, source, global) => invoke("preview_skill_import", { path, source, global }),
  reviewSkillImport: (path, source, global, pick) => invoke("review_skill_import", { path, source, global, pick }),
  importSkills: (path, source, global, picks, replace) =>
    invoke("import_skills", { path, source, global, picks, replace }),
  reviewSkill: (id, path) => invoke("review_skill", { id, path }),
  exportSkills: (ids, to) => invoke("export_skills", { ids, to }),
  proposeTrialTasks: (path, id) => invoke("propose_trial_tasks", { path, id }),
  trialCheck: (path) => invoke("trial_check", { path }),
  startSkillTrial: (path, id, tasks, check) => invoke("start_skill_trial", { path, id, tasks, check }),
  cancelSkillTrial: (path, trial) => invoke("cancel_skill_trial", { path, trial }),
  takeTrialOutcome: (trial) => invoke("take_trial_outcome", { trial }),
  codeCopy: (copy) => invoke("code_copy", { copy }),
  codeTree: (copy, options) => invoke("code_tree", { copy, ignored: options?.ignored ?? false }),
  readFile: (copy, path, rev) => invoke("read_file", { copy, path, rev: rev ?? null }),
  fileDiff: (copy, path, options) => invoke("file_diff", { copy, path, options: options ?? null }),
  searchText: (copy, query, options) => invoke("search_text", { copy, query, options: options ?? null }),
  fileHistory: (copy, path, lines) => invoke("file_history", { copy, path, lines: lines ?? null }),
  blame: (copy, path, options) =>
    invoke("blame_file", { copy, path, rev: options?.rev ?? null, lines: options?.lines ?? null }),
  fileSymbols: (copy, path) => invoke("file_symbols", { copy, path }),
  openFileInEditor: (copy, path, line) => invoke("open_file_in_editor", { copy, path, line: line ?? null }),
  revealInFileManager: (copy, path) => invoke("reveal_in_file_manager", { copy, path }),
  writeFile: (copy, path, text, expected) => invoke("write_file", { copy, path, text, expected }),
  createFile: (copy, path) => invoke("create_file", { copy, path }),
  createFolder: (copy, path) => invoke("create_folder", { copy, path }),
  renamePath: (copy, from, to) => invoke("rename_path", { copy, from, to }),
  deletePath: (copy, path) => invoke("delete_path", { copy, path }),
  duplicateFile: (copy, path) => invoke("duplicate_file", { copy, path }),
  onCloseRequested: (confirm) => {
    closeGuards.add(confirm);
    listenForClose();
    return () => closeGuards.delete(confirm);
  },
  sendMessage: (id, text) => invoke("send_message", { id, text }),
  queued: (id) => invoke("queued", { id }),
  queueMessage: (id, text, images) => invoke("queue_message", { id, text, images: images ?? null }),
  imageSupport: (account) => invoke("image_support", { account }),
  projectFiles: (path) => invoke("project_files", { path }),
  attachment: (id, file) => invoke("attachment", { id, file }),
  editQueued: (message, text) => invoke("edit_queued", { message, text }),
  removeQueued: (message) => invoke("remove_queued", { message }),
  sendQueuedNow: (id) => invoke("send_queued_now", { id }),
  accountCommands: (account, project) => invoke("account_commands", { account, project }),
  templates: (project) => invoke("templates", { project }),
  allTemplates: () => invoke("all_templates"),
  addTemplate: (input) => invoke("add_template", { input }),
  updateTemplate: (id, name, text) => invoke("update_template", { id, name, text }),
  removeTemplate: (id) => invoke("remove_template", { id }),
  restoreBuiltinTemplates: () => invoke("restore_builtin_templates"),
  stopTask: (id) => invoke("stop_task", { id }),
  taskDiff: (id) => invoke("task_diff", { id }),
  taskOutputs: (id) => invoke("task_outputs", { id }),
  openOutput: (id, path) => invoke("open_output", { id, path }),
  saveOutput: (id, path, folder) => invoke("save_output", { id, path, folder }),
  acceptPreview: (id) => invoke("accept_preview", { id }),
  acceptTask: (id, message, mode) => invoke("accept_task", { id, message: message ?? null, mode: mode ?? null }),
  undoAccept: (id) => invoke("undo_accept", { id }),
  folderStatus: (path) => invoke("folder_status", { path }),
  pushFolder: (path) => invoke("push_folder", { path }),
  pullFolder: (path) => invoke("pull_folder", { path }),
  shipState: (task) => invoke("ship_state", { task }),
  pushTask: (task) => invoke("push_task", { task }),
  pullRequestDraft: (task) => invoke("pull_request_draft", { task }),
  createPullRequest: (task, input) => invoke("create_pull_request", { task, input }),
  suggestCommitMessage: (task) => invoke("suggest_commit_message", { task }),
  waitForLimit: (id) => invoke("wait_for_limit", { id }),
  cancelWait: (id) => invoke("cancel_wait", { id }),
  resumeAfterLimit: (id) => invoke("resume_after_limit", { id }),
  discardTask: (id) => invoke("discard_task", { id }),
  recentCommits: (project) => invoke("recent_commits", { project }),
  handoffOptions: (id) => invoke("handoff_options", { id }),
  handoffTask: (id, account, model) => invoke("handoff_task", { id, account, model }),
  onTaskEvent: (listener) =>
    subscribe<{ taskId: string; event: AgentEvent }>("task-event", ({ taskId, event }) => listener(taskId, event)),
  onTaskUpdated: (listener) => subscribe<Omit<Task, "live">>("task-updated", listener),
  onTaskQueue: (listener) =>
    subscribe<{ taskId: string; queue: QueuedMessage[] }>("task-queue", ({ taskId, queue }) => listener(taskId, queue)),
  onOpenTask: (listener) => subscribe<OpenTaskRequest>("open-task", listener),

  syncStatus: () => invoke("sync_status"),
  syncDefaultName: () => invoke("sync_default_name"),
  syncRepoIsPublic: (url) => invoke("sync_repo_is_public", { url }),
  syncSetup: (url, name) => invoke("sync_setup", { url, name }),
  syncJoin: (url, words, name) => invoke("sync_join", { url, words, name }),
  syncNow: () => invoke("sync_now"),
  syncRemoveDevice: (id) => invoke("sync_remove_device", { id }),
  syncLeave: () => invoke("sync_leave"),
  syncConflicts: () => invoke("sync_conflicts"),
  resolveSyncConflict: (id, useOther) => invoke("resolve_sync_conflict", { id, useOther }),
  memoryHome: (project) => invoke("memory_home", { path: project }),
  memoryHomes: () => invoke("memory_homes"),
  setMemoryHome: (project, home) => invoke("set_memory_home", { path: project, home }),
  answerOwner: (owner, mine) => invoke("answer_owner", { owner, mine }),
  forgetOwner: (owner) => invoke("forget_owner", { owner }),
  onSyncUpdated: (listener) => subscribe<null>("sync-updated", () => listener()),

  teams: () => invoke("teams"),
  teamStatus: (team) => invoke("team_status", { team }),
  teamCreate: (url, name, me, pc) => invoke("team_create", { url, name, me, pc }),
  teamInvite: (team, admin, mine) => invoke("team_invite", { team, admin, mine }),
  teamCancelInvite: (team, number) => invoke("team_cancel_invite", { team, number }),
  teamDismissFailedInvite: (team, number) => invoke("team_dismiss_failed_invite", { team, number }),
  teamJoin: (url, code, me, pc) => invoke("team_join", { url, code, me, pc }),
  teamSync: (team) => invoke("team_sync", { team }),
  teamLinkProject: (team, project) => invoke("team_link_project", { team, path: project }),
  teamNotes: (team) => invoke("team_notes", { team }),
  teamAddNote: (team, project, text, category) =>
    invoke("team_add_note", { team, project, text, category, topic: null }),
  teamEditNote: (team, uid, text) => invoke("team_edit_note", { team, uid, text, category: null }),
  teamPublishNote: (team, noteId, everyone) => invoke("team_publish_note", { team, noteId, everyone }),
  teamSuggestions: (team) => invoke("team_suggestions", { team }),
  teamAcceptSuggestion: (team, uid) => invoke("team_accept_suggestion", { team, uid }),
  teamRejectSuggestion: (team, uid) => invoke("team_reject_suggestion", { team, uid }),
  teamSetRole: (team, person, admin) => invoke("team_set_role", { team, person, admin }),
  teamRemovePerson: (team, person) => invoke("team_remove_person", { team, person }),
  teamRemovePc: (team, device) => invoke("team_remove_pc", { team, device }),
  teamRename: (team, name) => invoke("team_rename", { team, name }),
  teamRenamePerson: (team, person, name) => invoke("team_rename_person", { team, person, name }),
  teamLeave: (team, here) => invoke("team_leave", { team, here }),
};

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
