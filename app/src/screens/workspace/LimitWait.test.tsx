import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";

vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

const SLOW = { timeout: 8000 };
const TASK = "Traduce los correos de confirmación";

function setup(language: "es" | "en" = "es", api = MockApi.withSampleData().withSampleActivity()) {
  api.delayMs = 0;
  if (language === "en") void api.setSetting("ui.language", "en");
  render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
  return { api, user: userEvent.setup() };
}

/** The sample task at its limit (hedy, "s3"), waiting for the reset; opened from the board. */
async function openWaitingTask(user: ReturnType<typeof userEvent.setup>, board: RegExp, name: RegExp) {
  await user.click(await screen.findByRole("button", { name: board }, SLOW));
  await user.click(await screen.findByRole("button", { name: new RegExp(TASK) }, SLOW));
  return screen.findByRole("region", { name }, SLOW);
}

describe("Waiting for a limit", () => {
  afterEach(() => history.replaceState(null, "", "#"));

  it("counts down to the reset, survives navigation and can be cancelled", async () => {
    const { user } = setup();
    let card = await openWaitingTask(user, /^Actividad: /, /Claude personal llegó a su límite/);
    expect(
      within(card).getByText(/^Se renueva en 4\d min\. Entonces Hedy retoma la tarea con Claude personal\.$/),
    ).toBeInTheDocument();
    expect(within(card).getByText(/Hedy nunca cambia de cuenta por su cuenta/)).toBeInTheDocument();
    // The other options stay: another provider or an API key.
    expect((await within(card).findAllByRole("button", { name: /^Pasar la tarea a / })).length).toBeGreaterThan(0);
    expect(within(card).queryByRole("button", { name: "Esperar" })).toBeNull();

    // Somewhere else and back: still waiting (it is stored with the task).
    await user.click(screen.getByRole("button", { name: /^Actividad/ }));
    expect(await screen.findByText(/^sigue en 4\d min$/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: new RegExp(TASK) }));
    card = await screen.findByRole("region", { name: /Claude personal llegó a su límite/ }, SLOW);
    expect(within(card).getByText(/^Se renueva en 4\d min/)).toBeInTheDocument();

    await user.click(within(card).getByRole("button", { name: "Cancelar espera" }));
    const wait = await within(card).findByRole("button", { name: "Esperar" });
    expect(within(card).queryByText(/Entonces Hedy retoma/)).toBeNull();
    await user.click(wait);
    expect(await within(card).findByText(/^Se renueva en 4\d min\./)).toBeInTheDocument();
  });

  it("resumes on the same account at the reset, with the held message", async () => {
    const { api, user } = setup();
    const card = await openWaitingTask(user, /^Actividad: /, /Claude personal llegó a su límite/);
    await act(() => api.resumeDue("s3"));
    expect(await screen.findByText("Revisa también el asunto del correo.", { selector: ".bubble" })).toBeInTheDocument();
    await waitFor(() => expect(within(card).queryByText(/Entonces Hedy retoma/)).toBeNull());
    expect(within(card).queryByRole("button", { name: "Cancelar espera" })).toBeNull();
    expect(within(card).queryByRole("button", { name: "Esperar" })).toBeNull();
  });

  it("after a restart past the reset it waits for the user to continue", async () => {
    const { api, user } = setup();
    const card = await openWaitingTask(user, /^Actividad: /, /Claude personal llegó a su límite/);
    act(() => api.simulateRestartAfterReset("s3"));
    expect(await within(card).findByText("Ya puedes continuar: el límite de Claude personal se renovó.")).toBeInTheDocument();
    expect(screen.queryByText("Revisa también el asunto del correo.", { selector: ".bubble" })).toBeNull();
    await user.click(within(card).getByRole("button", { name: "Continuar" }));
    expect(await screen.findByText("Revisa también el asunto del correo.", { selector: ".bubble" })).toBeInTheDocument();
  });

  it("without a known reset the user continues, with a continue turn", async () => {
    const api = MockApi.withSampleData();
    const { user } = setup("es", api);
    await user.click(await screen.findByRole("button", { name: /^hedy/ }, SLOW));
    await user.click(await screen.findByRole("radio", { name: "Pregunta" }));
    await user.type(screen.getByLabelText("Pídele algo a hedy"), "¿Dónde se crean las citas?");
    await user.click(screen.getByRole("button", { name: "Enviar" }));
    expect((await screen.findAllByText(/Terminó/)).length).toBeGreaterThan(0);
    act(() => api.simulateLimit("t1", "reached"));

    const card = await screen.findByRole("region", { name: /Claude personal llegó a su límite/ });
    await user.click(within(card).getByRole("button", { name: "Esperar" }));
    expect(await within(card).findByText(/Claude personal no dijo cuándo: continúa cuando quieras\./)).toBeInTheDocument();
    await user.click(within(card).getByRole("button", { name: "Continuar" }));
    expect(
      await screen.findByText("Tu límite se renovó. Continúa con la tarea donde la dejaste.", { selector: ".bubble" }),
    ).toBeInTheDocument();
  });

  it("works in English", async () => {
    const { user } = setup("en");
    const card = await openWaitingTask(user, /^Activity: /, /Claude personal hit its limit/);
    expect(
      within(card).getByText(/^Renews in 4\d min\. Then Hedy resumes the task with Claude personal\.$/),
    ).toBeInTheDocument();
    expect(within(card).getByRole("button", { name: "Stop waiting" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /^Activity/ }));
    expect(await screen.findByText(/^resumes in 4\d min$/)).toBeInTheDocument();
  });
});
