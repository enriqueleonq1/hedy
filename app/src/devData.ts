// The browser's sample data with nothing in it, or failing (`npm run dev`
// only, never the app): `?data=empty` has no accounts and no projects,
// `?data=noprojects` the sample accounts without projects, and `?data=fail`
// every read but the overview rejecting, to see each screen's empty and error
// states (main.tsx).

import type { HedyApi } from "./api/HedyApi";

export type DataMode = "empty" | "noprojects" | "fail";

/** The `?data=` switch of a page's query string; anything else is the sample data. */
export function dataMode(search: string): DataMode | null {
  const value = new URLSearchParams(search).get("data");
  return value === "empty" || value === "noprojects" || value === "fail" ? value : null;
}

/** What a failing read rejects with (a string, as the Tauri commands do). */
export const SIMULATED_FAILURE = "Simulated failure (?data=fail)";

/**
 * `api` with every call rejecting, except the overview (so the shell draws)
 * and the event subscriptions (`on…`, which return a way to stop listening).
 */
export function failingApi<T extends HedyApi>(api: T): T {
  return new Proxy(api, {
    get(target, property, receiver) {
      const value: unknown = Reflect.get(target, property, receiver);
      if (typeof value !== "function") return value;
      const name = String(property);
      if (name === "overview" || /^on[A-Z]/.test(name)) return value.bind(target);
      return () => Promise.reject(SIMULATED_FAILURE);
    },
  });
}
