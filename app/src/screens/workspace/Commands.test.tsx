import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";

// xterm needs a real canvas; the Terminal tab is not exercised here.
vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

function setup(api = MockApi.withSampleData()) {
  api.delayMs = 0;
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<App api={api} queryClient={queryClient} />);
  return { api, user: userEvent.setup() };
}

const UNKNOWN = "Los comandos de esta cuenta aparecen después de su primera respuesta";
const READ_ONLY =
  "En una pregunta rápida los comandos no pueden escribir archivos. ¿Necesita escribir? Llévalo a un proyecto.";

describe("The account's slash commands in the composer", () => {
  afterEach(() => window.history.replaceState(null, "", "#"));

  it("lists them after the templates and inserts the chosen one, sent as typed", async () => {
    const { api, user } = setup();
    const start = vi.spyOn(api, "startTask");
    await user.click(await screen.findByRole("button", { name: /^hedy/ }));
    const composer = await screen.findByLabelText("Pídele algo a hedy");

    await user.type(composer, "/");
    const list = await screen.findByRole("listbox", { name: "Plantillas y comandos" });
    expect(within(list).getByRole("group", { name: "Plantillas" })).toBeInTheDocument();
    const commands = within(list).getByRole("group", { name: "Comandos de Claude personal" });
    const design = within(commands).getByRole("option", { name: /\/design/ });
    expect(design).toHaveTextContent("<screen>");
    expect(design).toHaveTextContent("plugin");
    expect(design).toHaveTextContent("Design a screen or a component");
    expect(within(commands).getByRole("option", { name: /\/brand-voice/ })).toHaveTextContent("habilidad");

    // The same query filters both groups, fuzzy for the commands.
    await user.type(composer, "dsg");
    const filtered = within(screen.getByRole("listbox", { name: "Plantillas y comandos" })).getAllByRole("option");
    expect(filtered).toHaveLength(1);
    expect(filtered[0]).toHaveTextContent("/design");
    expect(composer).toHaveAttribute("aria-activedescendant", filtered[0]!.id);
    await user.keyboard("{Enter}");

    expect(composer).toHaveValue("/design ");
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    expect(screen.getByText("/design espera: <screen>")).toBeInTheDocument();
    await user.type(composer, "pantalla de login");
    await user.click(screen.getByRole("button", { name: "Enviar" }));
    await waitFor(() => expect(start).toHaveBeenCalled());
    expect(start.mock.calls[0]![0].prompt).toBe("/design pantalla de login");
  });

  it("picks a command with the mouse", async () => {
    const { user } = setup();
    await user.click(await screen.findByRole("button", { name: /^hedy/ }));
    const composer = await screen.findByLabelText("Pídele algo a hedy");
    await user.type(composer, "/code");
    const option = await screen.findByRole("option", { name: /\/code-review/ });
    await user.click(option);
    expect(composer).toHaveValue("/code-review ");
    expect(composer).toHaveFocus();
  });

  it("says when the account's commands are not known yet and still sends any command", async () => {
    const { api, user } = setup();
    const start = vi.spyOn(api, "startTask");
    await user.click(await screen.findByRole("button", { name: /^tienda/ }));
    const composer = await screen.findByLabelText("Pídele algo a tienda");

    await user.type(composer, "/");
    expect(await screen.findByText(UNKNOWN)).toHaveAttribute("role", "status");
    expect(screen.getByText("Comandos de Claude trabajo", { selector: "span" })).toBeInTheDocument();

    await user.type(composer, "revisar-todo ya");
    await user.click(screen.getByRole("button", { name: "Enviar" }));
    await waitFor(() => expect(start).toHaveBeenCalled());
    expect(start.mock.calls[0]![0].prompt).toBe("/revisar-todo ya");
  });

  it("offers no commands for an account that does not tell them", async () => {
    window.history.replaceState(null, "", "#c");
    const { user } = setup();
    const composer = await screen.findByRole("textbox", { name: "¿Qué quieres saber?" });
    const account = await screen.findByRole("combobox", { name: "Cuenta de la pregunta" });
    await user.click(account);
    await user.click(screen.getByRole("option", { name: "Kimi" }));
    await user.type(composer, "/");
    expect(await screen.findByRole("listbox", { name: "Plantillas" })).toBeInTheDocument();
    expect(screen.queryByText(UNKNOWN)).not.toBeInTheDocument();
    expect(screen.queryByRole("group", { name: /^Comandos de/ })).not.toBeInTheDocument();
  });

  it("notes in a quick question that the user's commands cannot write files", async () => {
    const api = MockApi.withSampleData();
    const chat = await api.startConversation({ prompt: "Hola", mode: "ask", account: "claude-personal", model: null });
    window.history.replaceState(null, "", `#c=${chat.id}`);
    const { user } = setup(api);
    // The conversation's own composer (not the one shown while the list loads).
    await screen.findByRole("heading", { name: "Hola" });
    await waitFor(() => expect(document.getElementById("composer-input")).not.toBeNull());
    const composer = document.getElementById("composer-input") as HTMLTextAreaElement;

    await user.type(composer, "/compact ");
    // A built-in command: nothing to say.
    await waitFor(() => expect(composer).toHaveValue("/compact "));
    expect(screen.queryByText(READ_ONLY)).not.toBeInTheDocument();

    await user.clear(composer);
    await user.type(composer, "/brand-voice hola");
    const note = await screen.findByRole("note");
    expect(note).toHaveTextContent(READ_ONLY);
    expect(within(note).getByRole("button", { name: "Llevar a un proyecto…" })).toBeInTheDocument();
  });
});
