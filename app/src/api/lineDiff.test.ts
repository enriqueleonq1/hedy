import { describe, expect, it } from "vitest";

import { diffOps, textDiff } from "./lineDiff";

describe("lineDiff", () => {
  it("keeps the common start and end as context and numbers both sides", () => {
    const ops = diffOps(["a", "b", "c", "d"], ["a", "B", "c", "d"]);
    expect(ops.map((o) => [o.kind, o.oldLine, o.newLine])).toEqual([
      ["context", 1, 1],
      ["removed", 2, null],
      ["added", null, 2],
      ["context", 3, 3],
      ["context", 4, 4],
    ]);
  });

  it("stays cheap on a big file with a small edit, and replaces a middle too big to compare", () => {
    const big = Array.from({ length: 20_000 }, (_, i) => `línea ${i}`);
    const edited = [...big];
    edited[10_000] = "cambiada";
    const diff = textDiff("x.txt", `${big.join("\n")}\n`, `${edited.join("\n")}\n`);
    expect([diff?.additions, diff?.deletions]).toEqual([1, 1]);

    const other = Array.from({ length: 3_000 }, (_, i) => `otra ${i}`);
    const replaced = diffOps(big.slice(0, 3_000), other);
    expect(replaced.filter((o) => o.kind === "removed")).toHaveLength(3_000);
    expect(replaced.filter((o) => o.kind === "added")).toHaveLength(3_000);
  });
});
