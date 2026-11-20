import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../App";
import { MockApi } from "../api/MockApi";

vi.mock("../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));
const SLOW = { timeout: 8000 };

async function open() {
  const api = MockApi.withSampleData().withSampleActivity();
  api.delayMs = 0;
  render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: /^Actividad: / }, SLOW));
  await user.click(await screen.findByRole("button", { name: /Arregla el cálculo de impuestos/ }, SLOW));
  await screen.findByRole("list", { name: "Archivos cambiados" }, SLOW);
  const bar = screen.getByRole("region", { name: "Comprobar con tus reglas" });
  await user.click(within(bar).getByRole("button", { name: "Comprobar con tus reglas" }));
  await within(bar).findByText("2 líneas no siguen tus reglas");
  return user;
}

it("knows a warning is already in the comments after a reload", async () => {
  const user = await open();
  const table = () => screen.getByRole("table", { name: /impuestos/ });
  await user.click(within(table()).getByRole("button", { name: "Pedir que lo corrija" }));
  cleanup();
  history.replaceState(null, "", "#");
  await open();
  expect(within(table()).queryByRole("button", { name: "Pedir que lo corrija" })).toBeNull();
});
