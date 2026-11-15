import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";
import type { ReactNode } from "react";

import { App } from "../../App";
import type { CodeTree } from "../../api/codeTypes";
import { MockApi } from "../../api/MockApi";
import type { NavigatorTarget } from "../../files/navigator";

vi.mock("../Terminal/TerminalView", () => ({ TerminalView: () => null }));

// Every request to show a file, as the side panel would receive it.
const opened = vi.hoisted(() => [] as NavigatorTarget[]);
vi.mock("../../files/navigator", async (importOriginal) => {
  const mod = await importOriginal<typeof import("../../files/navigator")>();
  return {
    ...mod,
    FileNavigatorProvider: ({ children }: { children: ReactNode }) => (
      <mod.FileNavigatorProvider onOpen={(t) => opened.push(t)}>{children}</mod.FileNavigatorProvider>
    ),
  };
});

const TASK = "#p=%2Fcode%2Fhedy&t=s5";
const SLOW = { timeout: 8000 };

function setup(hash = TASK, api = MockApi.withSampleData().withSampleActivity()) {
  api.delayMs = 0;
  history.replaceState(null, "", hash);
  render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
  return { api, user: userEvent.setup() };
}

function field() {
  return screen.getByRole("combobox", { name: "Paleta de comandos" });
}

/** The labels of the options on screen, in order. */
function options(): string[] {
  return screen.getAllByRole("option").map((o) => o.querySelector(".palette__label")?.textContent ?? "");
}

async function workspace() {
  await screen.findByRole("heading", { name: /Arregla el cálculo de impuestos/ }, SLOW);
}

async function quickOpen(user: ReturnType<typeof userEvent.setup>) {
  await user.keyboard("{Control>}p{/Control}");
  return field();
}

/** Opens impuestos.py of task s5 through the palette, so it is the active file. */
async function openImpuestos(user: ReturnType<typeof userEvent.setup>) {
  await quickOpen(user);
  await user.type(field(), "impu");
  await waitFor(() => expect(options()[0]).toBe("impuestos.py"));
  await user.keyboard("{Enter}");
}

