import { describe, expect, it } from "vitest";

import type { AgentCommand } from "../api/types";
import { commandName, commandText, commandWithArguments, filterCommands } from "./commands";

const command = (name: string, description: string | null = null): AgentCommand => ({
  name,
  description,
  source: "skill",
  argumentHint: null,
});

const list = [
  command("design", "Design a screen"),
  command("code-review", "Review the diff"),
  command("deploy-notes", "Write release notes"),
  command("compact", "Free up context"),
];

describe("filterCommands", () => {
  it("ranks names that start with the query, then contain it, then fuzzy, then descriptions", () => {
    expect(filterCommands(list, "").map((c) => c.name)).toEqual(list.map((c) => c.name));
    expect(filterCommands(list, "de").map((c) => c.name)).toEqual(["design", "deploy-notes", "code-review"]);
    expect(filterCommands(list, "cdr").map((c) => c.name)).toEqual(["code-review"]);
    expect(filterCommands(list, "release").map((c) => c.name)).toEqual(["deploy-notes"]);
    expect(filterCommands(list, "DÉSIGN").map((c) => c.name)).toEqual(["design"]);
  });

  it("matches nothing once arguments are being written", () => {
    expect(filterCommands(list, "design login")).toEqual([]);
  });
});

describe("commands in a message", () => {
  it("finds the command a message starts with", () => {
    expect(commandName("/design login screen")).toBe("design");
    expect(commandName("/studio:mockup")).toBe("studio:mockup");
    expect(commandName("/home/user/file.rs is broken")).toBeNull();
    expect(commandName("hello /design")).toBeNull();
  });

  it("knows when the arguments of a known command are being written", () => {
    expect(commandWithArguments("/design ", list)?.name).toBe("design");
    expect(commandWithArguments("/design login", list)?.name).toBe("design");
    expect(commandWithArguments("/design", list)).toBeNull();
    expect(commandWithArguments("/unknown x", list)).toBeNull();
    expect(commandText(list[0]!)).toBe("/design ");
  });
});
