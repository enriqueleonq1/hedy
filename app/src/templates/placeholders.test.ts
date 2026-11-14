import type { PromptTemplate } from "../api/types";
import { fillTemplate, filterTemplates, placeholders, slashQuery } from "./placeholders";

const template = (name: string, text = name): PromptTemplate => ({
  id: name,
  project: null,
  name,
  text,
  builtin: null,
  placeholders: placeholders(text),
});

describe("template placeholders", () => {
  it("finds each placeholder once, like the backend", () => {
    expect(placeholders("Pruebas para {archivo} con {marco_de-tests}; {archivo}")).toEqual(["archivo", "marco_de-tests"]);
    expect(placeholders("Explica {descripción}")).toEqual(["descripción"]);
    expect(placeholders('Devuelve {"a": 1} y { } y {}')).toEqual([]);
  });

  it("fills the placeholders and keeps the empty ones", () => {
    expect(fillTemplate("Pruebas para {archivo} en {rama}", { archivo: " src/citas.py ", rama: "" })).toBe(
      "Pruebas para src/citas.py en {rama}",
    );
  });

  it("filters by name first, then by text, without accents or case", () => {
    const list = [template("Revisar el cambio"), template("Escribir pruebas", "pruebas de revisión"), template("Explicar código")];
    expect(filterTemplates(list, "rev").map((t) => t.name)).toEqual(["Revisar el cambio", "Escribir pruebas"]);
    expect(filterTemplates(list, "CODIGO").map((t) => t.name)).toEqual(["Explicar código"]);
    expect(filterTemplates(list, "")).toHaveLength(3);
    expect(filterTemplates(list, "zzz")).toEqual([]);
  });

  it("opens the picker only for a leading slash on one line", () => {
    expect(slashQuery("/pru")).toBe("pru");
    expect(slashQuery("/")).toBe("");
    expect(slashQuery("hola /x")).toBeNull();
    expect(slashQuery("/a\nb")).toBeNull();
  });
});