describe("Quick open (Ctrl P)", () => {
  beforeEach(() => {
    opened.length = 0;
  });
  afterEach(() => {
    history.replaceState(null, "", "#");
    localStorage.clear();
  });

  it("lists the open task's files, changed ones first with their status, and says which copy", async () => {
    const { user } = setup();
    await workspace();
    expect(await quickOpen(user)).toHaveFocus();
    expect(field()).toHaveAttribute("placeholder", "Busca un archivo por nombre (añade :línea para ir a ella)");
    const changed = await screen.findByRole("group", { name: "Con cambios" }, SLOW);
    const rows = within(changed).getAllByRole("option");
    expect(rows.map((r) => r.querySelector(".palette__label")?.textContent)).toEqual([
      "impuestos.py",
      "totales.py",
      "test_facturas.py",
    ]);
    expect(within(rows[0]!).getByLabelText("Modificado")).toHaveTextContent("M");
    expect(screen.getByRole("group", { name: "Todos los archivos" })).toBeInTheDocument();
    expect(await within(screen.getByRole("dialog")).findByText("Tarea: Arregla el cálculo de impuestos en las facturas")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Archivos" })).toHaveAttribute("aria-pressed", "true");
  });

  it("matches loosely, highlights what matched and opens the file through the navigator", async () => {
    const { user } = setup();
    await workspace();
    await quickOpen(user);
    await user.type(field(), "fimp");
    await waitFor(() => expect(options()[0]).toBe("impuestos.py"));
    const first = screen.getAllByRole("option")[0]!;
    expect([...first.querySelectorAll("mark")].map((m) => m.textContent)).toEqual(["imp", "f"]);
    expect(first).toHaveTextContent("facturas");
    await user.keyboard("{Enter}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(opened).toEqual([{ copy: { kind: "task", task: "s5" }, path: "facturas/impuestos.py", view: "file" }]);
  });

  it("opens the diff with Ctrl Enter or Ctrl click, and at a line with `name:line`", async () => {
    const { user } = setup();
    await workspace();
    await quickOpen(user);
    await user.type(field(), "totales");
    await waitFor(() => expect(options()[0]).toBe("totales.py"));
    expect(screen.getByText("Ver cambios")).toBeInTheDocument();
    await user.keyboard("{Control>}{Enter}{/Control}");
    expect(opened.at(-1)).toEqual({ copy: { kind: "task", task: "s5" }, path: "facturas/totales.py", view: "diff" });

    await quickOpen(user);
    await user.type(field(), "impuestos:16");
    await waitFor(() => expect(options()[0]).toBe("impuestos.py"));
    await user.keyboard("{Enter}");
    expect(opened.at(-1)).toMatchObject({ path: "facturas/impuestos.py", line: 16, view: "file" });

    await quickOpen(user);
    await user.type(field(), "test_fact");
    await waitFor(() => expect(options()[0]).toBe("test_facturas.py"));
    fireEvent.click(screen.getAllByRole("option")[0]!, { ctrlKey: true });
    expect(opened.at(-1)).toMatchObject({ path: "tests/test_facturas.py", view: "diff" });
  });

  it("remembers recent files per project", async () => {
    const { user } = setup();
    await workspace();
    await openImpuestos(user);
    await quickOpen(user);
    const recent = await screen.findByRole("group", { name: "Recientes" }, SLOW);
    expect(within(recent).getAllByRole("option").map((o) => o.querySelector(".palette__label")?.textContent)).toEqual([
      "impuestos.py",
    ]);
    expect(options()[0]).toBe("impuestos.py");
    expect(JSON.parse(localStorage.getItem("hedy.palette.recentFiles") ?? "{}")).toEqual({
      "/code/hedy": ["facturas/impuestos.py"],
    });
  });

  it("without a project, says so and offers the projects, staying open to search there", async () => {
    const { user } = setup("#");
    await screen.findByRole("heading", { name: /¡Hola, Enrique!/ });
    await quickOpen(user);
    expect(screen.getByText("Abre un proyecto para buscar en sus archivos.")).toBeInTheDocument();
    expect(options()).toEqual(["hedy", "homming"]);
    await user.click(screen.getAllByRole("option")[0]!);
    expect(screen.getByRole("dialog", { name: "Paleta de comandos" })).toBeInTheDocument();
    expect(await screen.findByRole("group", { name: "Todos los archivos" }, SLOW)).toBeInTheDocument();
    // The workspace opens on its most recent open task: that is the copy searched.
    expect(screen.getByTitle(/^hedy · Tarea: /)).toBeInTheDocument();
  });

  it("does not take Ctrl P from a terminal, and takes it from the composer without touching its text", async () => {
    const { user } = setup();
    await workspace();
    const terminal = document.createElement("div");
    terminal.className = "xterm";
    const input = document.createElement("textarea");
    terminal.append(input);
    document.body.append(terminal);
    input.focus();
    await user.keyboard("{Control>}p{/Control}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    terminal.remove();

    const composer = screen.getByPlaceholderText(/Responde o pide algo más/);
    await user.click(composer);
    await user.type(composer, "hola");
    await user.keyboard("{Control>}p{/Control}");
    expect(field()).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(composer).toHaveValue("hola");
    expect(composer).toHaveFocus();
  });

  it("renders only the rows in view of a huge tree", async () => {
    const api = MockApi.withSampleData().withSampleActivity();
    const tree = await api.codeTree({ kind: "task", task: "s5" });
    const entries = Array.from({ length: 5000 }, (_, i) => ({
      path: `src/modulo${Math.floor(i / 100)}/archivo${i}.ts`,
      status: null,
      oldPath: null,
      locked: false,
      ignored: false,
    }));
    vi.spyOn(api, "codeTree").mockResolvedValue({ ...tree, entries } satisfies CodeTree);
    const { user } = setup(TASK, api);
    await workspace();
    await quickOpen(user);
    await screen.findByRole("group", { name: "Todos los archivos" }, SLOW);
    expect(screen.getAllByRole("option").length).toBeLessThan(80);
    expect(within(screen.getByRole("dialog")).getByRole("status")).toHaveTextContent("5000 resultados");
    // The last one is rendered once it is the active one.
    await user.keyboard("{ArrowUp}");
    const last = screen.getAllByRole("option").at(-1)!;
    expect(last).toHaveAttribute("aria-selected", "true");
    expect(last).toHaveTextContent("archivo4999.ts");
    expect(field()).toHaveAttribute("aria-activedescendant", last.id);
  });
});

describe("Palette modes", () => {
  beforeEach(() => {
    opened.length = 0;
  });
  afterEach(() => {
    history.replaceState(null, "", "#");
    localStorage.clear();
  });

  it("F1 and Ctrl Shift P open the commands, each with its shortcut", async () => {
    const { user } = setup();
    await workspace();
    await user.keyboard("{F1}");
    expect(field()).toHaveValue(">");
    expect(screen.getByRole("button", { name: /Comandos/ })).toHaveAttribute("aria-pressed", "true");
    expect(field()).toHaveAttribute("placeholder", "Escribe un comando");
    await user.keyboard("{Escape}");
    await user.keyboard("{Control>}{Shift>}P{/Shift}{/Control}");
    expect(field()).toHaveValue(">");
    await user.type(field(), "abrir un archivo");
    await waitFor(() => expect(options()[0]).toBe("Abrir un archivo…"));
    expect(screen.getAllByRole("option")[0]!.querySelector(".palette__keys")).toHaveAccessibleName("Ctrl P");
    // A mode command switches the palette instead of closing it.
    await user.keyboard("{Enter}");
    expect(field()).toHaveValue("");
    expect(screen.getByRole("button", { name: "Archivos" })).toHaveAttribute("aria-pressed", "true");
  });

  it("offers the code commands for the active file and the folder's pull and push", async () => {
    const { api, user } = setup();
    const reveal = vi.spyOn(api, "revealInFileManager");
    const push = vi.spyOn(api, "pushFolder");
    await workspace();
    await openImpuestos(user);
    await user.keyboard("{F1}");
    const code = await screen.findByRole("group", { name: "Código" });
    const labels = within(code)
      .getAllByRole("option")
      .map((o) => o.querySelector(".palette__label")?.textContent);
    for (const label of [
      "Ir a una línea…",
      "Ir a un símbolo del archivo…",
      "Abrir la búsqueda en archivos",
      "Abrir la revisión",
      "Ver el archivo antes de la IA",
      "Abrir el archivo en VS Code",
      "Copiar la ruta del archivo",
      "Mostrar el archivo en su carpeta",
      "Publicar la rama de tu carpeta (push)",
    ]) {
      expect(labels).toContain(label);
    }
    await user.type(field(), "mostrar el archivo");
    await user.keyboard("{Enter}");
    expect(reveal).toHaveBeenCalledWith({ kind: "task", task: "s5" }, "facturas/impuestos.py");

    await user.keyboard("{F1}");
    await user.type(field(), "ver el archivo antes");
    await user.keyboard("{Enter}");
    expect(opened.at(-1)).toEqual({ copy: { kind: "task", task: "s5" }, path: "facturas/impuestos.py", rev: "base", view: "file" });

    await user.keyboard("{F1}");
    await user.type(field(), "push");
    await user.keyboard("{Enter}");
    await waitFor(() => expect(push).toHaveBeenCalledWith("/code/hedy"));
  });

  it("lists the modes with ?, completes a prefix with Tab and goes back with Backspace", async () => {
    const { user } = setup();
    await workspace();
    await quickOpen(user);
    // The hint row says how to reach each mode.
    expect(screen.getByRole("button", { name: "> comandos" })).toBeInTheDocument();
    await user.type(field(), "?");
    expect(options()).toEqual(["Abrir un archivo", "Ejecutar un comando", "Ir a una línea", "Ir a un símbolo", "Buscar texto"]);
    await user.keyboard("{ArrowDown}{ArrowDown}{ArrowDown}{Tab}");
    expect(field()).toHaveValue("@");
    expect(field()).toHaveFocus();
    await user.keyboard("{Backspace}");
    expect(field()).toHaveValue("");
    expect(screen.getByRole("button", { name: "Archivos" })).toHaveAttribute("aria-pressed", "true");
  });

  it("goes to a line of the active file with `:`", async () => {
    const { user } = setup();
    await workspace();
    await quickOpen(user);
    await user.type(field(), ":");
    expect(screen.getByText("Abre un archivo primero (Ctrl P o el explorador).")).toBeInTheDocument();
    await user.keyboard("{Escape}");

    await openImpuestos(user);
    await quickOpen(user);
    await user.type(field(), ":");
    expect(await screen.findByText("Escribe una línea de impuestos.py, entre 1 y 23.")).toBeInTheDocument();
    await user.type(field(), "12:4");
    expect(options()).toEqual(["Ir a la línea 12"]);
    await user.keyboard("{Enter}");
    expect(opened.at(-1)).toEqual({ copy: { kind: "task", task: "s5" }, path: "facturas/impuestos.py", line: 12, view: "file" });

    // Past the end, the last line.
    await quickOpen(user);
    await user.type(field(), ":999");
    expect(options()).toEqual(["Ir a la línea 23"]);
  });

  it("goes to a symbol of the active file with `@`, grouped by kind", async () => {
    const { user } = setup();
    await workspace();
    await openImpuestos(user);
    await quickOpen(user);
    await user.type(field(), "@");
    const functions = await screen.findByRole("group", { name: "Funciones" });
    expect(within(functions).getAllByRole("option").map((o) => o.querySelector(".palette__label")?.textContent)).toEqual([
      "redondear",
      "iva_de_linea",
      "iva_total",
    ]);
    await user.type(field(), "ivt");
    expect(options()[0]).toBe("iva_total");
    await user.keyboard("{Enter}");
    expect(opened.at(-1)).toEqual({ copy: { kind: "task", task: "s5" }, path: "facturas/impuestos.py", line: 21, view: "file" });
  });

  it("searches text across the copy with `#` and opens a match at its line", async () => {
    const { api, user } = setup();
    const search = vi.spyOn(api, "searchText");
    await workspace();
    await quickOpen(user);
    await user.type(field(), "#iva_total");
    const group = await screen.findByRole("group", { name: "facturas/impuestos.py" }, SLOW);
    // Debounced: one search for the whole word, not one per letter.
    expect(search).toHaveBeenCalledTimes(1);
    expect(search).toHaveBeenCalledWith({ kind: "task", task: "s5" }, "iva_total");
    const row = within(group).getAllByRole("option")[0]!;
    expect(row.querySelector("mark")).toHaveTextContent("iva_total");
    expect(row).toHaveTextContent("21");
    await user.click(row);
    expect(opened.at(-1)).toEqual({ copy: { kind: "task", task: "s5" }, path: "facturas/impuestos.py", line: 21, view: "file" });
  });

  it("switches modes with the chips and the hint row, and highlights the row under the mouse", async () => {
    const { user } = setup();
    await workspace();
    await quickOpen(user);
    await user.type(field(), "total");
    await user.click(screen.getByRole("button", { name: /Texto/ }));
    expect(field()).toHaveValue("#total");
    expect(field()).toHaveFocus();
    await user.click(screen.getByRole("button", { name: /Comandos/ }));
    expect(field()).toHaveValue(">total");
    await user.click(screen.getByRole("button", { name: "Archivos" }));
    expect(field()).toHaveValue("total");
    await user.clear(field());
    await user.click(screen.getByRole("button", { name: "? ayuda" }));
    expect(field()).toHaveValue("?");
    const rows = screen.getAllByRole("option");
    await user.hover(rows[2]!);
    expect(rows[2]).toHaveAttribute("aria-selected", "true");
    await user.click(rows[2]!);
    expect(field()).toHaveValue(":");
  });

  it("finds files from Ctrl K too, after the commands", async () => {
    const { user } = setup();
    await workspace();
    await user.keyboard("{Control>}k{/Control}");
    expect(field()).toHaveAttribute("placeholder", "Busca un proyecto, una tarea o una acción");
    await user.type(field(), "impuestos");
    const files = await screen.findByRole("group", { name: "Archivos" }, SLOW);
    expect(within(files).getAllByRole("option")[0]).toHaveTextContent("impuestos.py");
  });

  it("lists the new shortcuts in the ? overlay", async () => {
    const { user } = setup("#");
    await screen.findByRole("heading", { name: /¡Hola, Enrique!/ });
    await act(async () => {
      await user.keyboard("?");
    });
    const dialog = screen.getByRole("dialog", { name: "Atajos de teclado" });
    expect(within(dialog).getByText("Abrir un archivo del proyecto")).toBeInTheDocument();
    expect(within(dialog).getByText("Ejecutar un comando")).toBeInTheDocument();
    expect(within(dialog).getByText("F1")).toBeInTheDocument();
  });
});
