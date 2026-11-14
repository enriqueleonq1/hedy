import { Channel, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

import type { HedyApi } from "./HedyApi";
import type { AgentEvent, Task, TerminalEvent } from "./types";

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
  removeProject: (path) => invoke("remove_project", { path }),

  setSetting: (key, value) => invoke("set_setting", { key, value }),
  openInEditor: (path) => invoke("open_in_editor", { path }),
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
  answerPermission: (id, requestId, decision) => invoke("answer_permission", { id, requestId, decision }),
  answerPlan: (id, requestId, approve, feedback) => invoke("answer_plan", { id, requestId, approve, feedback }),
  sendMessage: (id, text) => invoke("send_message", { id, text }),
  stopTask: (id) => invoke("stop_task", { id }),
  taskDiff: (id) => invoke("task_diff", { id }),
  acceptTask: (id) => invoke("accept_task", { id }),
  discardTask: (id) => invoke("discard_task", { id }),
  recentCommits: (project) => invoke("recent_commits", { project }),
  onTaskEvent: (listener) =>
    subscribe<{ taskId: string; event: AgentEvent }>("task-event", ({ taskId, event }) => listener(taskId, event)),
  onTaskUpdated: (listener) => subscribe<Omit<Task, "live">>("task-updated", listener),
};

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
