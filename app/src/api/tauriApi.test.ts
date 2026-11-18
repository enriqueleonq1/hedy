import { invoke } from "@tauri-apps/api/core";

import { tauriApi } from "./tauriApi";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));

// Tauri closes the window after each close listener unless that listener's
// own event was prevented: one listener must ask every guard.
const listeners: ((event: { preventDefault: () => void }) => Promise<void>)[] = [];
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    onCloseRequested: (handler: (typeof listeners)[number]) => {
      listeners.push(handler);
      return Promise.resolve(() => {});
    },
  }),
}));

async function close(): Promise<boolean> {
  let prevented = false;
  for (const listener of listeners) await listener({ preventDefault: () => (prevented = true) });
  return !prevented;
}

describe("tauriApi.onCloseRequested", () => {
  it("keeps the window open when any guard says no", async () => {
    const yes = tauriApi.onCloseRequested(async () => true);
    const no = tauriApi.onCloseRequested(async () => false);
    expect(listeners).toHaveLength(1);
    expect(await close()).toBe(false);
    no();
    expect(await close()).toBe(true);
    yes();
  });
});

// The same commands and argument names `sync_ipc_end_to_end` (src-tauri/src/sync.rs)
// sends through the real IPC: a rename on either side breaks one of the two.
describe("tauriApi sync payloads", () => {
  it("sends each sync command with the arguments the backend reads", async () => {
    const calls: [() => Promise<unknown>, string, Record<string, unknown> | undefined][] = [
      [() => tauriApi.syncStatus(), "sync_status", undefined],
      [() => tauriApi.syncDefaultName(), "sync_default_name", undefined],
      [() => tauriApi.syncRepoIsPublic("u"), "sync_repo_is_public", { url: "u" }],
      [() => tauriApi.syncSetup("u", "Casa"), "sync_setup", { url: "u", name: "Casa" }],
      [() => tauriApi.syncJoin("u", "w", "Casa"), "sync_join", { url: "u", words: "w", name: "Casa" }],
      [() => tauriApi.syncNow(), "sync_now", undefined],
      [() => tauriApi.syncRemoveDevice("d"), "sync_remove_device", { id: "d" }],
      [() => tauriApi.syncLeave(), "sync_leave", undefined],
      [() => tauriApi.syncConflicts(), "sync_conflicts", undefined],
      [() => tauriApi.resolveSyncConflict(3, true), "resolve_sync_conflict", { id: 3, useOther: true }],
      [() => tauriApi.memoryHome("/p"), "memory_home", { path: "/p" }],
      [() => tauriApi.memoryHomes(), "memory_homes", undefined],
      [
        () => tauriApi.setMemoryHome("/p", { kind: "shared", group: "g" }),
        "set_memory_home",
        { path: "/p", home: { kind: "shared", group: "g" } },
      ],
      [() => tauriApi.setMemoryHome("/p", null), "set_memory_home", { path: "/p", home: null }],
      [() => tauriApi.answerOwner("o", true), "answer_owner", { owner: "o", mine: true }],
      [() => tauriApi.forgetOwner("o"), "forget_owner", { owner: "o" }],
    ];
    for (const [call, cmd, args] of calls) {
      vi.mocked(invoke).mockClear();
      await call();
      expect(vi.mocked(invoke).mock.calls[0]).toEqual(args === undefined ? [cmd] : [cmd, args]);
    }
  });
});
