import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { CodeCopy } from "../../api/codeTypes";
import { renderCode } from "../testing";
import { previewLine } from "./searchModel";
import { Highlighted, SearchView } from "./SearchView";

const S5: CodeCopy = { kind: "task", task: "s5" };

function setup() {
  const onOpen = vi.fn();
  const view = renderCode(<SearchView copy={S5} info={undefined} onOpen={onOpen} openKey={null} />);
  return { ...view, onOpen, user: userEvent.setup() };
}

describe("SearchView", () => {
  it("groups matches by file, highlights them and opens one at its line", async () => {
    const { onOpen, user } = setup();
    expect(screen.getByText("Busca texto en esta copia")).toBeInTheDocument();
    await user.type(screen.getByRole("searchbox", { name: "Buscar en los archivos" }), "redondear");
    const results = await screen.findByRole("tree", { name: "Resultados de la búsqueda" });
    expect(screen.getByText(/coincidencias en \d+ archivos/)).toBeInTheDocument();
    const file = within(results).getByRole("treeitem", { name: /impuestos\.py/ });
    expect(file).toHaveAttribute("aria-expanded", "true");
    expect(results.querySelector("mark")).toHaveTextContent("redondear");
    const match = within(results).getAllByRole("treeitem", { name: /^Línea \d+:/ })[0]!;
    await user.click(match);
    expect(onOpen).toHaveBeenCalledWith(
      expect.stringMatching(/\.py$/),
      expect.any(Number),
      expect.arrayContaining([expect.objectContaining({ start: expect.any(Number) })]),
      expect.any(String),
    );
    // A file's matches fold away.
    await user.click(file);
    expect(file).toHaveAttribute("aria-expanded", "false");
  });

  it("toggles case, whole word and regex, and says when nothing matches or the query is wrong", async () => {
    const { user } = setup();
    const field = screen.getByRole("searchbox", { name: "Buscar en los archivos" });
    await user.type(field, "IVA");
    await screen.findByRole("tree", { name: "Resultados de la búsqueda" });
    const caseButton = screen.getByRole("button", { name: "Coincidir mayúsculas y minúsculas" });
    fireEvent.keyDown(field, { key: "c", altKey: true });
    expect(caseButton).toHaveAttribute("aria-pressed", "true");
    await user.clear(field);
    await user.type(field, "zzzz-no-existe");
    expect(await screen.findByText("Sin resultados")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Expresión regular" }));
    await user.clear(field);
    await user.type(field, "(");
    expect(await screen.findByText("La búsqueda falló")).toBeInTheDocument();
  });

  it("filters by the files to include", async () => {
    const { user } = setup();
    await user.click(screen.getByRole("button", { name: "Archivos a incluir" }));
    await user.type(screen.getByRole("textbox", { name: "Archivos a incluir" }), "*.md");
    await user.type(screen.getByRole("searchbox", { name: "Buscar en los archivos" }), "IVA");
    const results = await screen.findByRole("tree", { name: "Resultados de la búsqueda" });
    const files = within(results).getAllByRole("treeitem").filter((row) => row.getAttribute("aria-level") === "1");
    expect(files.length).toBeGreaterThan(0);
    for (const row of files) expect(row).toHaveTextContent(/\.md/);
  });
});

describe("search models", () => {
  it("cuts a long line before its first match and moves the ranges", () => {
    const text = `    ${"x".repeat(60)}needle`;
    const cut = previewLine(text, [{ start: 64, end: 70 }]);
    expect(cut.cut).toBe(true);
    expect(cut.text.slice(cut.ranges[0]!.start, cut.ranges[0]!.end)).toBe("needle");
    const short = previewLine("  a needle", [{ start: 4, end: 10 }]);
    expect(short).toEqual({ text: "a needle", ranges: [{ start: 2, end: 8 }], cut: false });
  });

  it("marks every range", () => {
    render(
      <p>
        <Highlighted text="ab ab" ranges={[{ start: 3, end: 5 }, { start: 0, end: 2 }]} />
      </p>,
    );
    expect(document.querySelectorAll("mark")).toHaveLength(2);
  });
});
