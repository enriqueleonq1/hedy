import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";

// xterm needs a real canvas; the Terminal tab is not exercised here.
vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

function setup(api = MockApi.withSampleData().withSampleActivity()) {
  api.delayMs = 0;
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<App api={api} queryClient={queryClient} />);
  return { api, user: userEvent.setup() };
}

describe("Files a question wrote", () => {
  afterEach(() => window.history.replaceState(null, "", "#"));

  it("are listed under the answer, opened and saved somewhere else", async () => {
    window.history.replaceState(null, "", "#c=c1");
    const { api, user } = setup();
    const open = vi.spyOn(api, "openOutput");
    const save = vi.spyOn(api, "saveOutput");
    const card = await screen.findByRole("region", { name: "Archivos que escribió" });
    expect(card).toHaveTextContent("Están fuera de tu proyecto");
    expect(within(card).getByText("oauth-vs-oidc.md")).toBeInTheDocument();
    expect(within(card).getByText("4,8 KB")).toBeInTheDocument();

    // Something that could run is only saved, never opened from Hedy.
    expect(within(card).getByText("instalar.sh")).toBeInTheDocument();
    expect(within(card).queryByRole("button", { name: "Abrir instalar.sh" })).not.toBeInTheDocument();
    expect(within(card).getByRole("button", { name: "Guardar instalar.sh en…" })).toBeInTheDocument();
    await user.click(within(card).getByRole("button", { name: "Abrir diagrama/index.html" }));
    expect(open).toHaveBeenCalledWith("c1", "diagrama/index.html");
    await user.click(within(card).getByRole("button", { name: "Guardar oauth-vs-oidc.md en…" }));
    expect(save).toHaveBeenCalledWith("c1", "oauth-vs-oidc.md", "/code/nuevo-proyecto");
    expect(await screen.findByText("Guardado en /code/nuevo-proyecto/oauth-vs-oidc.md")).toBeInTheDocument();
  });

  it("show nothing for a task that wrote nothing", async () => {
    window.history.replaceState(null, "", "#c=c1");
    const api = MockApi.withSampleData().withSampleActivity();
    vi.spyOn(api, "taskOutputs").mockResolvedValue([]);
    setup(api);
    await screen.findByText(/OAuth 2.0 autoriza/);
    expect(screen.queryByRole("region", { name: "Archivos que escribió" })).not.toBeInTheDocument();
  });
});
