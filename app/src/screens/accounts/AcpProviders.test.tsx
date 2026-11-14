import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";

vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

async function addAccount(user: ReturnType<typeof userEvent.setup>, name: string, kind: string) {
  await user.click(screen.getByRole("button", { name: "+ Agregar cuenta" }));
  const dialog = screen.getByRole("dialog", { name: "Agregar cuenta" });
  await user.type(within(dialog).getByLabelText("Nombre"), name);
  await user.click(within(dialog).getByRole("radio", { name: new RegExp(`^${kind}`) }));
  return dialog;
}

describe("ACP providers", () => {
  afterEach(() => history.replaceState(null, "", "#"));

  it("adds Kimi Code and one Antigravity account, and says how to log in", async () => {
    const api = new MockApi();
    history.replaceState(null, "", "#accounts");
    render(<App api={api} queryClient={new QueryClient()} />);
    const user = userEvent.setup();
    await screen.findByRole("heading", { name: /Tus cuentas/ });

    let dialog = await addAccount(user, "Kimi Code", "Kimi Code");
    await user.click(within(dialog).getByRole("button", { name: "Crear e iniciar sesión" }));
    const kimi = await screen.findByRole("article", { name: "Kimi Code" });
    expect(within(kimi).getByText(/escribe \/login/)).toBeInTheDocument();

    dialog = await addAccount(user, "Antigravity", "Antigravity");
    expect(within(dialog).getByText(/solo admite una cuenta por equipo/)).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Crear e iniciar sesión" }));
    await screen.findByRole("article", { name: "Antigravity" });
    expect((await api.overview()).accounts.find((a) => a.id === "antigravity")?.defaultFolder).toBe(true);

    dialog = await addAccount(user, "Otra", "Antigravity");
    expect(within(dialog).getByText(/Ya tienes una cuenta de Antigravity/)).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Crear e iniciar sesión" })).toBeDisabled();
  });
});
