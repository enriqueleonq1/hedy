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

/** Every text, accessible name, title and placeholder on screen (or in `root`) that is still Spanish chrome. */
function spanishLeftovers(root: HTMLElement = document.body): string[] {
  const texts = new Set<string>();
  for (const el of root.querySelectorAll("*")) {
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
    expect(await screen.findByRole("heading", { name: /¡Hola, Alex!/ })).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual(expect.arrayContaining(["Tus planes", "Nueva tarea", "Tu memoria"]));
  });

  it("greets on the home screen and frames it in English", async () => {
    const api = MockApi.withSampleData();
    // An API key spent something: the one cost worth adding up.
    api.recordUsage({ project: "/code/hedy", account: "kimi", at: Math.floor(Date.now() / 1000), tokens: 400, costUsd: 1.25 });
    setup(api);
    expect(await screen.findByRole("heading", { level: 1, name: "Hi, Alex!" })).toBeInTheDocument();
    expect(
      await screen.findByText("7 things are waiting; you can answer them right here. Claude personal is already at 83 % (this week)."),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "New task" })).toBeInTheDocument();

    // The sidebar.
    const sidebar = screen.getByRole("complementary", { name: "Navigation" });
    expect(await within(sidebar).findByRole("button", { name: "Home: 7 things are waiting for you" })).toBeInTheDocument();
    expect(within(sidebar).getByRole("button", { name: "Your memory" })).toBeInTheDocument();
    expect(within(sidebar).getByRole("button", { name: "Skills" })).toBeInTheDocument();
    expect(within(sidebar).getByRole("button", { name: "Search and run commands (Ctrl K)" })).toBeInTheDocument();
    expect(within(sidebar).getByRole("button", { name: "Add a project" })).toBeInTheDocument();
    expect(within(sidebar).getByRole("button", { name: /^hedy/ })).toBeInTheDocument();
    expect(within(sidebar).getByRole("button", { name: "New task in tienda" })).toBeInTheDocument();
    expect(within(within(sidebar).getByRole("button", { name: /^tienda/ })).getByText("Protected project: it only uses its account")).toBeInTheDocument();
    expect(within(sidebar).getByRole("heading", { name: "Chats" })).toBeInTheDocument();
    expect(within(sidebar).getByText("Ask something without opening a project.")).toBeInTheDocument();
    expect(within(sidebar).getByRole("button", { name: "Claude personal: Week · 83%. Open the account" })).toBeInTheDocument();
    expect(within(sidebar).getByRole("button", { name: /^Terminal/ })).toBeInTheDocument();
    expect(within(sidebar).getByRole("button", { name: "Settings" })).toBeInTheDocument();

    // The filters and the inbox, sync and team included.
    expect(screen.getByRole("radiogroup", { name: "Show" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: /^Need you\s*7$/ })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("radio", { name: "History" })).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Project" })).toHaveDisplayValue("All projects");
    expect(screen.getByRole("searchbox", { name: "Search tasks and pending items" })).toBeInTheDocument();
    const needs = screen.getByRole("region", { name: "Need you (7)" });
    expect(within(needs).getByText("One change on your computers to resolve")).toBeInTheDocument();
    expect(within(needs).getByRole("button", { name: "Resolve" })).toBeInTheDocument();
    expect(within(needs).getByText("2 suggestions from team Plataforma")).toBeInTheDocument();
    expect(within(needs).getByText("As an admin, you decide if they go into the team's memory")).toBeInTheDocument();
    expect(within(needs).getAllByText("Note to approve").length).toBeGreaterThan(0);

    // The plans.
    const plans = screen.getByRole("region", { name: "Your plans" });
    expect(within(plans).getByRole("button", { name: "Accounts" })).toBeInTheDocument();
    expect(within(plans).getByRole("meter", { name: "Claude trabajo: Current session (5 h) 34 %" })).toBeInTheDocument();
    expect(within(plans).getByRole("button", { name: /^Claude personal/ })).toHaveTextContent(/This week · Resets (today|tomorrow|on .+) at .+$/);
    expect(within(plans).getByRole("button", { name: /^ChatGPT/ })).toHaveTextContent("No data");
    expect(await within(plans).findByRole("button", { name: /^Kimi/ })).toHaveTextContent("Pay as you go");
    expect(
      await screen.findByText(/^In Hedy: 216k tokens this week \(about \$1\.25 on API keys\); hedy was the busiest project\.$/),
    ).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);
  });

  it("runs a task in the workspace in English", async () => {
    const { user } = setup();
    await user.click(await screen.findByRole("button", { name: /^hedy/ }));
    expect(await screen.findByText(/^What should we do in hedy\?/)).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Mode" })).toHaveTextContent("Change code");
    expect(screen.getByText("Enter send · Shift+Enter new line")).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);
    // Every menu of the composer speaks English too.
    await user.click(screen.getByRole("combobox", { name: "Mode" }));
    expect(screen.getByRole("option", { name: "Research", description: "Searches the web before answering" })).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);
    await user.click(screen.getByRole("combobox", { name: "Model" }));
    expect(screen.getByRole("option", { name: "Automatic", description: "Each phase uses the model you assigned it" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "Opus", description: "The most capable, spends more" })).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);
    await user.keyboard("{Escape}");
    await user.click(screen.getByRole("button", { name: "Templates" }));
    expect(screen.getByRole("menuitem", { name: "Use a template" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Save as template" })).toBeDisabled();
    expect(spanishLeftovers()).toEqual([]);
    await user.keyboard("{Escape}");

    await user.type(screen.getByLabelText("Ask hedy for something"), "Agrega un recordatorio");
    await user.click(screen.getByRole("button", { name: "Send" }));
    const plan = await screen.findByRole("region", { name: "Claude Opus's plan" });
    await user.click(within(plan).getByRole("button", { name: "Approve plan" }));

    const card = await screen.findByRole("region", { name: "Claude wants to run a command" });
    await user.click(screen.getByRole("button", { name: "2 steps · edited 2 files" }));
    expect(screen.getByRole("button", { name: "citas/servicio.py" }).parentElement).toHaveTextContent("Claude edited citas/servicio.py");
    await user.click(within(card).getByRole("button", { name: "Always allow in this task" }));
    expect(await screen.findByText(/Claude: tests pass/)).toBeInTheDocument();
    expect(await screen.findByText(/^Finished/, { selector: ".turn-end" })).toBeInTheDocument();
    expect(screen.getByText(/Always allowed in this task/)).toBeInTheDocument();
    expect(within(screen.getByRole("region", { name: "Steps" })).getAllByText(/\(done\)/)).toHaveLength(3);
    expect(await screen.findByRole("list", { name: "Changed files" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Accept changes" })).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);
  });

  it("queues messages and picks templates in English", async () => {
    const { user } = setup(MockApi.withSampleData().withSampleActivity().withSampleTemplates());
    await user.click(await screen.findByRole("button", { name: /^tienda/ }));
    // The project opens on its home: its tasks by what they need, in English.
    expect(await screen.findByRole("heading", { level: 1, name: "tienda" })).toBeInTheDocument();
    const working = await screen.findByRole("region", { name: "Working" });
    expect(within(working).getByRole("button", { name: /^Revisa los tests del carrito/ })).toHaveTextContent(/Working$/);
    expect(screen.getByRole("region", { name: "Waiting for you" })).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);
    await user.click(within(working).getByRole("button", { name: /^Revisa los tests del carrito/ }));
    const queue = await screen.findByRole("region", { name: "Queued messages" });
    expect(within(queue).getAllByText("Queued")).toHaveLength(2);
    expect(within(queue).getByText(/goes as a single message when Claude Opus finishes its turn/)).toBeInTheDocument();
    expect(within(queue).getAllByRole("button", { name: "Edit the queued message" })).toHaveLength(2);
    const composer = screen.getByLabelText(/^Write while Claude Opus works/);
    // Empty, the composer offers to stop the agent; with text, to queue.
    expect(screen.getByRole("button", { name: "Stop" })).toBeInTheDocument();
    expect(composer).toHaveAttribute("placeholder", "Write; I will pass it on when its turn ends…");
    expect(spanishLeftovers()).toEqual([]);

    await user.click(screen.getByRole("button", { name: "Tasks (3)" }));
    const tasks = screen.getByRole("group", { name: "Tasks in this project" });
    expect(within(tasks).getByRole("button", { name: "New task" })).toBeInTheDocument();
    expect(within(tasks).getByText("Waiting for you: permission")).toBeInTheDocument();
    expect(within(tasks).getByText("Failed")).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);
    await user.keyboard("{Escape}");

    await user.type(composer, "/");
    const list = await screen.findByRole("listbox", { name: "Templates and commands" });
    expect(within(list).getByText("Write tests")).toBeInTheDocument();
    // This account has not answered in Hedy yet: its commands are not known.
    expect(screen.getByText("This account's commands show up after its first answer")).toBeInTheDocument();
    expect(screen.getByText("Claude trabajo commands")).toBeInTheDocument();
    expect(screen.getByText("↑ ↓ to choose · Enter to use · Esc to close")).toBeInTheDocument();
    await user.type(composer, "write");
    await user.keyboard("{Enter}");
    const fill = await screen.findByRole("group", { name: "Fill in “Write tests”" });
    expect(within(fill).getByLabelText("{file}")).toHaveFocus();
    expect(within(fill).getByRole("button", { name: "Use template" })).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);
  });

  it("keeps a queue on hold in English", async () => {
    const { api, user } = setup();
    await user.click(await screen.findByRole("button", { name: /^hedy/ }));
    await user.type(await screen.findByLabelText("Ask hedy for something"), "Recordatorios");
    await user.click(screen.getByRole("button", { name: "Plan first" }));
    await user.click(screen.getByRole("button", { name: "Send" }));
    await screen.findByRole("region", { name: "Claude wants to run a command" });
    await user.type(screen.getByLabelText(/^Answer the permission request above first/), "Sigue");
    await user.click(screen.getByRole("button", { name: "Send afterwards" }));
    const queue = await screen.findByRole("region", { name: "Queued messages" });
    expect(within(queue).getByText("Answer the card above first; the queue goes after that.")).toBeInTheDocument();

    api.simulateLimit((await api.tasks("/code/hedy"))[0]!.id, "reached");
    expect(await within(queue).findByText("On hold")).toBeInTheDocument();
    expect(within(queue).getByText(/the account reached its limit/)).toBeInTheDocument();
    expect(within(queue).getByRole("button", { name: "Send when the turn ends" })).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);
  });

  it("warns about a limit in English", async () => {
    const { api, user } = setup();
    await user.click(await screen.findByRole("button", { name: /^hedy/ }));
    await user.type(await screen.findByLabelText("Ask hedy for something"), "Recordatorios");
    await user.click(screen.getByRole("button", { name: "Plan first" }));
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
    await user.click(await screen.findByRole("button", { name: /Open the account$/ }));
    expect(await screen.findByRole("heading", { name: "Your accounts and models" })).toBeInTheDocument();
    const card = screen.getByRole("article", { name: "Claude trabajo" });
    expect(within(card).getByText("Connected")).toBeInTheDocument();
    expect(within(card).getByRole("button", { name: "Actions for Claude trabajo" })).toBeInTheDocument();
    expect(screen.getByText(/Each plan.s limit is reported by its CLI/)).toBeInTheDocument();
    expect(within(card).getByText(/CLI's default folder/)).toBeInTheDocument();
    const personal = screen.getByRole("article", { name: "Claude personal" });
    expect(within(personal).getByRole("meter", { name: "Current session (5 h): 58%" })).toBeInTheDocument();
    expect(within(personal).getByRole("meter", { name: "This week: 83%" })).toBeInTheDocument();
    expect(within(personal).getByText("Per model: Opus 12%")).toBeInTheDocument();
    expect(within(personal).getByText("from Claude Code's /usage · 2 min ago")).toBeInTheDocument();
    expect(within(personal).getByText("Tokens in Hedy this week")).toBeInTheDocument();
    expect(within(personal).getAllByText(/^Resets (today|tomorrow|on .+) at .+$/)).toHaveLength(2);
    expect(within(screen.getByRole("article", { name: "ChatGPT" })).getByText("could not refresh")).toBeInTheDocument();
    expect(within(screen.getByRole("article", { name: "ChatGPT" })).getByRole("button", { name: "Sign in" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "This week's usage by client" })).toBeInTheDocument();
    expect(screen.getByLabelText("Client for hedy")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "+ Add account" }));
    const dialog = screen.getByRole("dialog", { name: "Add account" });
    expect(within(dialog).getByText("Pro, Max or Team subscription")).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Create and sign in" })).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);
  });

  it("shows Home's inbox and filters in English", async () => {
    const { user } = setup(MockApi.withSampleData().withSampleActivity());
    const needs = await screen.findByRole("region", { name: "Need you (10)" }, { timeout: 8000 });
    expect(screen.getByRole("button", { name: "Continue «Revisa los tests del carrito»" })).toBeInTheDocument();
    expect(within(needs).getByText("Plan to approve")).toBeInTheDocument();
    expect(within(needs).getByText("Asks for permission")).toBeInTheDocument();
    expect(within(needs).getByText("Paused by a limit")).toBeInTheDocument();
    expect(within(needs).getByText(/^Hedy never switches accounts on its own: /)).toBeInTheDocument();
    // The inbox also holds the proposals, answered from their rows.
    expect(await within(needs).findByRole("button", { name: "Allow once" })).toBeInTheDocument();
    expect(within(needs).getByRole("button", { name: "Deny" })).toBeInTheDocument();
    expect(within(needs).getAllByText("Note to approve")).toHaveLength(2);
    expect(within(needs).getByText("Skill change to approve")).toBeInTheDocument();
    expect(within(needs).getAllByText("proposed by Codex").length).toBeGreaterThan(0);
    expect(within(needs).getAllByRole("button", { name: "Approve change" })).toHaveLength(2);
    expect(within(needs).getAllByText(/^running for \d+ min$/)).toHaveLength(3);
    const ready = screen.getByRole("region", { name: "Ready to review (3)" });
    expect(within(ready).getByText("Changes to review")).toBeInTheDocument();
    expect(within(ready).getByRole("button", { name: "Review changes" })).toBeInTheDocument();
    expect(within(ready).getAllByText("Answered").length).toBeGreaterThan(0);
    expect(within(ready).getByText("40 min ago")).toBeInTheDocument();
    expect(within(ready).getByText("Conversation")).toBeInTheDocument();
    expect(within(ready).getAllByRole("button", { name: "See the answer" })).toHaveLength(2);
    // The limit's row, already waiting: hand it over or stop waiting.
    const limit = within(needs).getByRole("group", { name: "Limit of Traduce los correos de confirmación" });
    expect(await within(limit).findByRole("button", { name: "Hand it to ChatGPT" })).toBeInTheDocument();
    expect(within(limit).getByRole("button", { name: "Stop waiting" })).toBeInTheDocument();
    const failed = screen.getByRole("region", { name: "Stopped with an error (1)" });
    expect(within(failed).getByRole("button", { name: "See what happened" })).toBeInTheDocument();
    const side = screen.getByRole("complementary", { name: "Your plans and what is running" });
    expect(within(side).getByRole("heading", { name: "Running" })).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);

    await user.click(screen.getByRole("radio", { name: /^Running\s*1$/ }));
    const running = screen.getByRole("region", { name: "Running (1)" });
    expect(within(running).getByText("Working")).toBeInTheDocument();
    expect(await within(running).findByText("Sniffing around")).toBeInTheDocument();
    expect(within(running).getByText("Implementation")).toBeInTheDocument();
    expect(within(running).getByText("2 queued")).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);

    await user.click(screen.getByRole("radio", { name: "History" }));
    const history = await screen.findByRole("region", { name: "History" });
    expect((await within(history).findAllByText("Failed"))[0]).toBeInTheDocument();
    expect(within(history).getAllByText("Ready")[0]).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);

    await user.click(screen.getByRole("radio", { name: /^Need you/ }));
    await user.click(within(screen.getByRole("region", { name: "Need you (10)" })).getByRole("button", { name: /Traduce los correos/ }));
    expect(await screen.findByRole("heading", { name: "Traduce los correos de confirmación" })).toBeInTheDocument();
  });

  it("shows the command palette and the status bar in English", async () => {
    const { user } = setup(MockApi.withSampleData().withSampleActivity());
    await screen.findByRole("heading", { name: /Hi, Alex!/ });
    // The status bar belongs to the workspace.
    expect(screen.queryByRole("contentinfo", { name: "Status bar" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /^hedy/ }));
    const bar = await screen.findByRole("contentinfo", { name: "Status bar" });
    expect(within(bar).getByRole("button", { name: "Keyboard shortcuts (?)" })).toBeInTheDocument();
    // Only the bar: the task's plan behind it is sample text, in Spanish.
    expect(spanishLeftovers(bar)).toEqual([]);

    await user.click(screen.getByRole("button", { name: /^Home/ }));
    await screen.findByRole("heading", { name: /Hi, Alex!/ });
    await user.keyboard("{Control>}k{/Control}");
    expect(screen.getByRole("dialog", { name: "Command palette" })).toBeInTheDocument();
    expect(await screen.findByRole("group", { name: "Needs you" }, { timeout: 8000 })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: /^Answer: Migra el reporte/ })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: /^Review 2 suggestions from team Plataforma/ })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: /^Open a terminal in hedy/ })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: /^Claude trabajo account/ })).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);
  });

  it("shows Home's empty states in English, at the old Activity address too", async () => {
    window.history.replaceState(null, "", "#activity");
    const { user } = setup();
    await screen.findByRole("heading", { level: 1, name: "Hi, Alex!" });
    await user.click(await screen.findByRole("radio", { name: /^Running\s*0$/ }));
    expect(screen.getByText("No AI is working right now")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "Sleeping" })).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);
    await user.click(screen.getByRole("radio", { name: /^Ready\s*0$/ }));
    expect(screen.getByText("Nothing to review")).toBeInTheDocument();
    await user.click(screen.getByRole("radio", { name: /^Need you/ }));
    await user.type(screen.getByRole("searchbox", { name: "Search tasks and pending items" }), "nothing like this");
    expect(await screen.findByText("Nothing is waiting for you")).toBeInTheDocument();
    expect(screen.getByText("When an AI needs something from you, it shows up here.")).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);
    window.history.replaceState(null, "", "#");
  });

  it("shows settings in English", async () => {
    window.history.replaceState(null, "", "#settings");
    setup();
    expect(await screen.findByRole("heading", { name: "Settings" })).toBeInTheDocument();
    expect(screen.getByRole("radiogroup", { name: "Mode" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Light" })).toBeInTheDocument();
    expect(screen.getByText("Done!")).toBeInTheDocument();
    const working = screen.getByLabelText("The mascot at work, by what the AI is doing");
    for (const name of ["Reading files", "Searching the web", "Thinking it through", "Waiting for a slow answer"]) {
      expect(within(working).getByRole("img", { name })).toBeInTheDocument();
    }
    expect(within(working).getByText("Sniffing around · Tracking · Searching")).toBeInTheDocument();
    expect(screen.getByLabelText("Your name")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Code map" })).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Tell me when a task needs me or finishes" })).toBeInTheDocument();
    const templates = screen.getByRole("region", { name: "Templates" });
    expect(await within(templates).findByText("Explain code")).toBeInTheDocument();
    expect(within(templates).getAllByText("Every project").length).toBeGreaterThan(0);
    expect(within(templates).getByRole("button", { name: "Restore examples" })).toBeInTheDocument();
    expect(within(templates).getByRole("button", { name: "Edit Explain code" })).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);
  });

  it("shows a project's status and memory in English", async () => {
    const { user } = setup();
    await user.click(await screen.findByRole("button", { name: /^hedy/ }));
    await user.click(within(await screen.findByRole("navigation", { name: "Project sections" })).getByRole("button", { name: "Summary" }));
    expect(await screen.findByText("Project status")).toBeInTheDocument();
    expect(within(await screen.findByRole("region", { name: "Tests" })).getByText("Passed")).toBeInTheDocument();
    const changes = screen.getByRole("region", { name: "Changes" });
    expect(within(changes).getByText("3 uncommitted")).toBeInTheDocument();
    expect(within(changes).getByText("branch main")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Turn on the code map" })).toBeInTheDocument();
    expect(await screen.findByText(/was the busiest day/)).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);

    await user.click(screen.getByRole("button", { name: "Memory" }));
    expect(await screen.findByRole("heading", { name: "hedy memory" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Decisions" })).toBeInTheDocument();
    expect(screen.getByRole("searchbox", { name: "Search memory" })).toBeInTheDocument();
    // A proposed change under a topic, and a proposal similar to an approved note.
    const proposed = screen.getByRole("region", { name: "Proposed by your AIs · awaiting approval" });
    expect(within(proposed).getByText("Change to a note with the same topic")).toBeInTheDocument();
    expect(within(proposed).getByText("Now says")).toBeInTheDocument();
    expect(within(proposed).getByRole("button", { name: "Approve change" })).toBeInTheDocument();
    const similar = within(proposed).getByRole("region", { name: "Similar to" });
    expect(within(similar).getByRole("button", { name: "Replace that one" })).toBeInTheDocument();
    expect(within(proposed).getByRole("button", { name: "Keep both" })).toBeInTheDocument();
    expect(within(proposed).getByRole("button", { name: "Discard the proposal" })).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);
  });

  it("shows a project's skills in English", async () => {
    const { user } = setup();
    await user.click(await screen.findByRole("button", { name: /^hedy/ }));
    await user.click(within(await screen.findByRole("navigation", { name: "Project sections" })).getByRole("button", { name: "Memory" }));
    await user.click(await screen.findByRole("tab", { name: /^Skills/ }));
    const proposed = await screen.findByRole("region", { name: "Proposed by your AIs · awaiting approval" });
    expect(screen.getByRole("tab", { name: /Skills.*2 to approve/ })).toHaveAttribute("aria-selected", "true");
    expect(within(proposed).getByText("Update of an approved skill")).toBeInTheDocument();
    expect(within(proposed).getByRole("button", { name: "Approve change" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "In this project" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Yours · in every project" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Where your AIs read them" })).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);

    const endpoint = within(proposed).getByRole("listitem", { name: "agregar-endpoint" });
    await user.click(within(endpoint).getByRole("button", { name: "Edit" }));
    expect(screen.getByRole("button", { name: "Save and approve" })).toBeInTheDocument();
    expect(screen.getByRole("radiogroup", { name: "Where" })).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);

    await user.click(screen.getByRole("button", { name: "Save and approve" }));
    expect(await screen.findByText("Skill «agregar-endpoint» ready: your AIs use it inside Hedy.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "+ New skill" }));
    expect(screen.getByRole("dialog", { name: "New skill" })).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);
  });

  it("starts a new project in English", async () => {
    const { user } = setup();
    await screen.findByRole("heading", { name: /Hi, Alex!/ });
    await user.click(screen.getByRole("button", { name: "Add a project" }));
    expect(screen.getByRole("menuitem", { name: /New project.*A new folder, ready to work in/ })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: /Open folder….*A project you already have on this computer/ })).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);
    await user.click(screen.getByRole("menuitem", { name: /New project/ }));
    const dialog = screen.getByRole("dialog", { name: "New project" });
    await user.type(within(dialog).getByRole("textbox", { name: "Name" }), "Notes app");
    expect(await within(dialog).findByText("/code/notes-app")).toBeInTheDocument();
    expect(within(dialog).getByText(/^Creates/)).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: /More options/ }));
    expect(within(dialog).getByRole("switch", { name: /./ })).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);
    await user.click(within(dialog).getByRole("button", { name: "Create project" }));
    expect(await screen.findByText("Notes app is ready. Write its first task.")).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);
  });

  it("asks a quick question in English", async () => {
    const { user } = setup(MockApi.withSampleData().withSampleActivity());
    await screen.findByRole("heading", { name: /Hi, Alex!/ });
    await user.click(screen.getByRole("button", { name: "Quick question" }));
    expect(await screen.findByRole("heading", { name: "Quick question" })).toBeInTheDocument();
    expect(screen.getByText("No project · never touches your files")).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "What do you want to know?" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Conversations (1)" })).toBeInTheDocument();
    expect(screen.getByText("Enter send · Shift+Enter new line")).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);
    // The account and model menus, with the plan's usage and the promise about switching.
    await user.click(screen.getByRole("combobox", { name: "Account for the question" }));
    expect(screen.getByRole("listbox", { name: "Account" })).toBeInTheDocument();
    expect(screen.getByText("Hedy never switches accounts on its own")).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);
    await user.click(screen.getByRole("combobox", { name: "Model" }));
    expect(screen.getByRole("option", { name: "Account's model", description: "Whatever the account uses by default" })).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);
    await user.keyboard("{Escape}");
    await user.type(screen.getByRole("textbox", { name: "What do you want to know?" }), "What is a monad?{Enter}");
    expect(await screen.findByRole("heading", { name: "What is a monad?" })).toBeInTheDocument();
    expect(await screen.findByText(/^Short answer/)).toBeInTheDocument();
    expect(screen.getByText(/is answering/)).toBeInTheDocument();
    await user.click(await screen.findByRole("button", { name: "More actions" }));
    expect(screen.getByRole("menuitem", { name: "Remove this conversation" })).toBeInTheDocument();
    await user.click(screen.getByRole("menuitem", { name: "Remove this conversation" }));
    expect(screen.getByRole("dialog", { name: "Remove this conversation?" })).toBeInTheDocument();
    expect(spanishLeftovers()).toEqual([]);
  });
});
