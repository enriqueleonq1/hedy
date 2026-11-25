import { act, render, screen, waitFor, within } from "@testing-library/react";
import { EditorView } from "@codemirror/view";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../App";
import { MockApi } from "../api/MockApi";

vi.mock("../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

const SLOW = { timeout: 8000 };

function setup(hash: string) {
  const api = MockApi.withSampleData().withSampleActivity();
  api.delayMs = 0;
  history.replaceState(null, "", hash);
  render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
  return { api, user: userEvent.setup() };
}

describe("side panel in the app", () => {
  it("focuses the files from the palette's explorer command", async () => {
    const { user } = setup("#p=%2Fcode%2Fhedy&t=s5");
    await screen.findByRole("heading", { name: /Arregla el cálculo de impuestos/ }, SLOW);
    expect(await screen.findByRole("tab", { name: /^Cambios \d+$/ }, SLOW)).toHaveAttribute("aria-selected", "true");
    await user.keyboard("{Control>}k{/Control}");
    await user.type(screen.getByRole("combobox", { name: "Paleta de comandos" }), "archivos");
    await user.keyboard("{Enter}");
    await waitFor(() => expect(screen.getByRole("tab", { name: "Archivos" })).toHaveAttribute("aria-selected", "true"), SLOW);
    await waitFor(() => expect(screen.getByRole("tree", { name: "Archivos" })).toHaveFocus(), SLOW);
  });

  it("fixes a line from the review in two clicks and shows the edit in the review", async () => {
    const { api, user } = setup("#p=%2Fcode%2Fhedy&t=s5");
    await screen.findByRole("heading", { name: /Arregla el cálculo de impuestos/ }, SLOW);
    const review = await screen.findByRole("region", { name: "facturas/impuestos.py" }, SLOW);
    await user.click(within(review).getByRole("button", { name: "Editar aquí" }));
    const editor = await screen.findByRole("textbox", { name: "Contenido de impuestos.py" }, SLOW);
    await waitFor(() => expect(editor).toHaveAttribute("aria-readonly", "false"), SLOW);
    expect(screen.getByRole("tab", { name: "Archivos" })).toHaveAttribute("aria-selected", "true");

    const view = EditorView.findFromDOM(document.querySelector(".cm-editor") as HTMLElement);
    act(() => view?.dispatch({ changes: { from: 0, insert: "# corregido por mí\n" } }));
    await user.click(await screen.findByRole("button", { name: "Guardar" }));
    await screen.findByText("Guardado", undefined, SLOW);

    await user.click(screen.getByRole("tab", { name: /^Cambios \d+$/ }));
    const updated = await screen.findByRole("region", { name: "facturas/impuestos.py" }, SLOW);
    expect(await within(updated).findByText("# corregido por mí", undefined, SLOW)).toBeInTheDocument();
    expect((await api.taskDiff("s5")).find((f) => f.path === "facturas/impuestos.py")?.additions).toBeGreaterThan(1);
  });

  it("opens a file the agent edited from the timeline, in the task's copy", async () => {
    const { user } = setup("#p=%2Fcode%2Fhedy&t=s5");
    await screen.findByRole("heading", { name: /Arregla el cálculo de impuestos/ }, SLOW);
    const chat = screen.getByRole("region", { name: "Chat" });
    // Its tests failed: the run of steps is open already.
    expect(await within(chat).findByRole("button", { name: /pasos/ }, SLOW)).toHaveAttribute("aria-expanded", "true");
    await user.click(within(chat).getByRole("button", { name: "facturas/impuestos.py" }));
    expect(await screen.findByRole("textbox", { name: "Contenido de impuestos.py" }, SLOW)).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Qué copia se muestra" })).toHaveTextContent(/Tarea: Arregla el cálculo/);
  });
});
