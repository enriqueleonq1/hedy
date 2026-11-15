import { fold, foldEach, fuzzyMatch, fuzzyPath, fuzzyScore } from "./fuzzy";

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

describe("fuzzy matching with positions", () => {
  it("folds character by character, so positions point at the original text", () => {
    expect(foldEach("Sesión.py")).toBe("sesion.py");
    expect(foldEach("Sesión.py")).toHaveLength("Sesión.py".length);
    expect(fuzzyMatch("sion", "Sesión")!.positions).toEqual([2, 3, 4, 5]);
  });

  it("says which letters matched, every piece of the query", () => {
    expect(fuzzyMatch("cuenta", "Cambiar la cuenta de hedy")!.positions).toEqual([11, 12, 13, 14, 15, 16]);
    expect(fuzzyMatch("hedy cam", "Cambiar la cuenta de hedy")!.positions).toEqual([0, 1, 2, 21, 22, 23, 24]);
    expect(fuzzyMatch("xyz", "Cambiar la cuenta de hedy")).toBeNull();
    expect(fuzzyMatch("", "lo que sea")).toEqual({ score: 0, positions: [] });
  });

  it("prefers consecutive letters and the starts of words", () => {
    // "iva" as a run at a word start beats letters spread out.
    const run = fuzzyMatch("iva", "iva_total")!.score;
    const spread = fuzzyMatch("iva", "imprimir_valor")!.score;
    expect(run).toBeGreaterThan(spread);
    // Word starts: "_", ".", "/", "-" and camelCase humps.
    expect(fuzzyMatch("it", "iva_total")!.positions).toEqual([0, 4]);
    expect(fuzzyMatch("gc", "getConfig")!.positions).toEqual([0, 3]);
  });
});

describe("fuzzy paths", () => {
  it("ranks a match in the file name over one spread over the folders", () => {
    const name = fuzzyPath("tot", "facturas/totales.py")!.score;
    const folders = fuzzyPath("tot", "tests/other/test_utils.py")!.score;
    expect(name).toBeGreaterThan(folders);
  });

  it("ranks the exact name, then its start, then a match inside it", () => {
    const exact = fuzzyPath("pdf.py", "facturas/pdf.py")!.score;
    const prefix = fuzzyPath("pdf", "facturas/pdf.py")!.score;
    const inside = fuzzyPath("df", "facturas/pdf.py")!.score;
    expect(exact).toBeGreaterThan(prefix);
    expect(prefix).toBeGreaterThan(inside);
  });

  it("positions index the whole path", () => {
    expect(fuzzyPath("imp", "facturas/impuestos.py")!.positions).toEqual([9, 10, 11]);
    // A piece with a slash matches across folders.
    expect(fuzzyPath("fac/imp", "facturas/impuestos.py")!.positions).toEqual([0, 1, 2, 8, 9, 10, 11]);
  });

  it("matches initials across folders and words", () => {
    const m = fuzzyPath("fimp", "facturas/impuestos.py")!;
    expect(m.positions).toEqual([0, 9, 10, 11]);
    expect(fuzzyPath("zz", "facturas/impuestos.py")).toBeNull();
  });

  it("needs every piece and ignores case and accents", () => {
    expect(fuzzyPath("test fact", "tests/test_facturas.py")).not.toBeNull();
    expect(fuzzyPath("test citas", "tests/test_facturas.py")).toBeNull();
    expect(fuzzyPath("README", "readme.md")).not.toBeNull();
    expect(fuzzyPath("nino", "docs/Niño.md")).not.toBeNull();
  });

  it("sorts a realistic list as VS Code would", () => {
    const paths = [
      "tests/test_facturas.py",
      "facturas/totales.py",
      "facturas/__init__.py",
      "docs/facturacion.md",
      "facturas/impuestos.py",
    ];
    const rank = (q: string) =>
      paths
        .map((p) => ({ p, m: fuzzyPath(q, p) }))
        .filter((x) => x.m)
        .sort((a, b) => b.m!.score - a.m!.score)
        .map((x) => x.p);
    expect(rank("imp")[0]).toBe("facturas/impuestos.py");
    expect(rank("tot")[0]).toBe("facturas/totales.py");
    expect(rank("factura")[0]).toBe("docs/facturacion.md");
    expect(rank("testfac")[0]).toBe("tests/test_facturas.py");
  });
});
