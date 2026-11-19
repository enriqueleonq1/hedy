import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../App";
import { MockApi } from "../api/MockApi";

vi.mock("../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

const SLOW = { timeout: 8000 };
const TASK = "Arregla el cálculo de impuestos en las facturas";

function setup(language: "es" | "en" = "es") {
  const api = MockApi.withSampleData().withSampleActivity();
  api.delayMs = 0;
  if (language === "en") void api.setSetting("ui.language", "en");
  const check = vi.spyOn(api, "checkTaskRules");
  render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
  return { api, check, user: userEvent.setup() };
}

/** Opens the sample task with changes (hedy, "s5"): its comments are in Spanish. */
async function openTask(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole("button", { name: /^(Actividad|Activity): / }, SLOW));
  await user.click(await screen.findByRole("button", { name: new RegExp(TASK) }, SLOW));
  return screen.findByRole("list", { name: /Archivos cambiados|Changed files/ }, SLOW);
}

describe("Checking the changes against the rules", () => {
  afterEach(() => {
    history.replaceState(null, "", "#");
    localStorage.clear();
  });

  it("only spends tokens when asked, and shows each warning under its line", async () => {
    const { check, user } = setup();
    const files = await openTask(user);
    const bar = screen.getByRole("region", { name: "Revisar con tus reglas" });
    expect(within(bar).getByText(/Gasta unos pocos tokens/)).toBeInTheDocument();
    expect(check).not.toHaveBeenCalled();

    await user.click(within(bar).getByRole("button", { name: "Revisar con tus reglas" }));
    expect(await within(bar).findByText("2 líneas no siguen tus reglas")).toBeInTheDocument();
    expect(check).toHaveBeenCalledWith("s5");
    expect(within(bar).getByText(/^3 archivos contra \d+ reglas · con /)).toBeInTheDocument();
    // Each file with a warning says how many.
    expect(within(files).getAllByTitle("1 aviso de tus reglas")).toHaveLength(2);

    // Under the line, the rule it cites and what to do.
    const table = screen.getByRole("table", { name: /facturas\/impuestos\.py/ });
    const card = within(table).getByRole("listitem");
    expect(within(card).getByText("Los comentarios del código van en inglés")).toBeInTheDocument();
    expect(within(card).getByText("El comentario está en español.")).toBeInTheDocument();
    expect(within(card).getByText("Sugerencia: Escríbelo en inglés.")).toBeInTheDocument();

    // "Pedir que lo corrija": it becomes a comment for the AI, sent with the others.
    await user.click(within(card).getByRole("button", { name: "Pedir que lo corrija" }));
    expect(within(card).getByText("En tus comentarios")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "1 comentario" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "1 comentario" }));
    expect(screen.getByLabelText("Mensaje para la IA")).toHaveTextContent(
      "No sigue la regla «Los comentarios del código van en inglés»: El comentario está en español. Sugerencia: Escríbelo en inglés.",
    );
  });

  it("forgets a warning the user says is fine, and can show it again", async () => {
    const { user } = setup();
    const files = await openTask(user);
    const bar = screen.getByRole("region", { name: "Revisar con tus reglas" });
    await user.click(within(bar).getByRole("button", { name: "Revisar con tus reglas" }));
    await within(bar).findByText("2 líneas no siguen tus reglas");

    await user.click(within(screen.getByRole("table", { name: /impuestos/ })).getByRole("button", { name: "Está bien así" }));
    expect(within(bar).getByText("1 línea no sigue tus reglas")).toBeInTheDocument();
    expect(within(files).getAllByTitle("1 aviso de tus reglas")).toHaveLength(1);
    expect(within(bar).getByText(/1 descartado/)).toBeInTheDocument();

    // "Pedir que corrija todo" adds what is left; nothing is sent until the user sends it.
    await user.click(within(bar).getByRole("button", { name: "Pedir que corrija todo" }));
    expect(screen.getByRole("button", { name: "1 comentario" })).toBeInTheDocument();
    expect(within(bar).queryByRole("button", { name: "Pedir que corrija todo" })).toBeNull();

    await user.click(within(bar).getByRole("button", { name: "Mostrar" }));
    expect(within(bar).getByText("2 líneas no siguen tus reglas")).toBeInTheDocument();
    expect(within(bar).getByRole("button", { name: "Volver a revisar" })).toBeInTheDocument();
  });

  it("speaks English", async () => {
    const { user } = setup("en");
    await openTask(user);
    const bar = screen.getByRole("region", { name: "Check against your rules" });
    await user.click(within(bar).getByRole("button", { name: "Check against your rules" }));
    expect(await within(bar).findByText("2 lines do not follow your rules")).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "Ask to fix it" }).length).toBeGreaterThan(0);
    expect(screen.getAllByRole("button", { name: "It is fine" }).length).toBeGreaterThan(0);
  });
});
