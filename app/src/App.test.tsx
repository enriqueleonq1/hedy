import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "./App";
import { MockApi } from "./api/MockApi";

function renderApp(api = MockApi.withSampleData()) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<App api={api} queryClient={queryClient} />);
  return { api, user: userEvent.setup() };
}

describe("Welcome", () => {
  it("greets the user and lists projects with their accounts", async () => {
    renderApp();
    expect(await screen.findByRole("heading", { name: /¡Hola, Enrique!/ })).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "Hedy te saluda" })).toHaveAttribute("data-state", "done");
    const homming = screen.getByRole("heading", { name: "homming" }).closest("article")!;
    expect(within(homming).getByText("Claude trabajo")).toBeInTheDocument();
    expect(within(homming).getByText("Protegido")).toBeInTheDocument();
    expect(screen.getByText(/Aún no hay datos/)).toBeInTheDocument();
  });

  it("invites to add an account when there are none", async () => {
    renderApp(new MockApi());
    expect(await screen.findByRole("button", { name: "Agregar mi primera cuenta" })).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "Hedy te saluda" })).toHaveAttribute("data-state", "sleeping");
  });
});

describe("Accounts", () => {
  async function openAccounts() {
    const ctx = renderApp();
    await ctx.user.click(await screen.findByRole("button", { name: "Cuentas y modelos" }));
    return ctx;
  }

  it("shows one card per account with its login state", async () => {
    await openAccounts();
    const card = screen.getByRole("article", { name: "Claude trabajo" });
    expect(within(card).getByText("Anthropic · Team")).toBeInTheDocument();
    expect(within(card).getByText(/Conectada · team/)).toBeInTheDocument();
    expect(within(card).getByText(/carpeta por defecto de la CLI/)).toBeInTheDocument();
    expect(within(card).getByRole("listitem")).toHaveTextContent("homming");
    const personal = screen.getByRole("article", { name: "Claude personal" });
    expect(within(personal).getByRole("meter", { name: "Límite del plan: 34%" })).toBeInTheDocument();
    expect(within(personal).getByText("182k tokens")).toBeInTheDocument();
    expect(within(card).getByText("sin datos")).toBeInTheDocument();
    const codex = screen.getByRole("article", { name: "ChatGPT" });
    expect(within(codex).getByRole("button", { name: "Iniciar sesión" })).toBeInTheDocument();
  });

  it("adds an API key account and stores its key", async () => {
    const { user } = await openAccounts();
    await user.click(screen.getByRole("button", { name: "+ Agregar cuenta" }));
    const dialog = screen.getByRole("dialog", { name: "Agregar cuenta" });
    await user.type(within(dialog).getByLabelText("Nombre"), "Gemini personal");
    await user.click(within(dialog).getByRole("radio", { name: /API key de Google/ }));
    await user.click(within(dialog).getByRole("button", { name: "Crear y guardar key" }));

    const keyDialog = await screen.findByRole("dialog", { name: "API key de Gemini personal" });
    await user.type(within(keyDialog).getByLabelText("API key"), "AIza-test");
    await user.click(within(keyDialog).getByRole("button", { name: "Guardar en el llavero" }));

    const card = await screen.findByRole("article", { name: "Gemini personal" });
    await waitFor(() => expect(within(card).getByText(/Key guardada/)).toBeInTheDocument());
  });

  it("warns that Gemini CLI does not accept personal accounts", async () => {
    const { user } = await openAccounts();
    await user.click(screen.getByRole("button", { name: "+ Agregar cuenta" }));
    await user.click(screen.getByRole("radio", { name: /Gemini CLI/ }));
    expect(screen.getByText(/ya no acepta cuentas personales/)).toBeInTheDocument();
  });

  it("does not offer a protected project's account to other projects", async () => {
    const api = MockApi.withSampleData();
    await api.useAccount("/code/hobby", "claude-personal");
    const { user } = renderApp(api);
    await user.click(await screen.findByRole("button", { name: "Cuentas y modelos" }));

    const select = await screen.findByRole("combobox", { name: "Cuenta de hobby" });
    const options = within(select).getAllByRole("option").map((o) => o.textContent);
    expect(options).not.toContain("trabajo");
    expect(options).toContain("personal");
    expect(screen.getByRole("combobox", { name: "Cuenta de homming" })).toBeDisabled();
  });
});

describe("Models per phase", () => {
  it("assigns phases to other providers and summarises them", async () => {
    const { user } = renderApp();
    await user.click(await screen.findByRole("button", { name: "Cuentas y modelos" }));
    const edit = screen.getByRole("button", { name: "Modelos por fase de hedy" });
    expect(edit).toHaveTextContent(/^Claude$/);
    await user.click(edit);

    const dialog = screen.getByRole("dialog", { name: "Modelos por fase · hedy" });
    await user.selectOptions(within(dialog).getByLabelText("Cuenta para Implementar"), "chatgpt");
    await user.selectOptions(within(dialog).getByLabelText("Cuenta para Escribir tests"), "kimi");
    await user.type(within(dialog).getByLabelText("Modelo para Escribir tests"), "kimi-k3");
    await user.click(within(dialog).getByRole("button", { name: "Guardar" }));

    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Modelos por fase de hedy" })).toHaveTextContent(
        "Claude planifica · Codex implementa · Kimi tests",
      ),
    );
  });
});

describe("Settings", () => {
  it("changes the theme", async () => {
    const { user } = renderApp();
    await user.click(await screen.findByRole("button", { name: "Ajustes" }));
    await user.click(screen.getByRole("radio", { name: "Claro" }));
    await waitFor(() => expect(document.documentElement.dataset.theme).toBe("light"));
    expect(screen.getByRole("radio", { name: "Claro" })).toHaveAttribute("aria-checked", "true");
  });

  it("previews the four mascot states", async () => {
    const { user } = renderApp();
    await user.click(await screen.findByRole("button", { name: "Ajustes" }));
    const gallery = screen.getByLabelText("Estados de la mascota");
    for (const name of ["Durmiendo", "Pensando", "Trabajando", "¡Listo!"]) {
      expect(within(gallery).getByRole("img", { name })).toBeInTheDocument();
    }
  });
});
