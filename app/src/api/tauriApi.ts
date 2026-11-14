import { Channel, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
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

/** `HedyApi` over Tauri commands (see `src-tauri/src/commands.rs`). */
export const tauriApi: HedyApi = {
  overview: () => invoke("overview"),

  addAccount: (input) => invoke("add_account", { input }),
  setApiKey: (id, key) => invoke("set_api_key", { id, key }),
  removeAccount: (id) => invoke("remove_account", { id }),
  setDefaultAccount: (id) => invoke("set_default_account", { id }),
  setAccountColor: (id, color) => invoke("set_account_color", { id, color }),

  useAccount: (path, account, protect) =>
    invoke("use_account", { path, account, protect: protect ?? null }),
  offerableAccounts: (path) => invoke("offerable_accounts", { path }),
  setProjectColor: (path, color) => invoke("set_project_color", { path, color }),
  setProjectPhases: (path, phases) => invoke("set_project_phases", { path, phases }),
  setProjectClient: (path, client) => invoke("set_project_client", { path, client }),
  projectStatus: (path) => invoke("project_status", { path }),
  projectUsage: (path, days) => invoke("project_usage", { path, days }),
  usageSummary: (days) => invoke("usage_summary", { days }),
  removeProject: (path) => invoke("remove_project", { path }),

  setSetting: (key, value) => invoke("set_setting", { key, value }),
  openInEditor: (path) => invoke("open_in_editor", { path }),
  openLink: (url) => invoke("open_link", { url }),
  detectClis: () => invoke("detect_clis"),
  doctor: () => invoke("doctor"),
  pickFolder: async () => {
    const folder = await open({ directory: true, multiple: false });
    return typeof folder === "string" ? folder : null;
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
  tasks: (project) => invoke("tasks", { project }),
  taskEvents: (id) => invoke("task_events", { id }),
  activity: () => invoke("activity"),
  answerPermission: (id, requestId, decision) => invoke("answer_permission", { id, requestId, decision }),
  answerPlan: (id, requestId, approve, feedback) => invoke("answer_plan", { id, requestId, approve, feedback }),
  memory: (path) => invoke("memory", { path }),
  searchMemory: (path, query) => invoke("search_memory", { path, query }),
  addNote: (project, text, category) => invoke("add_note", { project, text, category }),
  editNote: (id, text, category) => invoke("edit_note", { id, text, category }),
  approveNote: (id) => invoke("approve_note", { id }),
  replaceNote: (proposal, target) => invoke("replace_note", { proposal, target }),
  forgetNote: (id) => invoke("forget_note", { id }),
  keepNote: (keep, forget) => invoke("keep_note", { keep, forget }),
  dismissContradiction: (a, b) => invoke("dismiss_contradiction", { a, b }),
  setMemoryReader: (path, account, reads) => invoke("set_memory_reader", { path, account, reads }),
  sendMessage: (id, text) => invoke("send_message", { id, text }),
  queued: (id) => invoke("queued", { id }),
  queueMessage: (id, text) => invoke("queue_message", { id, text }),
  editQueued: (message, text) => invoke("edit_queued", { message, text }),
  removeQueued: (message) => invoke("remove_queued", { message }),
  sendQueuedNow: (id) => invoke("send_queued_now", { id }),
  templates: (project) => invoke("templates", { project }),
  allTemplates: () => invoke("all_templates"),
  addTemplate: (input) => invoke("add_template", { input }),
  updateTemplate: (id, name, text) => invoke("update_template", { id, name, text }),
  removeTemplate: (id) => invoke("remove_template", { id }),
  restoreBuiltinTemplates: () => invoke("restore_builtin_templates"),
  stopTask: (id) => invoke("stop_task", { id }),
  taskDiff: (id) => invoke("task_diff", { id }),
  acceptTask: (id) => invoke("accept_task", { id }),
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
};

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
