import { formatDuration, formatTokens, formatUsd, timeAgo } from "./format";

it("formats token counts", () => {
  expect(formatTokens(950)).toBe("950");
  expect(formatTokens(182_000)).toBe("182k");
  expect(formatTokens(999_499)).toBe("999k");
  // Rounds before choosing the unit: never "1000k".
  expect(formatTokens(999_600)).toBe("1.0M");
  expect(formatTokens(2_400_000)).toBe("2.4M");
});

it("formats costs, durations and ages", () => {
  expect(formatUsd(3.1)).toBe("$3,10");
  expect(formatUsd(null)).toBe("—");
  expect(formatDuration(42_000)).toBe("42 s");
  expect(formatDuration(185_000)).toBe("3 min 5 s");
  expect(timeAgo(0, 3 * 86_400)).toBe("hace 3 días");
});
