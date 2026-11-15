import { translator } from "../i18n";
import { formatCountdown } from "./countdown";

describe("formatCountdown", () => {
  const es = translator("es");

  it("shows hours, minutes or seconds depending on what is left", () => {
    expect(formatCountdown(2 * 3600 + 5 * 60, es)).toBe("2 h 05 min");
    expect(formatCountdown(47 * 60, es)).toBe("47 min");
    expect(formatCountdown(4 * 60 + 5, es)).toBe("4 min 05 s");
    expect(formatCountdown(40, es)).toBe("40 s");
    expect(formatCountdown(0.2, es)).toBe("1 s");
  });

  it("is null once due", () => {
    expect(formatCountdown(0, es)).toBeNull();
    expect(formatCountdown(-30, es)).toBeNull();
  });
});
