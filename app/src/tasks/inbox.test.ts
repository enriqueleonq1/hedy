import type { AgentEvent } from "../api/types";
import { openPermission } from "./inbox";

const request = (requestId: string, summary: string): AgentEvent => ({
  kind: "permissionRequest",
  requestId,
  tool: "Bash",
  summary,
  input: {},
});

describe("openPermission", () => {
  it("finds the request still waiting for an answer", () => {
    expect(openPermission([])).toBeNull();
    const resolved: AgentEvent = { kind: "permissionResolved", requestId: "a", decision: "allow", automatic: false, reason: null };
    expect(openPermission([request("a", "ls"), resolved])).toBeNull();
    expect(openPermission([request("a", "ls"), resolved, request("b", "npm test")])?.summary).toBe("npm test");
  });
});
