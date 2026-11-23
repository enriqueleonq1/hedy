import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../App";
import { MockApi } from "../api/MockApi";

// xterm needs a real canvas; no terminal is opened here.
vi.mock("./Terminal/TerminalView", () => ({ TerminalView: () => null }));

function setup() {
  const api = MockApi.withSampleData().withSampleActivity();
  api.delayMs = 0;
  render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
  return { api, user: userEvent.setup() };
}

afterEach(() => localStorage.clear());

describe("A project's latest tasks in the sidebar", () => {
  it("unfolds under the project on screen, opens a task and folds away", async () => {
    const { api, user } = setup();
    await user.click(await screen.findByRole("button", { name: /^hedy/ }));
    const toggle = screen.getByRole("button", { name: "Ocultar las tareas de hedy" });
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    const list = await screen.findByRole("list", { name: "Últimas tareas" });
    const tasks = await api.tasks("/code/hedy");
    await waitFor(() => expect(within(list).getAllByRole("button")).toHaveLength(Math.min(tasks.length, 10)));
    // Newest first, each opening its task.
    const newest = [...tasks].sort((a, b) => b.createdAt - a.createdAt)[0];
    const first = within(list).getAllByRole("button")[0];
    if (!first || !newest) throw new Error("hedy has tasks");
    expect(first).toHaveAttribute("title", expect.stringContaining(newest.title));
    await user.click(first);
    await waitFor(() => expect(first).toHaveAttribute("aria-current", "page"));

    await user.click(toggle);
    expect(screen.queryByRole("list", { name: "Últimas tareas" })).toBeNull();
    expect(screen.getByRole("button", { name: "Ver las tareas de hedy" })).toHaveAttribute("aria-expanded", "false");
  });

  it("unfolds another project's tasks too, and remembers it", async () => {
    const { user } = setup();
    await user.click(await screen.findByRole("button", { name: "Ver las tareas de tienda" }));
    expect(await screen.findByRole("list", { name: "Últimas tareas" })).toBeInTheDocument();
    expect(JSON.parse(localStorage.getItem("hedy.sidebar.tasks") ?? "{}")).toMatchObject({ "/code/tienda": true });
  });
});
