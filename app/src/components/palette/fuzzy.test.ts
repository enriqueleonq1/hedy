import { fold, fuzzyScore } from "./fuzzy";

describe("fuzzy matching", () => {
  it("ignores accents and case", () => {
    expect(fold("Sesión ÁRBOL")).toBe("sesion arbol");
    expect(fuzzyScore("sesion", "Sesión 5 h")).not.toBeNull();
  });

  it("needs every word, in any order", () => {
    expect(fuzzyScore("hedy nueva", "Nueva tarea en hedy")).not.toBeNull();
    expect(fuzzyScore("hedy codex", "Nueva tarea en hedy")).toBeNull();
  });

  it("ranks the start of a word over a substring over letters in order", () => {
    const start = fuzzyScore("term", "Abrir una terminal en hedy")!;
    const inside = fuzzyScore("rmin", "Abrir una terminal en hedy")!;
    const loose = fuzzyScore("tmnl", "Abrir una terminal en hedy")!;
    expect(start).toBeGreaterThan(inside);
    expect(inside).toBeGreaterThan(loose);
    expect(fuzzyScore("hedy", "hedy")!).toBeGreaterThan(fuzzyScore("hedy", "Nueva tarea en hedy")!);
  });

  it("matches everything with an empty query", () => {
    expect(fuzzyScore("  ", "lo que sea")).toBe(0);
  });
});
