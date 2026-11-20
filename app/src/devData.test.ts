import { MockApi } from "./api/MockApi";
import { dataMode, failingApi, SIMULATED_FAILURE } from "./devData";

describe("the browser's ?data= switch", () => {
  it("reads only the modes it knows", () => {
    expect(dataMode("?data=empty")).toBe("empty");
    expect(dataMode("?theme=light&data=noprojects")).toBe("noprojects");
    expect(dataMode("?data=fail&lang=en")).toBe("fail");
    expect(dataMode("?data=other")).toBeNull();
    expect(dataMode("")).toBeNull();
  });

  it("fails every read but the overview, and keeps the event subscriptions", async () => {
    const api = failingApi(MockApi.withSampleData());
    expect((await api.overview()).accounts.length).toBeGreaterThan(0);
    await expect(api.syncStatus()).rejects.toBe(SIMULATED_FAILURE);
    await expect(api.tasks("/code/hedy")).rejects.toBe(SIMULATED_FAILURE);
    const stop = api.onSyncUpdated(() => {});
    expect(typeof stop).toBe("function");
  });
});
