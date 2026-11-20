import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { useEffect } from "react";

import { I18nProvider } from "../../i18n";
import { DEFAULT_TERMINAL_HEIGHT, MIN_TERMINAL_HEIGHT, TerminalPanel } from "./TerminalPanel";
import { TerminalProvider, useTerminals } from "./TerminalProvider";

// xterm needs a real canvas; the panel's sizing is what is tested here.
vi.mock("./TerminalView", () => ({ TerminalView: () => null }));

function OpenOne() {
  const { open } = useTerminals();
  useEffect(() => open({ kind: "shell", path: null }, "Terminal"), [open]);
  return null;
}

function setup() {
  const user = userEvent.setup();
  render(
    <QueryClientProvider client={new QueryClient()}>
      <I18nProvider setting="es">
        <TerminalProvider>
          <OpenOne />
          <TerminalPanel />
        </TerminalProvider>
      </I18nProvider>
    </QueryClientProvider>,
  );
  const edge = screen.getByRole("separator", { name: "Cambiar el alto de la terminal" });
  const panel = screen.getByRole("region", { name: "Terminal" });
  return { user, edge, panel };
}

// jsdom's window is 768 px tall, and 180 px always stay above the terminal.
const MAX = 768 - 180;

beforeEach(() => localStorage.clear());

describe("terminal panel size", () => {
  it("starts at the default height, with a horizontal edge to resize it", () => {
    const { edge, panel } = setup();
    expect(panel).toHaveStyle({ height: `${DEFAULT_TERMINAL_HEIGHT}px` });
    expect(edge).toHaveAttribute("aria-orientation", "horizontal");
    expect(edge).toHaveAttribute("aria-valuenow", String(DEFAULT_TERMINAL_HEIGHT));
  });

  it("grows when dragged up and shrinks when dragged down, within its limits", () => {
    const { edge, panel } = setup();
    fireEvent.pointerDown(edge, { button: 0, clientY: 400 });
    fireEvent.pointerMove(edge, { clientY: 300 });
    expect(panel).toHaveStyle({ height: `${DEFAULT_TERMINAL_HEIGHT + 100}px` });
    fireEvent.pointerMove(edge, { clientY: 2000 });
    fireEvent.pointerUp(edge);
    expect(panel).toHaveStyle({ height: `${MIN_TERMINAL_HEIGHT}px` });
    expect(localStorage.getItem("hedy.terminal.height")).toBe(String(MIN_TERMINAL_HEIGHT));
  });

  it("moves with the arrow keys", async () => {
    const { user, edge, panel } = setup();
    edge.focus();
    await user.keyboard("{ArrowUp}");
    expect(panel).toHaveStyle({ height: `${DEFAULT_TERMINAL_HEIGHT + 16}px` });
    await user.keyboard("{Shift>}{ArrowDown}{/Shift}");
    expect(panel).toHaveStyle({ height: `${DEFAULT_TERMINAL_HEIGHT + 16 - 64}px` });
  });

  it("maximizes and restores from the button or a double-click, as in VS Code", async () => {
    const { user, edge, panel } = setup();
    await user.click(screen.getByRole("button", { name: "Maximizar" }));
    expect(panel).toHaveStyle({ height: `${MAX}px` });
    await user.click(screen.getByRole("button", { name: "Restaurar" }));
    expect(panel).toHaveStyle({ height: `${DEFAULT_TERMINAL_HEIGHT}px` });
    await user.dblClick(edge);
    expect(panel).toHaveStyle({ height: `${MAX}px` });
  });

  it("remembers the height", () => {
    localStorage.setItem("hedy.terminal.height", "260");
    const { panel } = setup();
    expect(panel).toHaveStyle({ height: "260px" });
  });
});

describe("terminal tabs", () => {
  function OpenTwo() {
    const { open } = useTerminals();
    useEffect(() => {
      open({ kind: "shell", path: "/code/a" }, "Uno");
      open({ kind: "shell", path: "/code/b" }, "Dos");
    }, [open]);
    return null;
  }

  it("keeps only tabs in the tablist; arrow keys move between them, the rest is beside it", async () => {
    const user = userEvent.setup();
    render(
      <QueryClientProvider client={new QueryClient()}>
        <I18nProvider setting="es">
          <TerminalProvider>
            <OpenTwo />
            <TerminalPanel />
          </TerminalProvider>
        </I18nProvider>
      </QueryClientProvider>,
    );
    const list = screen.getByRole("tablist", { name: "Terminales" });
    expect(list.querySelectorAll("button:not([role=tab])")).toHaveLength(0);
    const one = screen.getByRole("tab", { name: "Uno" });
    const two = screen.getByRole("tab", { name: "Dos" });
    expect(two).toHaveAttribute("aria-selected", "true");
    expect(one).toHaveAttribute("tabindex", "-1");

    two.focus();
    await user.keyboard("{ArrowLeft}");
    expect(one).toHaveFocus();
    expect(one).toHaveAttribute("aria-selected", "true");
    expect(one).toHaveAttribute("tabindex", "0");

    // Closing is for the terminal on screen, outside the tablist.
    await user.click(screen.getByRole("button", { name: "Cerrar Uno" }));
    expect(screen.queryByRole("tab", { name: "Uno" })).toBeNull();
    expect(screen.getByRole("tab", { name: "Dos" })).toHaveAttribute("aria-selected", "true");
  });
});
