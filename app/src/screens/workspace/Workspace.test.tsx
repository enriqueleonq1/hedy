import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";

// xterm needs a real canvas; the Terminal tab is not exercised here.
vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

function setup() {
  const api = MockApi.withSampleData();
  api.delayMs = 0;
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<App api={api} queryClient={queryClient} />);
  return { api, user: userEvent.setup() };
}

async function openProject(user: ReturnType<typeof userEvent.setup>, name: RegExp) {
  await user.click(await screen.findByRole("button", { name }));
}

describe("Workspace", () => {
  it("runs a task end to end: plan, permission, tests, diff and accept", async () => {
    const { user } = setup();
    await openProject(user, /^hedy/);
    expect(await screen.findByText(/¿Qué hacemos en hedy\?/)).toBeInTheDocument();
    expect(screen.getByRole("radiogroup", { name: "Modo" })).toBeInTheDocument();

    await user.type(screen.getByLabelText("Pídele algo a hedy"), "Agrega un recordatorio 24 horas antes");
    await user.click(screen.getByRole("button", { name: "Enviar" }));

    // Plan first: nothing changes until it is approved.
    const plan = await screen.findByRole("region", { name: "Plan de Claude Opus" });
    expect(within(plan).getByText(/Buscar dónde se crean las citas/)).toBeInTheDocument();
    await user.click(within(plan).getByRole("button", { name: "Aprobar plan" }));

    const card = await screen.findByRole("region", { name: "Claude quiere correr un comando" });
    expect(within(card).getByText("pytest tests/test_recordatorios.py")).toBeInTheDocument();
    expect(screen.getByText("Claude editó citas/servicio.py")).toBeInTheDocument();
    await user.click(within(card).getByRole("button", { name: "Permitir en este chat" }));

    expect(await screen.findByText(/Pasan los tests/)).toBeInTheDocument();
    expect(await screen.findByText(/Terminó/)).toBeInTheDocument();
    expect(screen.getByText(/Permitido en este chat/)).toBeInTheDocument();
    const steps = screen.getByRole("region", { name: "Pasos" });
    expect(within(steps).getAllByText(/\(hecho\)/)).toHaveLength(3);

    const files = await screen.findByRole("list", { name: "Archivos cambiados" });
    expect(within(files).getByText("citas/servicio.py")).toBeInTheDocument();
    expect(screen.getByRole("table", { name: "Cambios en citas/servicio.py" })).toHaveTextContent(
      "programar_recordatorio(cita, horas=24)",
    );

    await user.click(screen.getByRole("button", { name: "Aceptar cambios" }));
    expect(await screen.findByText("Cambios unidos a main.")).toBeInTheDocument();
    expect(await screen.findByText(/ya se aceptó/)).toBeInTheDocument();
  });

  it("sends the plan back with feedback", async () => {
    const { user } = setup();
    await openProject(user, /^hedy/);
    await user.type(await screen.findByLabelText("Pídele algo a hedy"), "Recordatorios");
    await user.click(screen.getByRole("button", { name: "Enviar" }));

    const plan = await screen.findByRole("region", { name: "Plan de Claude Opus" });
    await user.click(within(plan).getByRole("button", { name: "Pedir cambios" }));
    await user.type(within(plan).getByLabelText("¿Qué cambiarías?"), "usa Celery");
    await user.click(within(plan).getByRole("button", { name: "Enviar cambios" }));

    await waitFor(() => expect(screen.getAllByRole("region", { name: "Plan de Claude Opus" })).toHaveLength(2));
    expect(screen.getByText(/con cambios pedidos/)).toBeInTheDocument();
    expect(screen.getByText(/usa Celery/)).toBeInTheDocument();
  });

  it("asks without a worktree in Pregunta mode", async () => {
    const { user } = setup();
    await openProject(user, /^hedy/);
    await user.click(await screen.findByRole("radio", { name: "Pregunta" }));
    expect(screen.queryByLabelText("Planificar primero")).not.toBeInTheDocument();
    await user.type(screen.getByLabelText("Pídele algo a hedy"), "¿Dónde se crean las citas?");
    await user.click(screen.getByRole("button", { name: "Enviar" }));
    expect(await screen.findByText(/no cambian archivos/)).toBeInTheDocument();
    expect(await screen.findByText(/Terminó/)).toBeInTheDocument();
  });

  it("previews only local addresses", async () => {
    const { user } = setup();
    await openProject(user, /^hedy/);
    await user.click(await screen.findByRole("tab", { name: "Vista previa" }));
    const address = screen.getByLabelText("Dirección del servidor local");
    await user.clear(address);
    await user.type(address, "https://example.com");
    expect(screen.getByText(/Solo direcciones locales/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Abrir" })).toBeDisabled();
    await user.clear(address);
    await user.type(address, "http://localhost:5173");
    await user.click(screen.getByRole("button", { name: "Abrir" }));
    expect(screen.getByTitle("Vista previa")).toHaveAttribute("src", "http://localhost:5173");
  });

  it("keeps project tabs open and closes them", async () => {
    const { user } = setup();
    await openProject(user, /^hedy/);
    await openProject(user, /^homming/);
    const tabs = screen.getByRole("navigation", { name: "Proyectos abiertos" });
    expect(within(tabs).getByRole("button", { name: "homming" })).toHaveAttribute("aria-current", "page");
    await user.click(within(tabs).getByRole("button", { name: "Cerrar homming" }));
    expect(within(tabs).getByRole("button", { name: "hedy" })).toHaveAttribute("aria-current", "page");
  });
});
