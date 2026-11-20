import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../App";
import { MockApi } from "../api/MockApi";

vi.mock("../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

const SLOW = { timeout: 8000 };
const TASK = "Arregla el cálculo de impuestos en las facturas";
const DIFF = "Cambios en facturas/impuestos.py";

function setup() {
  const api = MockApi.withSampleData().withSampleActivity();
  api.delayMs = 0;
  render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
  return { api, user: userEvent.setup() };
}

/** Opens the sample task with changes (hedy, "s5") and waits for its full diff. */
async function openReview(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole("button", { name: /^Inicio/ }, SLOW));
  await user.click(await screen.findByRole("button", { name: new RegExp(TASK) }, SLOW));
  const table = await screen.findByRole("table", { name: DIFF }, SLOW);
  // The whole file as context has arrived: the gap above the change can be expanded.
  await within(table).findByRole("button", { name: "Mostrar 12 líneas" }, SLOW);
  return table;
}

const row = (table: HTMLElement, newLine: number) => {
  const el = table.querySelector<HTMLElement>(`[data-line-new="${newLine}"]`);
  if (!el) throw new Error(`no row for new line ${newLine}`);
  return el;
};

describe("Line comments in the review", () => {
  afterEach(() => {
    history.replaceState(null, "", "#");
    localStorage.clear();
  });

  it("comments on a line from the gutter's +, and on a range by dragging", async () => {
    const { user } = setup();
    const table = await openReview(user);

    fireEvent.mouseDown(within(table).getByRole("button", { name: "Comentar la línea 16" }));
    fireEvent.mouseUp(window);
    const field = await screen.findByRole("textbox", { name: "Comentario sobre facturas/impuestos.py:16" });
    expect(field).toHaveFocus();
    await user.type(field, "¿Y si la cantidad es cero?");
    await user.keyboard("{Control>}{Enter}{/Control}");

    expect(screen.queryByRole("textbox", { name: /Comentario sobre/ })).toBeNull();
    expect(within(table).getByText("¿Y si la cantidad es cero?")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "1 comentario" })).toBeInTheDocument();
    const files = screen.getByRole("list", { name: "Archivos cambiados" });
    expect(within(files).getByRole("button", { name: /^facturas\/impuestos\.py.*1 comentario por enviar/ })).toBeInTheDocument();

    // A drag from line 16 to 18 comments on the range.
    fireEvent.mouseDown(within(table).getByRole("button", { name: "Comentar la línea 16" }));
    fireEvent.mouseEnter(row(table, 18));
    fireEvent.mouseUp(window);
    expect(await screen.findByRole("textbox", { name: "Comentario sobre facturas/impuestos.py:16-18" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Cancelar" }));
    expect(screen.queryByRole("textbox", { name: /Comentario sobre/ })).toBeNull();

    // Or a click on a line number, then Shift and the + of the last line.
    fireEvent.mouseDown(row(table, 17).querySelectorAll(".rd-no")[1] as HTMLElement);
    fireEvent.mouseDown(within(table).getByRole("button", { name: "Comentar la línea 18" }), { shiftKey: true });
    expect(await screen.findByRole("textbox", { name: "Comentario sobre facturas/impuestos.py:17-18" })).toBeInTheDocument();
  });

  it("comments from the keyboard: arrows move, Shift selects, C opens, Esc cancels", async () => {
    const { user } = setup();
    const table = await openReview(user);

    row(table, 16).focus();
    await user.keyboard("{Shift>}{ArrowDown}{ArrowDown}{/Shift}");
    expect(row(table, 18)).toHaveFocus();
    expect(row(table, 17)).toHaveAttribute("aria-selected", "true");
    await user.keyboard("c");
    const field = await screen.findByRole("textbox", { name: "Comentario sobre facturas/impuestos.py:16-18" });
    await waitFor(() => expect(field).toHaveFocus());
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("textbox", { name: /Comentario sobre/ })).toBeNull();

    await waitFor(() => expect(row(table, 18)).toHaveFocus());
    await user.keyboard("{Enter}");
    await user.type(await screen.findByRole("textbox", { name: "Comentario sobre facturas/impuestos.py:18" }), "Usa quantize");
    await user.click(screen.getByRole("button", { name: "Agregar comentario" }));
    expect(within(table).getByText("Usa quantize")).toBeInTheDocument();

    // Edited and deleted from the comment's own actions.
    await user.click(within(table).getByRole("button", { name: "Editar comentario" }));
    const edit = screen.getByRole("textbox", { name: "Comentario sobre facturas/impuestos.py:18" });
    await user.clear(edit);
    await user.type(edit, "Usa Decimal.quantize");
    await user.click(screen.getByRole("button", { name: "Guardar" }));
    expect(within(table).getByText("Usa Decimal.quantize")).toBeInTheDocument();
    await user.click(within(table).getByRole("button", { name: "Quitar comentario" }));
    expect(within(table).queryByText("Usa Decimal.quantize")).toBeNull();
  });

  it("comments from a line's right-click menu", async () => {
    const { user } = setup();
    const table = await openReview(user);
    fireEvent.contextMenu(row(table, 17), { clientX: 200, clientY: 300 });
    const menu = screen.getByRole("menu", { name: "Acciones de la línea" });
    expect(within(menu).getAllByRole("menuitem").map((i) => i.textContent)).toEqual([
      "Comentar la línea 17C",
      "Copiar la línea",
      "Abrir el archivo en esta línea",
      "Editar aquí",
      "Abrir en VS Code en esta línea",
    ]);
    await user.click(within(menu).getByRole("menuitem", { name: /Comentar la línea 17/ }));
    expect(await screen.findByRole("textbox", { name: "Comentario sobre facturas/impuestos.py:17" })).toBeInTheDocument();
  });

  it("sends the comments to the AI as one follow-up and keeps them as sent", async () => {
    const { api, user } = setup();
    const queue = vi.spyOn(api, "queueMessage");
    const table = await openReview(user);

    for (const [line, text] of [
      [16, "¿Y si la cantidad es cero?"],
      [18, "Usa quantize"],
    ] as const) {
      fireEvent.mouseDown(within(table).getByRole("button", { name: `Comentar la línea ${line}` }));
      fireEvent.mouseUp(window);
      await user.type(await screen.findByRole("textbox", { name: /Comentario sobre/ }), text);
      await user.keyboard("{Control>}{Enter}{/Control}");
    }

    // The message, before sending.
    await user.click(screen.getByRole("button", { name: "2 comentarios" }));
    const preview = screen.getByLabelText("Mensaje para la IA");
    expect(preview).toHaveTextContent(/Revisé tus cambios y te dejé 2 comentarios/);
    expect(preview).toHaveTextContent("1. `facturas/impuestos.py:16`");

    await user.click(screen.getByRole("button", { name: "Enviar 2 comentarios" }));
    await waitFor(() => expect(queue).toHaveBeenCalledTimes(1));
    const [taskId, text] = queue.mock.calls[0] ?? [];
    expect(taskId).toBe("s5");
    expect(text).toContain("1. `facturas/impuestos.py:16`\n```\n    neto = precio * cantidad\n```\n¿Y si la cantidad es cero?");
    expect(text).toContain("2. `facturas/impuestos.py:18`\n```\n    return redondear(neto * IVA)\n```\nUsa quantize");

    // Sent: folded history, and the bar is back to the decision.
    expect(await screen.findByText("2 comentarios enviados")).toBeInTheDocument();
    expect(within(table).getAllByRole("button", { name: /Enviado/ })).toHaveLength(2);
    expect(screen.queryByRole("button", { name: /^Enviar \d+ comentarios?$/ })).toBeNull();
    // The follow-up is in the chat.
    const chat = screen.getByRole("region", { name: "Chat" });
    expect((await within(chat).findAllByText(/Revisé tus cambios y te dejé 2 comentarios/)).length).toBeGreaterThan(0);
  });

  it("sends one comment from its thread", async () => {
    const { api, user } = setup();
    const queue = vi.spyOn(api, "queueMessage");
    const table = await openReview(user);
    fireEvent.mouseDown(within(table).getByRole("button", { name: "Comentar la línea 16" }));
    fireEvent.mouseUp(window);
    await user.type(await screen.findByRole("textbox", { name: /Comentario sobre/ }), "Solo este");
    await user.keyboard("{Control>}{Enter}{/Control}");
    // The bar counts what it would send.
    await user.click(screen.getByRole("button", { name: "1 comentario" }));
    expect(screen.getByRole("button", { name: "Enviar 1 comentario" })).toBeInTheDocument();
    await user.click(within(table).getByRole("button", { name: "Enviar este comentario a la IA" }));
    await waitFor(() => expect(queue).toHaveBeenCalledWith("s5", expect.stringContaining("Solo este")));
    expect(queue.mock.calls[0]?.[1]).toMatch(/^Revisé tus cambios y te dejé un comentario\./);
  });

  it("expands the context between changes", async () => {
    const { user } = setup();
    const table = await openReview(user);
    expect(table.querySelector('[data-line-new="1"]')).toBeNull();
    await user.click(within(table).getByRole("button", { name: "Mostrar 12 líneas" }));
    expect(row(table, 1)).toBeInTheDocument();
    expect(within(table).queryByRole("button", { name: "Mostrar 12 líneas" })).toBeNull();
  });

  it("switches between the unified and the side-by-side view", async () => {
    const { user } = setup();
    const table = await openReview(user);
    expect(table).toHaveClass("rd--unified");
    await user.click(screen.getByRole("button", { name: "Vista dividida (lado a lado)" }));
    const split = screen.getByRole("table", { name: DIFF });
    expect(split).toHaveClass("rd--split");
    // The removed line sits next to the line that replaced it.
    const removed = split.querySelector('[data-row^="old-"].rd-row--removed');
    expect(removed?.parentElement?.querySelector(".rd-row--added")).toHaveTextContent("neto = precio * cantidad");
    expect(screen.getByRole("button", { name: "Vista dividida (lado a lado)" })).toHaveAttribute("aria-pressed", "true");
    await user.click(screen.getByRole("button", { name: "Vista unificada" }));
    expect(screen.getByRole("table", { name: DIFF })).toHaveClass("rd--unified");
  });

  it("offers a changed file's actions on right click", async () => {
    const { api, user } = setup();
    await openReview(user);
    const files = screen.getByRole("list", { name: "Archivos cambiados" });
    const item = within(files).getByRole("button", { name: /^facturas\/totales\.py/ }).closest("li") as HTMLElement;
    fireEvent.contextMenu(item, { clientX: 100, clientY: 100 });
    const menu = screen.getByRole("menu", { name: "Acciones de facturas/totales.py" });
    expect(within(menu).getAllByRole("menuitem").map((i) => i.textContent)).toEqual([
      "Abrir archivo",
      "Editar aquí",
      "Abrir en VS Code",
      "Copiar ruta",
      "Marcar como visto",
      "Historial",
    ]);
    await user.click(within(menu).getByRole("menuitem", { name: "Abrir en VS Code" }));
    await waitFor(() => expect(api.code.opened.at(-1)?.path).toMatch(/facturas\/totales\.py$/));

    fireEvent.contextMenu(item, { clientX: 100, clientY: 100 });
    await user.click(screen.getByRole("menuitem", { name: "Marcar como visto" }));
    expect(screen.getByText("1 de 3 vistos")).toBeInTheDocument();

    // The same actions behind the row's "⋯" button.
    await user.click(within(files).getByRole("button", { name: "Acciones de facturas/totales.py" }));
    await user.click(screen.getByRole("menuitem", { name: "Historial" }));
    expect(await screen.findByRole("dialog", { name: "Historial de facturas/totales.py" })).toBeInTheDocument();
  });

  it("turns the bar into the accept action once every file is reviewed", async () => {
    const { user } = setup();
    await openReview(user);
    const files = screen.getByRole("list", { name: "Archivos cambiados" });
    for (const box of within(files).getAllByRole("checkbox")) await user.click(box);
    expect(screen.getByText("Todo visto")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Aceptar cambios…" })).toHaveClass("btn--primary");
  });
});
