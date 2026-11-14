import { formatDuration, formatTokens, formatUsd, timeAgo } from "./format";
import { translator } from "./i18n";

it("formats token counts", () => {
  expect(formatTokens(950)).toBe("950");
  expect(formatTokens(182_000)).toBe("182k");
  expect(formatTokens(999_499)).toBe("999k");
  // Rounds before choosing the unit: never "1000k".
  expect(formatTokens(999_600)).toBe("1.0M");
  expect(formatTokens(2_400_000)).toBe("2.4M");
});

it("formats costs, durations and ages", () => {
  expect(formatUsd(3.1, "es")).toBe("$3,10");
  expect(formatUsd(null, "es")).toBe("—");
  expect(formatDuration(42_000)).toBe("42 s");
  expect(formatDuration(185_000)).toBe("3 min 5 s");
  expect(timeAgo(0, translator("es"), 3 * 86_400)).toBe("hace 3 días");
});

it("formats costs and ages in English", () => {
  const t = translator("en");
  expect(formatUsd(3.1, "en")).toBe("$3.10");
  expect(timeAgo(0, t, 3 * 86_400)).toBe("3 days ago");
  expect(timeAgo(0, t, 86_400)).toBe("yesterday");
  expect(timeAgo(0, t, 20)).toBe("just now");
  expect(timeAgo(0, t, 5 * 60)).toBe("5 min ago");
});
