import type { Account, UsageWindow } from "../api/types";
import { translator } from "../i18n";
import {
  formatBalance,
  headlineText,
  headlineWindow,
  planCardState,
  resetLabel,
  shortWindowLabel,
  usageLevel,
  windowLabel,
  windowText,
} from "./planUsage";

const window = (extra: Partial<UsageWindow>): UsageWindow => ({
  kind: "session",
  minutes: 300,
  model: null,
  percent: 10,
  resetsText: null,
  resetsAt: null,
  ...extra,
});

describe("plan usage", () => {
  it("colors by the meters' thresholds: amber from 70 %, red from 90 %", () => {
    expect([usageLevel(69.9), usageLevel(70), usageLevel(89.9), usageLevel(90), usageLevel(100)]).toEqual([
      "ok",
      "warning",
      "warning",
      "danger",
      "danger",
    ]);
  });

  it("uses the shared window closest to its limit for compact labels", () => {
    const account = {
      usage: {
        plan: {
          usage: {
            windows: [
              window({ percent: 20 }),
              window({ kind: "week", percent: 70 }),
              window({ kind: "week", percent: 99, model: "Opus" }),
            ],
          },
        },
      },
    } as unknown as Account;
    expect(headlineWindow(account)?.percent).toBe(70);
  });

  it("formats resets in the active language, or keeps the source's text", () => {
    const es = translator("es");
    const en = translator("en");
    const now = new Date(2026, 10, 14, 12, 0).getTime();
    const at = (h: number, d = 14) => new Date(2026, 10, d, h, 0).getTime() / 1000;
    expect(resetLabel(window({ resetsAt: at(19) }), es, "es", now)).toBe("Se renueva hoy a las 19:00");
    expect(resetLabel(window({ resetsAt: at(0, 15) }), en, "en", now)).toMatch(/^Resets tomorrow at 12:00\sAM$/);
    expect(resetLabel(window({ resetsAt: at(0, 20) }), es, "es", now)).toMatch(/^Se renueva el viernes, 20 nov\.? a las 00:00$/);
    expect(resetLabel(window({ resetsText: "in 3 hr" }), en, "en", now)).toBe("Resets in 3 hr");
    expect(resetLabel(window({}), en, "en", now)).toBeNull();
  });

  it("names every kind of window", () => {
    const t = translator("en");
    expect(windowLabel(window({}), t)).toBe("Current session (5 h)");
    expect(windowLabel(window({ kind: "day" }), t)).toBe("Today");
    expect(windowLabel(window({ kind: "other", minutes: 60 }), t)).toBe("1 h limit");
    expect(windowLabel(window({ kind: "other", minutes: 15 }), t)).toBe("15 min limit");
  });

  it("sums up an account in one line with the same pattern everywhere", () => {
    const es = translator("es");
    const en = translator("en");
    const withWindows = (windows: UsageWindow[]) =>
      ({ usage: { limit: null, plan: { usage: { windows } } } }) as unknown as Account;
    const account = withWindows([window({ percent: 34.4 }), window({ kind: "week", percent: 83 })]);
    expect(headlineText(account, es)).toBe("Semana · 83%");
    expect(headlineText(withWindows([window({ percent: 34.4 })]), es)).toBe("Sesión 5 h · 34%");
    expect(windowText(window({ percent: 12 }), en)).toBe("5-hour session · 12%");
    expect(shortWindowLabel(window({ kind: "other", minutes: 120 }), en)).toBe("2 h");
    expect(shortWindowLabel(window({ kind: "other", minutes: null }), es)).toBe("Límite");
    expect(headlineText(withWindows([]), es)).toBeNull();
  });

  it("formats a balance in its currency", () => {
    expect(formatBalance({ available: 12.5, currency: "USD" }, "en")).toBe("$12.50");
    expect(formatBalance({ available: 1, currency: "???" }, "en")).toBe("1.00 ???");
  });

  it("picks what a plan card shows: rings only when there is something to draw", () => {
    const card = (kind: Account["kind"], plan: Partial<Account["usage"]["plan"]>) =>
      planCardState({
        kind,
        usage: { plan: { usage: null, attemptedAt: 1, problem: null, supported: true, ...plan } },
      } as unknown as Account);
    const reading = (extra: object) => ({ windows: [], checkedAt: 1, source: "claudeUsage", balance: null, ...extra });
    expect(card("subscription", { usage: reading({ windows: [window({})] }) as never })).toBe("windows");
    expect(card("apiKey", { usage: reading({ balance: { available: 3, currency: "USD" } }) as never })).toBe("balance");
    // Never asked yet: a skeleton while the first reading comes.
    expect(card("subscription", { attemptedAt: null })).toBe("loading");
    expect(card("apiKey", { attemptedAt: null })).toBe("loading");
    expect(card("apiKey", {})).toBe("payAsYouGo");
    expect(card("subscription", { problem: "unavailable" })).toBe("noData");
    expect(card("subscription", { attemptedAt: null, problem: "unavailable" })).toBe("noData");
    expect(card("subscription", { supported: false, attemptedAt: null })).toBe("unsupported");
  });
});
