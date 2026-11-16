import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../../App";
import { MockApi } from "../../../api/MockApi";

vi.mock("../../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

const QUESTION = "¿Qué diferencia hay entre OAuth y OpenID Connect?";
const CONTEXT = `Contexto: conversación «${QUESTION}»`;

/** The app on `hash`, over the sample data (the conversation `c1` answered already). */
function setup(hash = "#c=c1", api = MockApi.withSampleData().withSampleActivity()) {
  api.delayMs = 0;
  history.replaceState(null, "", hash);
  render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
  return { api, user: userEvent.setup() };
}

const conversationHeading = () => screen.findByRole("heading", { name: QUESTION, level: 1 });

/** The workspace header's own buttons (the answer's suggestion repeats some). */
const header = () => within(screen.getByRole("heading", { level: 1 }).closest("header") as HTMLElement);

/** The card's toggle: its name also says how much it carries. */
const CARD_TOGGLE = (name: string) => name.startsWith(CONTEXT) && name !== CONTEXT;

describe("Carrying a conversation into a project", () => {
  afterEach(() => {
    history.replaceState(null, "", "#");
    localStorage.clear();
  });

  it("opens a task in an existing project with the conversation as context, linked both ways", async () => {
    const { api, user } = setup();
    const start = vi.spyOn(api, "startTask");
    await conversationHeading();

    // A quiet icon in the header, named and explained.
    const button = header().getByRole("button", { name: "Llevar a un proyecto" });
    expect(button).toHaveAttribute("title", expect.stringContaining("Una tarea nueva con esta conversación como contexto"));
    await user.click(button);
    const dialog = screen.getByRole("dialog", { name: "Llevar a un proyecto" });
    expect(within(dialog).getByRole("combobox", { name: "Proyecto existente" })).toHaveTextContent("hedy");
    await user.click(within(dialog).getByRole("button", { name: "Abrir tarea" }));

    // A new task in hedy, the context in a card above the composer (not in the text).
    expect(await screen.findByRole("heading", { name: "hedy", level: 1 })).toBeInTheDocument();
    expect(window.location.hash).toBe("#p=%2Fcode%2Fhedy&new&from=c1");
    const card = await screen.findByRole("region", { name: CONTEXT });
    expect(await within(card).findByText("Preguntas 1/1 · 173 caracteres")).toBeInTheDocument();
    const input = screen.getByLabelText("Pídele algo a hedy");
    expect(input).toHaveValue("");
    expect(input).toHaveAttribute("placeholder", "¿Qué hacemos con esto en hedy?");

    await user.type(input, "Añade login con OpenID Connect");
    await user.click(screen.getByRole("button", { name: "Enviar" }));
    expect(start).toHaveBeenCalledWith(
      expect.objectContaining({
        project: "/code/hedy",
        prompt: "Añade login con OpenID Connect",
        carry: { conversation: "c1", turns: [0], summary: null },
      }),
    );

    // The task's first message is the instruction; the context folds above it.
    expect(await screen.findByRole("heading", { name: "Añade login con OpenID Connect", level: 1 })).toBeInTheDocument();
    const toggle = screen.getByRole("button", { name: CONTEXT });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    await user.click(toggle);
    expect(screen.getByLabelText("Contexto que recibió el agente")).toHaveTextContent(/--- Fin del contexto ---/);

    // "Viene de" goes back to the conversation, which lists where it continued.
    await user.click(screen.getByRole("button", { name: `Abrir la conversación «${QUESTION}»` }));
    await conversationHeading();
    const link = await screen.findByRole("button", { name: "Abrir la tarea «Añade login con OpenID Connect» de hedy" });
    expect(link).toHaveTextContent("hedy · Añade login con OpenID Connect");
    await user.click(link);
    expect(await screen.findByRole("heading", { name: "Añade login con OpenID Connect", level: 1 })).toBeInTheDocument();
  });

  it("creates a new project named after the conversation and carries it into its first task", async () => {
    const { api, user } = setup();
    const create = vi.spyOn(api, "newProject");
    await conversationHeading();

    // From the suggestion under the finished answer.
    const suggestion = await screen.findByRole("group", { name: "¿Y ahora?" });
    await user.click(within(suggestion).getByRole("button", { name: "Llevar a un proyecto" }));
    const dialog = screen.getByRole("dialog", { name: "Llevar a un proyecto" });
    expect(await within(dialog).findByText("Nombre sugerido: diferencia-oauth-openid-connect")).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Proyecto nuevo…" }));

    const created = screen.getByRole("dialog", { name: "Nuevo proyecto" });
    expect(within(created).getByRole("textbox", { name: "Nombre" })).toHaveValue("diferencia-oauth-openid-connect");
    const button = within(created).getByRole("button", { name: "Crear proyecto" });
    await waitFor(() => expect(button).toBeEnabled());
    await user.click(button);
    expect(create).toHaveBeenCalledWith(expect.objectContaining({ name: "diferencia-oauth-openid-connect" }));

    expect(await screen.findByRole("heading", { name: "diferencia-oauth-openid-connect", level: 1 })).toBeInTheDocument();
    expect(await screen.findByRole("region", { name: CONTEXT })).toBeInTheDocument();
  });

  it("offers a new project when there is none yet", async () => {
    const api = MockApi.withSampleData().withSampleActivity();
    for (const project of ["/code/hedy", "/code/tienda"]) await api.removeProject(project);
    const { user } = setup("#c=c1", api);
    await conversationHeading();
    await user.click(header().getByRole("button", { name: "Llevar a un proyecto" }));
    const dialog = screen.getByRole("dialog", { name: "Llevar a un proyecto" });
    expect(within(dialog).getByText(/Aún no tienes proyectos/)).toBeInTheDocument();
    expect(within(dialog).queryByRole("button", { name: "Abrir tarea" })).not.toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Proyecto nuevo…" }));
    expect(await screen.findByRole("dialog", { name: "Nuevo proyecto" })).toBeInTheDocument();
  });

  it("trims questions, and sends nothing when none is left", async () => {
    const { api, user } = setup("#p=%2Fcode%2Fhedy&new&from=c1");
    const start = vi.spyOn(api, "startTask");
    const card = await screen.findByRole("region", { name: CONTEXT });
    const toggle = within(card).getByRole("button", { name: CARD_TOGGLE });
    await user.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(within(card).getByText(QUESTION)).toBeInTheDocument();
    expect(within(card).getByText(/OAuth 2.0 autoriza/)).toBeInTheDocument();

    await user.click(within(card).getByRole("button", { name: "Quitar la pregunta 1" }));
    expect(within(card).getByText(/No queda nada que llevar/)).toBeInTheDocument();
    expect(within(card).getByText("Preguntas 0/1 · 0 caracteres")).toBeInTheDocument();
    await user.type(screen.getByLabelText("Pídele algo a hedy"), "Hazlo");
    expect(screen.getByRole("button", { name: "Enviar" })).toBeDisabled();
    await user.keyboard("{Enter}");
    expect(start).not.toHaveBeenCalled();

    await user.click(within(card).getByRole("button", { name: "Volver a incluir la pregunta 1" }));
    expect(screen.getByRole("button", { name: "Enviar" })).toBeEnabled();

    // Removing the whole context leaves a plain new task.
    await user.click(within(card).getByRole("button", { name: "Quitar el contexto" }));
    expect(screen.queryByRole("region", { name: CONTEXT })).not.toBeInTheDocument();
    expect(window.location.hash).toBe("#p=%2Fcode%2Fhedy&new");
  });

  it("swaps the questions for an AI summary, editable, and says when that fails", async () => {
    const { api, user } = setup("#p=%2Fcode%2Fhedy&new&from=c1");
    const summarize = vi.spyOn(api, "summarizeConversation");
    const start = vi.spyOn(api, "startTask");
    const card = await screen.findByRole("region", { name: CONTEXT });
    await user.click(within(card).getByRole("button", { name: CARD_TOGGLE }));

    api.summary = { error: "la cuenta «Claude personal» está en su límite" };
    const button = within(card).getByRole("button", { name: "Resumir con IA" });
    expect(button).toHaveAttribute("title", "Con la cuenta y el modelo de la conversación (Claude personal); gasta algunos tokens.");
    await user.click(button);
    expect(await screen.findByText("la cuenta «Claude personal» está en su límite")).toBeInTheDocument();

    api.summary = "- OAuth autoriza; OIDC autentica.";
    await user.click(within(card).getByRole("button", { name: "Resumir con IA" }));
    expect(summarize).toHaveBeenLastCalledWith("c1", [0]);
    const summary = await within(card).findByRole("textbox", { name: "Resumen que se lleva (puedes editarlo)" });
    expect(summary).toHaveValue("- OAuth autoriza; OIDC autentica.");
    await user.type(summary, " Usar OIDC.");
    expect(within(card).getByText(/Resumen hecho con IA/)).toBeInTheDocument();

    await user.type(screen.getByLabelText("Pídele algo a hedy"), "Añade el login");
    await user.click(screen.getByRole("button", { name: "Enviar" }));
    expect(start).toHaveBeenCalledWith(
      expect.objectContaining({
        carry: { conversation: "c1", turns: [0], summary: "- OAuth autoriza; OIDC autentica. Usar OIDC." },
      }),
    );
    const toggle = await screen.findByRole("button", { name: CONTEXT });
    await user.click(toggle);
    expect(screen.getByLabelText("Contexto que recibió el agente")).toHaveTextContent(/Resumen de la conversación \(hecho con IA\)/);
  });

  it("notes a long context and suggests a summary", async () => {
    const api = MockApi.withSampleData().withSampleActivity();
    const real = api.conversationContext.bind(api);
    vi.spyOn(api, "conversationContext").mockImplementation(async (id) => ({ ...(await real(id)), longAfter: 50 }));
    setup("#p=%2Fcode%2Fhedy&new&from=c1", api);
    const card = await screen.findByRole("region", { name: CONTEXT });
    expect(await within(card).findByText(/Es largo: resúmelo/)).toBeInTheDocument();
    expect(within(card).getByRole("button", { name: "Resumir con IA" })).toBeInTheDocument();
  });

  it("waits while the conversation is still answering", async () => {
    const api = MockApi.withSampleData();
    api.delayMs = 60_000;
    const chat = await api.startConversation({ prompt: "¿Qué es un monad?", mode: "ask", account: null, model: null });
    history.replaceState(null, "", `#c=${chat.id}`);
    render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
    const user = userEvent.setup();
    await screen.findByRole("heading", { name: "¿Qué es un monad?", level: 1 });

    const button = header().getByRole("button", { name: "Llevar a un proyecto" });
    expect(button).toHaveAttribute("aria-disabled", "true");
    await user.click(button);
    expect(await screen.findByText("Espera a que termine de responder")).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.queryByRole("group", { name: "¿Y ahora?" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Más acciones" }));
    expect(screen.getByRole("menuitem", { name: /Llevar a un proyecto…/ })).toBeDisabled();
    expect(screen.getByRole("menuitem", { name: /Guardar como nota del proyecto…/ })).toBeDisabled();
  });

  it("saves the answer as an approved note of a project", async () => {
    const { api, user } = setup();
    const save = vi.spyOn(api, "saveConversationNote");
    await conversationHeading();
    await user.click(screen.getByRole("button", { name: "Más acciones" }));
    await user.click(screen.getByRole("menuitem", { name: /Guardar como nota del proyecto…/ }));

    const dialog = screen.getByRole("dialog", { name: "Guardar como nota del proyecto" });
    expect(within(dialog).getByRole("combobox", { name: "Proyecto" })).toHaveTextContent("hedy");
    const title = within(dialog).getByRole("textbox", { name: "Título" });
    expect(title).toHaveValue(QUESTION);
    const text = within(dialog).getByRole("textbox", { name: "Nota" });
    await waitFor(() => expect((text as HTMLTextAreaElement).value).toMatch(/^OAuth 2.0 autoriza/));
    expect(within(dialog).getByText(/se guarda aprobada/)).toBeInTheDocument();
    await user.clear(title);
    await user.type(title, "OAuth vs OIDC");
    await user.click(within(dialog).getByRole("button", { name: "Guardar nota" }));

    expect(save).toHaveBeenCalledWith("c1", "/code/hedy", "OAuth vs OIDC", expect.stringMatching(/^OAuth 2.0 autoriza/), "decision");
    expect(await screen.findByText("Nota guardada en hedy")).toBeInTheDocument();
    const memory = await api.memory("/code/hedy");
    const note = memory.notes.find((n) => n.text.startsWith("OAuth vs OIDC\n\nOAuth 2.0 autoriza"));
    expect(note).toMatchObject({ status: "approved", author: "user" });
  });

  it("speaks English", async () => {
    const api = MockApi.withSampleData().withSampleActivity();
    void api.setSetting("ui.language", "en");
    const { user } = setup("#c=c1", api);
    await conversationHeading();
    await user.click(header().getByRole("button", { name: "Carry into a project" }));
    const dialog = screen.getByRole("dialog", { name: "Carry into a project" });
    await user.click(within(dialog).getByRole("button", { name: "Open task" }));
    const card = await screen.findByRole("region", { name: `Context: conversation «${QUESTION}»` });
    expect(await within(card).findByText("Questions 1/1 · 173 characters")).toBeInTheDocument();
    expect(screen.getByLabelText("Ask hedy for something")).toHaveAttribute("placeholder", "What should we do with this in hedy?");
  });

  it("carries the open conversation from the command palette", async () => {
    const { user } = setup();
    await conversationHeading();
    await user.keyboard("{Control>}k{/Control}");
    await user.type(screen.getByRole("combobox", { name: "Paleta de comandos" }), "llevar la conversación");
    await user.keyboard("{Enter}");
    // A page of projects, with a new one first.
    await user.type(screen.getByRole("combobox"), "hedy");
    await user.keyboard("{Enter}");
    expect(await screen.findByRole("heading", { name: "hedy", level: 1 })).toBeInTheDocument();
    expect(await screen.findByRole("region", { name: CONTEXT })).toBeInTheDocument();
  });
});
