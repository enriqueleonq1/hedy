import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "./App";
import { MockApi } from "./api/MockApi";
import { messages } from "./i18n/messages";

// xterm needs a real canvas; terminals are not exercised here.
vi.mock("./components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

/** The app in English, over the (Spanish) sample data. */
function setup(api = MockApi.withSampleData()) {
  api.delayMs = 0;
  void api.setSetting("ui.language", "en");
  render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
  return { api, user: userEvent.setup() };
}

const escape = (text: string) => text.replace(/[.*+?^$()|[\]\\]/g, "\\$&");

/**
 * Spanish texts of the interface that read differently in English, as
 * patterns: `{param}` matches anything. Too short a fixed part ("«{title}»")
 * would match sample data, so those are left out.
 */
const SPANISH = (Object.keys(messages.es) as (keyof typeof messages.es)[])
  .filter((key) => messages.es[key] !== messages.en[key])
  .map((key) => messages.es[key].trim())
  .filter((text) => text.replace(/\{\w+\}/g, "").replace(/[^\p{L}]/gu, "").length >= 4)
  .map((text) => new RegExp(`^${text.split(/\{\w+\}/).map(escape).join(".+")}$`, "u"));

/** Every text, accessible name, title and placeholder on screen that is still Spanish chrome. */
function spanishLeftovers(): string[] {
  const texts = new Set<string>();
  for (const el of document.body.querySelectorAll("*")) {
    for (const attr of ["aria-label", "title", "placeholder"]) {
      const value = el.getAttribute(attr)?.trim();
      if (value) texts.add(value);
    }
    const own = [...el.childNodes]
      .filter((n) => n.nodeType === Node.TEXT_NODE)
      .map((n) => n.textContent)
      .join("")
      .trim();
    if (own) texts.add(own);
    const all = el.textContent?.trim();
    if (all) texts.add(all);
  }
  return [...texts].filter((text) => SPANISH.some((pattern) => pattern.test(text)));
}

describe("English", () => {
  it("catches Spanish chrome (the check itself)", async () => {
    const api = MockApi.withSampleData();
    render(<App api={api} queryClient={new QueryClient()} />);
    expect(await screen.findByRole("heading", { name: /¡Hola, Enrique!/ })).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual(expect.arrayContaining(["Cuentas y modelos", "¿En qué trabajamos hoy?"]));
  });

  it("greets on the home screen and frames it in English", async () => {
    setup();
    expect(await screen.findByRole("heading", { name: /Hi, Enrique!/ })).toBeInTheDocument();
    expect(screen.getByText("What are we working on today?")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "Hedy says hi" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Accounts and models" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Settings" })).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "Open projects" })).toBeInTheDocument();
    const homming = screen.getByRole("heading", { name: "homming" }).closest("article")!;
    expect(within(homming).getByText("Protected")).toBeInTheDocument();
    expect(
      await screen.findByText(/^216k tokens so far \(about \$4\.1\d\); hedy was the busiest project\.$/),
    ).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);
  });

  it("runs a task in the workspace in English", async () => {
    const { user } = setup();
    await user.click(await screen.findByRole("button", { name: /^hedy/ }));
    expect(await screen.findByText(/^What should we do in hedy\?/)).toBeInTheDocument();
    expect(screen.getByRole("radiogroup", { name: "Mode" })).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);

    await user.type(screen.getByLabelText("Ask hedy for something"), "Agrega un recordatorio");
    await user.click(screen.getByRole("button", { name: "Send" }));
    const plan = await screen.findByRole("region", { name: "Claude Opus's plan" });
    await user.click(within(plan).getByRole("button", { name: "Approve plan" }));

    const card = await screen.findByRole("region", { name: "Claude wants to run a command" });
    expect(screen.getByText("Claude edited citas/servicio.py")).toBeInTheDocument();
    await user.click(within(card).getByRole("button", { name: "Allow in this chat" }));
    expect(await screen.findByText(/Claude: tests pass/)).toBeInTheDocument();
    expect(await screen.findByText(/^Finished/)).toBeInTheDocument();
    expect(screen.getByText(/Allowed in this chat/)).toBeInTheDocument();
    expect(within(screen.getByRole("region", { name: "Steps" })).getAllByText(/\(done\)/)).toHaveLength(3);
    expect(await screen.findByRole("list", { name: "Changed files" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Accept changes" })).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);
  });

  it("warns about a limit in English", async () => {
    const { api, user } = setup();
    await user.click(await screen.findByRole("button", { name: /^hedy/ }));
    await user.type(await screen.findByLabelText("Ask hedy for something"), "Recordatorios");
    await user.click(screen.getByLabelText("Plan first"));
    await user.click(screen.getByRole("button", { name: "Send" }));
    await screen.findByRole("region", { name: "Claude wants to run a command" });

    const task = (await api.tasks("/code/hedy"))[0]!;
    api.simulateLimit(task.id);
    const card = await screen.findByRole("region", { name: "Your Claude personal account is at 92%" });
    expect(await within(card).findByRole("button", { name: "Hand the task to ChatGPT" })).toBeInTheDocument();
    expect(within(card).getByRole("button", { name: "Wait" })).toBeInTheDocument();
    expect(within(card).getByText("I can continue the task with another model.")).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);
  });

  it("shows accounts, usage and projects in English", async () => {
    const { user } = setup();
    await user.click(await screen.findByRole("button", { name: "Accounts and models" }));
    expect(await screen.findByRole("heading", { name: "Your accounts and models" })).toBeInTheDocument();
    const card = screen.getByRole("article", { name: "Claude trabajo" });
    expect(within(card).getByText(/Connected · team/)).toBeInTheDocument();
    expect(within(card).getByText(/CLI's default folder/)).toBeInTheDocument();
    const personal = screen.getByRole("article", { name: "Claude personal" });
    expect(within(personal).getByRole("meter", { name: "Plan limit: 34%" })).toBeInTheDocument();
    expect(within(screen.getByRole("article", { name: "ChatGPT" })).getByRole("button", { name: "Sign in" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "This week's usage by client" })).toBeInTheDocument();
    expect(screen.getByLabelText("Client for hedy")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "+ Add account" }));
    const dialog = screen.getByRole("dialog", { name: "Add account" });
    expect(within(dialog).getByText("Pro, Max or Team subscription")).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Create and sign in" })).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);
  });

  it("shows the task board in English", async () => {
    const { user } = setup(MockApi.withSampleData().withSampleActivity());
    await user.click(await screen.findByRole("button", { name: "Activity: 3 tasks need you" }));
    expect(await screen.findByRole("heading", { name: "Activity" })).toBeInTheDocument();
    const needs = await screen.findByRole("region", { name: "Need you (3)" });
    expect(within(needs).getByText("Plan to approve")).toBeInTheDocument();
    expect(within(needs).getByText("Asks for permission")).toBeInTheDocument();
    expect(within(needs).getByText("Blocked by a limit")).toBeInTheDocument();
    expect(within(needs).getAllByText(/^running for \d+ min$/)).toHaveLength(3);
    const running = screen.getByRole("region", { name: "Running (1)" });
    expect(within(running).getByText("Working")).toBeInTheDocument();
    expect(within(running).getByText("Implementation")).toBeInTheDocument();
    const recent = screen.getByRole("region", { name: "Last 24 h (4)" });
    for (const state of ["Changes to review", "Failed", "Answered", "Cancelled"]) {
      expect(within(recent).getByText(state)).toBeInTheDocument();
    }
    expect(within(recent).getByText("40 min ago")).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);

    await user.click(within(needs).getByRole("button", { name: /Traduce los correos/ }));
    expect(await screen.findByRole("heading", { name: "Traduce los correos de confirmación" })).toBeInTheDocument();
  });

  it("shows the empty task board in English", async () => {
    window.history.replaceState(null, "", "#activity");
    setup();
    expect(await screen.findByText("No running tasks, and none in the last 24 h.")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "Sleeping" })).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);
    window.history.replaceState(null, "", "#");
  });

  it("shows settings in English", async () => {
    window.history.replaceState(null, "", "#settings");
    setup();
    expect(await screen.findByRole("heading", { name: "Settings" })).toBeInTheDocument();
    expect(screen.getByRole("radiogroup", { name: "Theme" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Light" })).toBeInTheDocument();
    expect(screen.getByText("Done!")).toBeInTheDocument();
    expect(screen.getByLabelText("Your name")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Code map" })).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Tell me when a task needs me or finishes" })).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);
  });

  it("shows a project's status and memory in English", async () => {
    const { user } = setup();
    await user.click(await screen.findByRole("button", { name: /^hedy/ }));
    await user.click(await screen.findByRole("button", { name: "Status" }));
    expect(await screen.findByText("Project status")).toBeInTheDocument();
    expect(within(await screen.findByRole("region", { name: "Tests" })).getByText("Passed")).toBeInTheDocument();
    expect(screen.getByText(/^branch main · 3 uncommitted changes$/)).toBeInTheDocument();
    expect(await screen.findByText(/was the busiest day/)).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);

    await user.click(screen.getByRole("button", { name: "Memory" }));
    expect(await screen.findByRole("heading", { name: "hedy memory" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Decisions" })).toBeInTheDocument();
    expect(screen.getByRole("searchbox", { name: "Search memory" })).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);
  });
});
