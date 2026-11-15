import { render, screen, waitFor, within } from "@testing-library/react";
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
  it("focuses the explorer from the palette's Explorador command", async () => {
    const { user } = setup("#p=%2Fcode%2Fhedy&t=s5");
    await screen.findByRole("heading", { name: /Arregla el cálculo de impuestos/ }, SLOW);
    expect(await screen.findByRole("tab", { name: "Revisión" }, SLOW)).toHaveAttribute("aria-selected", "true");
    await user.keyboard("{Control>}k{/Control}");
    await user.type(screen.getByRole("combobox", { name: "Paleta de comandos" }), "explorador");
    await user.keyboard("{Enter}");
    await waitFor(() => expect(screen.getByRole("tab", { name: "Explorador" })).toHaveAttribute("aria-selected", "true"), SLOW);
    await waitFor(() => expect(screen.getByRole("tree", { name: "Archivos" })).toHaveFocus(), SLOW);
  });

  it("opens a file the agent edited from the timeline, in the task's copy", async () => {
    const { user } = setup("#p=%2Fcode%2Fhedy&t=s5");
    await screen.findByRole("heading", { name: /Arregla el cálculo de impuestos/ }, SLOW);
    const chat = screen.getByRole("region", { name: "Chat" });
    await user.click(await within(chat).findByRole("button", { name: /pasos/ }, SLOW));
    await user.click(within(chat).getByRole("button", { name: "facturas/impuestos.py" }));
    expect(await screen.findByRole("textbox", { name: "Contenido de impuestos.py" }, SLOW)).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Qué copia se muestra" })).toHaveTextContent(/Tarea: Arregla el cálculo/);
  });
});
