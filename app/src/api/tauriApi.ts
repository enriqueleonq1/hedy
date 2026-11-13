import { Channel, invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

import type { HedyApi } from "./HedyApi";
import type { TerminalEvent } from "./types";

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
};

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
