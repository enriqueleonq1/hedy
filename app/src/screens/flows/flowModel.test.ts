import { describe, expect, it } from "vitest";

import { mockProblems, mockProjectFlow, mockTemplates, MOCK_ROLES } from "../../api/mockFlows";
import type { Flow } from "../../api/types";
import { blankFlow, COL, connect, edgeFrom, history, insertStep, layout, nextId, removeStep, runOrder, setRounds } from "./flowModel";

const fixLoop = (): Flow => mockTemplates().find((t) => t.key === "focused-review")!.flow;

describe("flow model", () => {
  it("lays a flow out by its distance from the start, ignoring ways back", () => {
    const at = layout(fixLoop());
    expect([1, 2, 3, 4, 5, 6].map((i) => at.get(`n${i}`)!.x)).toEqual([0, 1, 2, 3, 4, 5].map((c) => c * COL));
    // Every step on one row: the loops go under it.
    expect(new Set([...at.values()].map((p) => p.y))).toEqual(new Set([0]));
  });

  it("a new flow is ready to run", () => {
    expect(mockProblems(blankFlow("Mío"), MOCK_ROLES)).toEqual([]);
  });

  it("inserting a step keeps the flow joined, and a condition gets its way back", () => {
    const start = blankFlow("Mío");
    const { flow, id } = insertStep(start, "n2", { kind: "condition", check: "tests" });
    expect(id).toBe("n4");
    expect(edgeFrom(flow, "n2", "after")!.to).toBe("n4");
    expect(edgeFrom(flow, "n4", "pass")!.to).toBe("n3");
    expect(edgeFrom(flow, "n4", "fail")).toEqual({ from: "n4", to: "n2", when: "fail", maxRounds: 2 });
    expect(mockProblems(flow, MOCK_ROLES)).toEqual([]);
    // The end moved a column to make room.
    expect(flow.nodes.find((n) => n.id === "n3")!.at!.x).toBe(3 * COL);
  });

  it("removing a step joins its neighbours and drops the ways back to it", () => {
    const flow = removeStep(fixLoop(), "n2");
    expect(edgeFrom(flow, "n1", "after")!.to).toBe("n3");
    // The tests' way back pointed at the programmer: it asks for a new one.
    expect(edgeFrom(flow, "n3", "fail")).toBeUndefined();
    expect(mockProblems(flow, MOCK_ROLES).map((p) => p.node)).toContain("n3");
    // Start and end stay.
    expect(removeStep(flow, "n1")).toBe(flow);
  });

  it("connecting replaces the old way and a way back keeps its rounds", () => {
    let flow = setRounds(fixLoop(), "n3", "fail", 5);
    flow = connect(flow, "n3", "fail", "n4");
    expect(edgeFrom(flow, "n3", "fail")).toEqual({ from: "n3", to: "n4", when: "fail", maxRounds: 5 });
    expect(flow.edges.filter((e) => e.from === "n3" && e.when === "fail")).toHaveLength(1);
    expect(setRounds(flow, "n3", "fail", 99).edges.find((e) => e.from === "n3" && e.when === "fail")!.maxRounds).toBe(10);
  });

  it("orders the steps as they run and finds the next free id", () => {
    const flow = mockProjectFlow();
    expect(runOrder(flow).map((n) => n.kind)[0]).toBe("start");
    expect(runOrder(flow).at(-1)!.kind).toBe("end");
    expect(nextId(flow)).toBe(`n${flow.nodes.length + 1}`);
  });

  it("undo and redo walk the history", () => {
    const a = blankFlow("a");
    const b = { ...a, name: "b" };
    let h = history.push(history.start(a), b);
    h = history.undo(h);
    expect(h.now).toBe(a);
    h = history.redo(h);
    expect(h.now).toBe(b);
    expect(history.redo(h)).toBe(h);
  });
});
