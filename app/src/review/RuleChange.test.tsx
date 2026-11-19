import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { ApiProvider } from "../api/context";
import { MockApi } from "../api/MockApi";
import type { MemorySource, TeamNote } from "../api/types";
import { ToastProvider } from "../components/ui/Toast";
import { RuleChangeDialog } from "./RuleChange";

const TEAM_RULE: MemorySource = {
  text: "Los modales van sobre el color de panel",
  category: "rule",
  topic: null,
  layer: "team",
  author: "luis",
  team: "Plataforma",
  project: null,
  at: 0,
  note: { kind: "team", team: "t-1", uid: "n-9" },
};

function setup(published: boolean, now: Partial<TeamNote> | null = {}) {
  const api = MockApi.withSampleData();
  const edit = vi.spyOn(api, "teamEditNote").mockResolvedValue({ published, uid: "s-1" });
  const note = { uid: "n-9", text: TEAM_RULE.text, canEdit: published, ...now } as TeamNote;
  vi.spyOn(api, "teamNotes").mockResolvedValue(now === null ? [] : [note]);
  const done = vi.fn();
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiProvider api={api}>
        <ToastProvider>
          <RuleChangeDialog rule={TEAM_RULE} project="/code/tienda" onClose={() => {}} onDone={done} />
        </ToastProvider>
      </ApiProvider>
    </QueryClientProvider>,
  );
  return { edit, done, user: userEvent.setup() };
}

describe("changing a team's rule", () => {
  it("proposes the change to whoever may make it, never changing it behind them", async () => {
    const { edit, done, user } = setup(false);
    expect(screen.getByText(/Es del equipo «Plataforma»: si no la escribiste tú/)).toBeInTheDocument();
    const field = screen.getByLabelText("Pasa a decir");
    await user.clear(field);
    await user.type(field, "Los modales van sobre el color de panel, salvo los de confirmación");
    const propose = screen.getByRole("button", { name: "Proponer el cambio" });
    await waitFor(() => expect(propose).toBeEnabled());
    await user.click(propose);
    expect(edit).toHaveBeenCalledWith("t-1", "n-9", "Los modales van sobre el color de panel, salvo los de confirmación");
    expect(await screen.findByText(/Enviado como sugerencia/)).toBeInTheDocument();
    // A suggestion changes nothing yet: nothing is settled.
    expect(done).not.toHaveBeenCalled();
  });

  it("proposes removing it: it holds meanwhile", async () => {
    const { edit, user } = setup(false);
    await user.click(screen.getByRole("radio", { name: "Quitarla" }));
    expect(await screen.findByText(/mientras tanto sigue vigente/)).toBeInTheDocument();
    const propose = screen.getByRole("button", { name: "Proponer el cambio" });
    await waitFor(() => expect(propose).toBeEnabled());
    await user.click(propose);
    expect(edit).toHaveBeenCalledWith("t-1", "n-9", null);
  });

  it("says plainly when the change counts for the whole team at once", async () => {
    const { edit, user } = setup(true);
    expect(await screen.findByText(/puedes cambiarla: el cambio vale para todo el equipo/)).toBeInTheDocument();
    await user.click(screen.getByRole("radio", { name: "Quitarla" }));
    expect(screen.getByText(/Se quita para todo el equipo/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Quitarla para todo el equipo" }));
    expect(edit).toHaveBeenCalledWith("t-1", "n-9", null);
    expect(await screen.findByText("Regla quitada para todo el equipo.")).toBeInTheDocument();
  });

  it("refuses to change a rule that changed or went since the check, but takes an exception", async () => {
    const { edit, user } = setup(false, { text: "Otro texto que ocupó su lugar" });
    expect(await screen.findByRole("alert")).toHaveTextContent(/cambió o se quitó desde la revisión/);
    expect(screen.getByRole("button", { name: "Proponer el cambio" })).toBeDisabled();
    await user.click(screen.getByRole("radio", { name: "Quitarla" }));
    expect(screen.getByRole("button", { name: "Proponer el cambio" })).toBeDisabled();
    await user.click(screen.getByRole("radio", { name: "Solo una excepción para este proyecto" }));
    await user.type(screen.getByLabelText("La excepción"), "aquí no");
    expect(screen.getByRole("button", { name: "Añadir la excepción" })).toBeEnabled();
    expect(edit).not.toHaveBeenCalled();
  });
});
