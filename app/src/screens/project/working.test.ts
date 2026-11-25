import { applyPreset, PRESETS, presetOf } from "./working";

describe("ways of working", () => {
  const base = { plan: false, tests: false, e2e: false, review: false, e2eCommand: "npm run e2e" };

  it("names every preset and keeps the commands", () => {
    for (const preset of PRESETS) {
      const style = applyPreset(preset, base);
      expect(presetOf(style)).toBe(preset);
      expect(style.e2eCommand).toBe("npm run e2e");
    }
    expect(presetOf(base)).toBe("now");
  });

  it("calls any other set of stops custom", () => {
    expect(presetOf({ ...applyPreset("afterOk", base), tests: false })).toBeNull();
  });
});
