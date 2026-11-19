import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { ApiProvider } from "../api/context";
import { MockApi } from "../api/MockApi";
import type { MemorySource } from "../api/types";
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

function setup(published: boolean) {
  const api = MockApi.withSampleData();
  const edit = vi.spyOn(api, "teamEditNote").mockResolvedValue({ published, uid: "s-1" });
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
    await user.click(screen.getByRole("button", { name: "Proponer el cambio" }));
    expect(edit).toHaveBeenCalledWith("t-1", "n-9", "Los modales van sobre el color de panel, salvo los de confirmación");
    expect(await screen.findByText(/Enviado como sugerencia/)).toBeInTheDocument();
    expect(done).toHaveBeenCalled();
  });

  it("proposes removing it: it holds meanwhile", async () => {
    const { edit, user } = setup(false);
    await user.click(screen.getByRole("radio", { name: "Quitarla" }));
    expect(screen.getByText(/mientras tanto sigue vigente/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Proponer el cambio" }));
    expect(edit).toHaveBeenCalledWith("t-1", "n-9", null);
  });
});
