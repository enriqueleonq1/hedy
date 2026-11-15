import type { Task } from "../../api/types";
import { resolveCopy } from "./copy";
import { parseInput, parseLine, prefixOf, splitFileLine } from "./modes";

describe("palette modes", () => {
  it("reads the mode from the first character, else the mode it was opened in", () => {
    expect(parseInput(">abrir", "files")).toEqual({ mode: "commands", query: "abrir" });
    expect(parseInput(":120", "files")).toEqual({ mode: "line", query: "120" });
    expect(parseInput("@iva", "all")).toEqual({ mode: "symbols", query: "iva" });
    expect(parseInput("#total", "all")).toEqual({ mode: "text", query: "total" });
    expect(parseInput("?", "files")).toEqual({ mode: "help", query: "" });
    expect(parseInput("impuestos", "files")).toEqual({ mode: "files", query: "impuestos" });
    expect(parseInput("impuestos", "all")).toEqual({ mode: "all", query: "impuestos" });
    expect(parseInput("", "files")).toEqual({ mode: "files", query: "" });
  });

  it("knows each mode's prefix", () => {
    expect(prefixOf("files")).toBe("");
    expect(prefixOf("all")).toBe("");
    expect(prefixOf("commands")).toBe(">");
    expect(prefixOf("text")).toBe("#");
  });

  it("reads a line, with an optional column", () => {
    expect(parseLine("120")).toEqual({ line: 120, column: null });
    expect(parseLine(" 120:5 ")).toEqual({ line: 120, column: 5 });
    expect(parseLine("120,5")).toEqual({ line: 120, column: 5 });
    expect(parseLine("0")).toBeNull();
    expect(parseLine("abc")).toBeNull();
    expect(parseLine("")).toBeNull();
  });

  it("splits `file:line` in files mode", () => {
    expect(splitFileLine("impuestos:16")).toEqual({ query: "impuestos", line: 16 });
    expect(splitFileLine("impuestos.py:16:3")).toEqual({ query: "impuestos.py", line: 16 });
    expect(splitFileLine("impuestos")).toEqual({ query: "impuestos", line: null });
    expect(splitFileLine(":16")).toEqual({ query: ":16", line: null });
  });
});

function task(id: string, extra: Partial<Task> = {}): Task {
  return {
    id,
    project: "/code/hedy",
    mode: "project",
    title: id,
    prompt: "",
    account: "a",
    model: null,
    workspace: { worktree: `/wt/${id}`, branch: `hedy/${id}`, baseBranch: "main", baseCommit: "abc" },
    sessionId: null,
    status: "readyForReview",
    createdAt: 0,
    stages: [],
    stage: 0,
    live: false,
    ...extra,
  } as Task;
}

describe("which copy the palette searches", () => {
  const folder = { copy: { kind: "folder", project: "/code/hedy" }, project: "/code/hedy", onScreen: true };

  it("is the open task's worktree, else the project's folder", () => {
    expect(resolveCopy({ screen: "workspace", project: "/code/hedy", task: "s5" }, [task("s5")])).toEqual({
      ...folder,
      copy: { kind: "task", task: "s5" },
    });
    expect(resolveCopy({ screen: "workspace", project: "/code/hedy", task: null }, [task("s5")])).toEqual(folder);
  });

  it("follows the workspace's default: the most recent open task", () => {
    const tasks = [task("old", { status: "accepted", merge: null }), task("s2", { status: "running" })];
    expect(resolveCopy({ screen: "workspace", project: "/code/hedy" }, tasks)?.copy).toEqual({ kind: "task", task: "s2" });
    expect(resolveCopy({ screen: "workspace", project: "/code/hedy" }, [])).toEqual(folder);
  });

  it("reads the folder for a task without files of its own", () => {
    const discarded = task("d", { status: "discarded" });
    expect(resolveCopy({ screen: "workspace", project: "/code/hedy", task: "d" }, [discarded])).toEqual(folder);
  });

  it("is the folder on a project's memory or status, off screen", () => {
    expect(resolveCopy({ screen: "status", project: "/code/hedy" }, undefined)).toEqual({ ...folder, onScreen: false });
  });

  it("is none outside a project", () => {
    expect(resolveCopy({ screen: "welcome" }, undefined)).toBeNull();
    expect(resolveCopy({ screen: "conversation", task: "c1" }, undefined)).toBeNull();
  });
});
