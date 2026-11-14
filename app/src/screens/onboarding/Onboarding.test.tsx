import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";

vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

function setup() {
  const api = new MockApi({ onboarded: false, language: "es" });
  render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
  return { api, user: userEvent.setup() };
}

describe("Onboarding", () => {
  it("walks through the guide and connects the accounts it finds", async () => {
    const { api, user } = setup();
    expect(await screen.findByRole("heading", { name: "Hola, soy Hedy." })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Empezar" }));

    expect(await screen.findByRole("heading", { name: /Varias IAs/ })).toBeInTheDocument();
    expect(screen.getByText("Mi tienda online")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Continuar" }));

    expect(await screen.findByRole("heading", { name: "Conecta tus IAs" })).toBeInTheDocument();
    const claude = (await screen.findByText("Claude Code")).closest("li");
    if (!claude) throw new Error("no Claude row");
    expect(within(claude).getByText("Ya tienes la sesión abierta en este equipo")).toBeInTheDocument();
    await user.click(within(claude).getByRole("button", { name: "Usar esta sesión" }));
    expect(await within(claude).findByText(/Conectada/)).toBeInTheDocument();
    const added = (await api.overview()).accounts.find((a) => a.provider === "claude");
    expect(added?.defaultFolder).toBe(true);

    // Not signed in: the account is created and its official login opens.
    const codex = screen.getByText("ChatGPT · Codex").closest("li");
    if (!codex) throw new Error("no Codex row");
    await user.click(within(codex).getByRole("button", { name: "Iniciar sesión" }));
    expect(await screen.findByText(/Se abrió el login de ChatGPT · Codex/)).toBeInTheDocument();
    expect(screen.getByText("Cómo instalar")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Continuar" }));
    expect(await screen.findByRole("heading", { name: "Tú tienes el control" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Rechazar" }));
    expect(screen.getByText(/El archivo queda como estaba/)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Continuar" }));
    expect(await screen.findByRole("heading", { name: /Lo que decides una vez/ })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Aprobar" }));
    expect(screen.getByText(/Guardada en la memoria/)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Continuar" }));
    expect(await screen.findByRole("heading", { name: "Todo listo" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Explorar Hedy" }));
    expect(await screen.findByRole("button", { name: "Inicio" })).toBeInTheDocument();
    expect((await api.overview()).onboarded).toBe(true);
  });

  it("speaks English when chosen, and can be skipped", async () => {
    const { api, user } = setup();
    await screen.findByRole("heading", { name: "Hola, soy Hedy." });
    await user.click(screen.getByRole("radio", { name: "English" }));
    expect(await screen.findByRole("heading", { name: "Hi, I'm Hedy." })).toBeInTheDocument();
    expect((await api.overview()).language).toBe("en");

    await user.click(screen.getByRole("button", { name: "Get started" }));
    await user.click(await screen.findByRole("button", { name: "Skip" }));
    expect(await screen.findByRole("heading", { name: "All set" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Explore Hedy" }));
    // The rest of the app follows the chosen language.
    expect(await screen.findByRole("button", { name: "Home" })).toBeInTheDocument();
  });

  it("can be seen again from Settings", async () => {
    const api = new MockApi();
    history.replaceState(null, "", "#settings");
    render(<App api={api} queryClient={new QueryClient()} />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Ver la guía de bienvenida" }));
    expect(await screen.findByRole("heading", { name: "Hola, soy Hedy." })).toBeInTheDocument();
  });
});
