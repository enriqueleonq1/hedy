import { act, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { ApiProvider, useOverview } from "../api/context";
import { MockApi } from "../api/MockApi";
import type { PlanState } from "../api/types";
import { translator } from "../i18n";
import { DUE_CHECK_MS, PlanUsageProvider, REFRESH_EVERY_MS, usageTitle, usePlanRefresh } from "./planUsage";

function Busy() {
  const { busy } = usePlanRefresh();
  return <output aria-label="busy">{[...busy].sort().join(",")}</output>;
}

function Harness() {
  const overview = useOverview();
  if (!overview.data) return null;
  return (
    <PlanUsageProvider accounts={overview.data.accounts} screen="welcome">
      <Busy />
    </PlanUsageProvider>
  );
}

async function setup(api = MockApi.withSampleData()) {
  api.delayMs = 0;
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <ApiProvider api={api}>
        <Harness />
      </ApiProvider>
    </QueryClientProvider>,
  );
  await act(() => vi.advanceTimersByTimeAsync(10));
  return api;
}

const busy = () => screen.getByRole("status", { name: "busy" }).textContent;
const callsFor = (spy: { mock: { calls: unknown[][] } }, id: string) => spy.mock.calls.filter(([x]) => x === id).length;

/** A promise for one reading that the test settles. */
function deferred() {
  let resolve!: (plan: PlanState) => void;
  const promise = new Promise<PlanState>((r) => (resolve = r));
  return { promise, resolve };
}

describe("plan usage refresh", () => {
  let focused = true;

  beforeEach(() => {
    vi.useFakeTimers();
    focused = true;
    vi.spyOn(document, "hasFocus").mockImplementation(() => focused);
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("asks again every three minutes while the window has focus", async () => {
    const api = MockApi.withSampleData();
    const refresh = vi.spyOn(api, "refreshPlanUsage");
    await setup(api);
    expect(REFRESH_EVERY_MS).toBe(3 * 60 * 1000);
    expect(callsFor(refresh, "claude-personal")).toBe(1);

    await act(() => vi.advanceTimersByTimeAsync(REFRESH_EVERY_MS - DUE_CHECK_MS));
    expect(callsFor(refresh, "claude-personal")).toBe(1);
    await act(() => vi.advanceTimersByTimeAsync(2 * DUE_CHECK_MS));
    expect(callsFor(refresh, "claude-personal")).toBe(2);
    await act(() => vi.advanceTimersByTimeAsync(REFRESH_EVERY_MS));
    expect(callsFor(refresh, "claude-personal")).toBe(3);
  });

  it("keeps the pace when a reading came from elsewhere, and waits while unfocused", async () => {
    const api = MockApi.withSampleData();
    const refresh = vi.spyOn(api, "refreshPlanUsage");
    await setup(api);
    // A focus halfway reads again (the mock has no cap): the next one is due three minutes after it.
    await act(() => vi.advanceTimersByTimeAsync(REFRESH_EVERY_MS / 2));
    await act(async () => {
      window.dispatchEvent(new Event("focus"));
      await vi.advanceTimersByTimeAsync(10);
    });
    expect(callsFor(refresh, "claude-personal")).toBe(2);
    await act(() => vi.advanceTimersByTimeAsync(REFRESH_EVERY_MS - DUE_CHECK_MS));
    expect(callsFor(refresh, "claude-personal")).toBe(2);
    await act(() => vi.advanceTimersByTimeAsync(2 * DUE_CHECK_MS));
    expect(callsFor(refresh, "claude-personal")).toBe(3);

    focused = false;
    await act(() => vi.advanceTimersByTimeAsync(3 * REFRESH_EVERY_MS));
    expect(callsFor(refresh, "claude-personal")).toBe(3);
  });

  it("marks the figures as refreshing on coming back after longer than the interval", async () => {
    const api = MockApi.withSampleData();
    await setup(api);
    expect(busy()).toBe("");
    const original = api.refreshPlanUsage.bind(api);
    let pending = deferred();
    const refresh = vi
      .spyOn(api, "refreshPlanUsage")
      .mockImplementation((id) => (id === "claude-personal" ? pending.promise : original(id)));
    const settle = async () => {
      const plan = (await api.overview()).accounts.find((a) => a.id === "claude-personal")!.usage.plan;
      pending.resolve(plan);
      await vi.advanceTimersByTimeAsync(10);
    };

    // A short trip away reads again quietly.
    focused = false;
    window.dispatchEvent(new Event("blur"));
    await act(() => vi.advanceTimersByTimeAsync(60_000));
    focused = true;
    await act(async () => {
      window.dispatchEvent(new Event("focus"));
      await vi.advanceTimersByTimeAsync(10);
    });
    expect(refresh).toHaveBeenCalledWith("claude-personal");
    expect(busy()).toBe("");
    await act(settle);

    // Longer than the interval: the last figure stays, marked, until the new reading comes.
    pending = deferred();
    focused = false;
    window.dispatchEvent(new Event("blur"));
    await act(() => vi.advanceTimersByTimeAsync(REFRESH_EVERY_MS + 1000));
    focused = true;
    await act(async () => {
      window.dispatchEvent(new Event("focus"));
      await vi.advanceTimersByTimeAsync(10);
    });
    expect(busy()).toContain("claude-personal");
    await act(settle);
    expect(busy()).toBe("");
  });

  it("marks a figure older than twice the interval while it is read again", async () => {
    const api = MockApi.withSampleData();
    const overview = api.overview.bind(api);
    const old = Math.floor(Date.now() / 1000) - (2 * REFRESH_EVERY_MS) / 1000 - 60;
    vi.spyOn(api, "overview").mockImplementation(async () => {
      const o = await overview();
      o.accounts.find((a) => a.id === "claude-personal")!.usage.plan.usage!.checkedAt = old;
      return o;
    });
    const original = api.refreshPlanUsage.bind(api);
    const pending = deferred();
    vi.spyOn(api, "refreshPlanUsage").mockImplementation((id) =>
      id === "claude-personal" ? pending.promise : original(id),
    );
    await setup(api);
    expect(busy()).toBe("claude-personal");
    await act(async () => {
      const plan = (await overview()).accounts.find((a) => a.id === "claude-personal")!.usage.plan;
      pending.resolve(plan);
      await vi.advanceTimersByTimeAsync(10);
    });
    expect(busy()).toBe("");
  });
});

describe("plan usage tooltip", () => {
  it("says how old a reading is in the tooltip", async () => {
    const api = MockApi.withSampleData();
    const overview = await api.overview();
    const personal = overview.accounts.find((a) => a.id === "claude-personal")!;
    const at = personal.usage.plan.usage!.checkedAt;
    expect(usageTitle(personal, translator("es"), "es", at + 12 * 60)).toBe(
      "Claude personal · Semana · 83% · actualizado hace 12 min",
    );
    expect(usageTitle(personal, translator("en"), "en", at + 20)).toBe("Claude personal · Week · 83% · updated just now");
  });
});
