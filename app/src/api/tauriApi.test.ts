import { tauriApi } from "./tauriApi";

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
