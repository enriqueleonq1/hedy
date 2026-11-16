import { codeThemeName } from "../../theme/codeTheme";
import { presetById } from "../../theme/presets";
import { highlightLines } from "./highlight";

// A diff in any theme but Hedy's own is colored with that theme's `syn*` tokens.
it("colors code with the theme's syntax tokens", async () => {
  const dracula = presetById("dracula");
  if (!dracula) throw new Error("no dracula preset");
  const name = codeThemeName(dracula);
  // The first call compiles the grammar, which may hit tokenizing's time limit on a busy machine.
  await highlightLines(["const a = 1;"], "a.ts", name);
  const [line] = (await highlightLines(['const greeting = "hola"; // saludo'], "a.ts", name)) ?? [];
  const color = (text: string) => line?.find((token) => token.content.includes(text))?.color?.toUpperCase();
  expect(color("const")).toBe(dracula.tokens.synKeyword);
  expect(color("hola")).toBe(dracula.tokens.synString);
  expect(color("saludo")).toBe(dracula.tokens.synComment);
});

it("keeps the editor themes Hedy's own themes always had", () => {
  const tokyo = presetById("tokyo-night");
  const burbuja = presetById("burbuja");
  expect(tokyo && codeThemeName(tokyo)).toBe("tokyo-night");
  expect(burbuja && codeThemeName(burbuja)).toBe("catppuccin-latte");
});
