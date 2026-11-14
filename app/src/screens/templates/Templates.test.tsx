import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";

// xterm needs a real canvas; the Terminal tab is not exercised here.
vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

function setup() {
  const api = MockApi.withSampleData().withSampleTemplates();
  api.delayMs = 0;
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<App api={api} queryClient={queryClient} />);
  return { api, user: userEvent.setup() };
}

describe("Prompt templates", () => {
  it("picks a template with / and fills its placeholders", async () => {
    const { user } = setup();
    await user.click(await screen.findByRole("button", { name: /^hedy/ }));
    const composer = await screen.findByLabelText("Pídele algo a hedy");

    await user.type(composer, "/");
    const list = await screen.findByRole("listbox", { name: "Plantillas" });
    // The project's own template first, then the global ones.
    const options = within(list).getAllByRole("option");
    expect(options[0]).toHaveTextContent("Revisar accesibilidad");
    expect(options[0]).toHaveAttribute("aria-selected", "true");
    expect(composer).toHaveAttribute("aria-activedescendant", options[0]!.id);
    await user.keyboard("{ArrowDown}");
    expect(composer).toHaveAttribute("aria-activedescendant", options[1]!.id);

    await user.type(composer, "pru");
    const filtered = within(screen.getByRole("listbox", { name: "Plantillas" })).getAllByRole("option");
    expect(filtered[0]).toHaveTextContent("Escribir pruebas");
    await user.keyboard("{Enter}");

    const fill = await screen.findByRole("group", { name: "Completa «Escribir pruebas»" });
    const field = within(fill).getByLabelText("{archivo}");
    expect(field).toHaveFocus();
    await user.type(field, "citas/servicio.py{Enter}");
    await waitFor(() =>
      expect(composer).toHaveValue(
        "Escribe pruebas para citas/servicio.py. Cubre los casos normales y los bordes, y ejecútalas al terminar.",
      ),
    );
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
  });

  it("closes the picker with Escape and says when nothing matches", async () => {
    const { user } = setup();
    await user.click(await screen.findByRole("button", { name: /^hedy/ }));
    const composer = await screen.findByLabelText("Pídele algo a hedy");
    await user.type(composer, "/zzz");
    expect(await screen.findByText("Ninguna plantilla se llama así.")).toHaveAttribute("role", "status");
    await user.keyboard("{Escape}");
    expect(screen.queryByText("Ninguna plantilla se llama así.")).not.toBeInTheDocument();
    expect(composer).toHaveValue("/zzz");
  });

  it("saves the composer text as a template", async () => {
    const { user } = setup();
    await user.click(await screen.findByRole("button", { name: /^hedy/ }));
    const composer = await screen.findByLabelText("Pídele algo a hedy");
    await user.type(composer, "Revisa los textos de {{pantalla}");
    await user.click(screen.getByRole("button", { name: "Guardar como plantilla" }));

    const dialog = screen.getByRole("dialog", { name: "Guardar el mensaje como plantilla" });
    expect(within(dialog).getByText("Pide: {pantalla}")).toBeInTheDocument();
    expect(within(dialog).getByRole("radio", { name: "Este proyecto" })).toHaveAttribute("aria-checked", "true");
    await user.type(within(dialog).getByLabelText("Nombre"), "Revisar textos");
    await user.click(within(dialog).getByRole("button", { name: "Guardar plantilla" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());

    await user.clear(composer);
    await user.type(composer, "/revisar t");
    const list = await screen.findByRole("listbox", { name: "Plantillas" });
    expect(within(list).getAllByRole("option")[0]).toHaveTextContent("Revisar textos");
  });

  it("manages templates in Settings: create, delete and restore examples", async () => {
    window.history.replaceState(null, "", "#settings");
    const { user } = setup();
    const section = await screen.findByRole("region", { name: "Plantillas" });
    expect(await within(section).findByText("Explicar código")).toBeInTheDocument();
    expect(within(section).getAllByText("Ejemplo")).toHaveLength(5);
    expect(within(section).getByText("Solo hedy")).toBeInTheDocument();

    await user.click(within(section).getByRole("button", { name: "Borrar Explicar código" }));
    await user.click(within(section).getByRole("button", { name: "Borrar Explicar código" }));
    await waitFor(() => expect(within(section).queryByText("Explicar código")).not.toBeInTheDocument());
    await user.click(within(section).getByRole("button", { name: "Restaurar ejemplos" }));
    expect(await within(section).findByText("Explicar código")).toBeInTheDocument();

    await user.click(within(section).getByRole("button", { name: "Nueva plantilla" }));
    const dialog = screen.getByRole("dialog", { name: "Nueva plantilla" });
    await user.type(within(dialog).getByLabelText("Nombre"), "Migrar datos");
    await user.type(within(dialog).getByLabelText("Texto"), "Escribe la migración de {{tabla}");
    await user.selectOptions(within(dialog).getByLabelText("Para"), "Solo homming");
    await user.click(within(dialog).getByRole("button", { name: "Guardar plantilla" }));
    expect(await within(section).findByText("Migrar datos")).toBeInTheDocument();
    expect(within(section).getByText("Solo homming")).toBeInTheDocument();

    await user.click(within(section).getByRole("button", { name: "Editar Migrar datos" }));
    const edit = screen.getByRole("dialog", { name: "Editar plantilla" });
    expect(within(edit).queryByLabelText("Para")).not.toBeInTheDocument();
    window.history.replaceState(null, "", "#");
  });
});
