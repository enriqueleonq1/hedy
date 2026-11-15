import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";

vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

const SLOW = { timeout: 8000 };
const TASK = "Arregla el cálculo de impuestos en las facturas";

function setup(language: "es" | "en" = "es") {
  const api = MockApi.withSampleData().withSampleActivity();
  api.delayMs = 0;
  if (language === "en") void api.setSetting("ui.language", "en");
  render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
  return { api, user: userEvent.setup() };
}

/** Opens the sample task with changes to review (hedy, "s5") from the board. */
async function openReviewTask(user: ReturnType<typeof userEvent.setup>, board: RegExp) {
  await user.click(await screen.findByRole("button", { name: board }, SLOW));
  await user.click(await screen.findByRole("button", { name: new RegExp(TASK) }, SLOW));
  return screen.findByRole("list", { name: /Archivos cambiados|Changed files/ }, SLOW);
}

describe("Review before accepting", () => {
  afterEach(() => {
    history.replaceState(null, "", "#");
    localStorage.clear();
  });

  it("shows what will be merged, warns about failed tests and uses the edited message", async () => {
    const { api, user } = setup();
    await openReviewTask(user, /^Actividad: /);
    await user.click(screen.getByRole("button", { name: "Aceptar cambios" }));

    const sheet = await screen.findByRole("dialog", { name: "Revisar antes de integrar" });
    expect(
      await within(sheet).findByText("Se integra en main desde hedy/arregla-el-calculo-de-impuestos-s5."),
    ).toBeInTheDocument();
    expect(within(sheet).getByText("Archivos: 3")).toBeInTheDocument();
    expect(within(sheet).getByText("9 líneas añadidas y 3 eliminadas")).toBeInTheDocument();
    expect(
      within(sheet).getByText(/Los últimos tests fallaron: pytest tests\/test_facturas\.py\. Puedes integrar igual\./),
    ).toBeInTheDocument();
    const message = within(sheet).getByLabelText("Mensaje del commit");
    expect(message).toHaveValue(`${TASK}\n\nEl IVA se redondea por línea y no al total.`);

    await user.clear(message);
    await user.type(message, "fix: IVA por línea");
    await user.click(within(sheet).getByRole("button", { name: "Aceptar e integrar" }));

    expect(await screen.findByText("Cambios integrados en main.")).toBeInTheDocument();
    expect(api.commitMessages.get("s5")).toBe("fix: IVA por línea");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(await screen.findByText(/ya se aceptó/)).toBeInTheDocument();
  });

  it("cancels without merging", async () => {
    const { api, user } = setup();
    await openReviewTask(user, /^Actividad: /);
    await user.click(screen.getByRole("button", { name: "Aceptar cambios" }));
    const sheet = await screen.findByRole("dialog", { name: "Revisar antes de integrar" });
    await user.click(within(sheet).getByRole("button", { name: "Cancelar" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(api.commitMessages.has("s5")).toBe(false);
    expect(screen.getByRole("list", { name: "Archivos cambiados" })).toBeInTheDocument();
  });

  it("undoes the merge from the toast and the task comes back to review", async () => {
    const { user } = setup();
    await openReviewTask(user, /^Actividad: /);
    await user.click(screen.getByRole("button", { name: "Aceptar cambios" }));
    const sheet = await screen.findByRole("dialog", { name: "Revisar antes de integrar" });
    await user.click(await within(sheet).findByRole("button", { name: "Aceptar e integrar" }));

    const toast = (await screen.findByText("Cambios integrados en main.")).closest(".toast") as HTMLElement;
    await user.click(within(toast).getByRole("button", { name: "Deshacer" }));
    expect(await screen.findByText(/Integración deshecha/)).toBeInTheDocument();
    const files = await screen.findByRole("list", { name: "Archivos cambiados" });
    expect(within(files).getByRole("button", { name: /^facturas\/impuestos\.py/ })).toBeInTheDocument();
  });

  it("explains why an undo is not safe, from the accepted task", async () => {
    const { api, user } = setup();
    await openReviewTask(user, /^Actividad: /);
    await user.click(screen.getByRole("button", { name: "Aceptar cambios" }));
    const sheet = await screen.findByRole("dialog", { name: "Revisar antes de integrar" });
    await user.click(await within(sheet).findByRole("button", { name: "Aceptar e integrar" }));
    await screen.findByText(/ya se aceptó/);

    api.undoRefusal = "No se puede deshacer sin riesgo: ya hay commits nuevos después de la integración.";
    expect(screen.getByText(/Solo es posible si nada más cambió en main/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Deshacer la integración" }));
    expect(await screen.findByText(/ya hay commits nuevos después de la integración/)).toBeInTheDocument();
    expect(screen.getByText(/ya se aceptó/)).toBeInTheDocument();
  });

  it("moves through files with j / k and counts the ones marked reviewed", async () => {
    const { user } = setup();
    const files = await openReviewTask(user, /^Actividad: /);
    expect(screen.getByText("0 de 3 revisados")).toBeInTheDocument();

    const first = within(files).getByRole("button", { name: /^facturas\/impuestos\.py/ });
    expect(first).toHaveAttribute("aria-current", "true");
    first.focus();
    await user.keyboard("j");
    const second = within(files).getByRole("button", { name: /^facturas\/totales\.py/ });
    expect(second).toHaveAttribute("aria-current", "true");
    expect(second).toHaveFocus();
    expect(screen.getByRole("table", { name: "Cambios en facturas/totales.py" })).toBeInTheDocument();
    await user.keyboard("j");
    await user.keyboard("j");
    expect(within(files).getByRole("button", { name: /^tests\/test_facturas\.py/ })).toHaveAttribute("aria-current", "true");
    await user.keyboard("k");
    expect(second).toHaveAttribute("aria-current", "true");

    await user.click(within(files).getByRole("checkbox", { name: "Marcar facturas/impuestos.py como revisado" }));
    await user.click(screen.getByRole("checkbox", { name: "Revisado" }));
    expect(screen.getByText("2 de 3 revisados")).toBeInTheDocument();
    expect(within(files).getByRole("button", { name: /^facturas\/impuestos\.py.*, revisado/ })).toBeInTheDocument();

    // Typing in a field never moves the selection.
    await user.click(screen.getByLabelText("Responde o pide algo más"));
    await user.keyboard("j");
    expect(second).toHaveAttribute("aria-current", "true");
  });

  it("works in English", async () => {
    const { user } = setup("en");
    await openReviewTask(user, /^Activity: /);
    expect(screen.getByText("0 of 3 reviewed")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Accept changes" }));
    const sheet = await screen.findByRole("dialog", { name: "Review before merging" });
    expect(await within(sheet).findByText("Files: 3")).toBeInTheDocument();
    expect(within(sheet).getByText(/The last tests failed: .*You can still merge\./)).toBeInTheDocument();
    expect(within(sheet).getByLabelText("Commit message")).toBeInTheDocument();
    await user.click(within(sheet).getByRole("button", { name: "Accept and merge" }));
    const toast = (await screen.findByText("Changes merged into main.")).closest(".toast") as HTMLElement;
    await user.click(within(toast).getByRole("button", { name: "Undo" }));
    await waitFor(() => expect(screen.getByText(/Merge undone/)).toBeInTheDocument());
  });
});
