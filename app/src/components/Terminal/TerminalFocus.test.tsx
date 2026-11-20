import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { useEffect } from "react";

import { ApiProvider } from "../../api/context";
import { MockApi } from "../../api/MockApi";
import { I18nProvider } from "../../i18n";
import { TerminalPanel } from "./TerminalPanel";
import { TerminalProvider, useTerminals } from "./TerminalProvider";

// xterm needs a real canvas: a stand-in that records which terminal took the
// keyboard, so the panel and the real TerminalView decide when to focus.
const focused: string[] = [];
let opened = 0;

vi.mock("@xterm/xterm", () => ({
  Terminal: class {
    cols = 80;
    rows = 24;
    options: Record<string, unknown> = {};
    name = "";
    loadAddon() {}
    open(element: HTMLElement) {
      this.name = `t${++opened}`;
      element.dataset.term = this.name;
    }
    write() {}
    onData() {
      return { dispose() {} };
    }
    onResize() {
      return { dispose() {} };
    }
    focus() {
      focused.push(this.name);
    }
    dispose() {}
  },
}));
vi.mock("@xterm/addon-fit", () => ({ FitAddon: class { fit() {} } }));

beforeAll(() => {
  globalThis.ResizeObserver ??= class {
    observe() {}
    disconnect() {}
    unobserve() {}
  } as unknown as typeof ResizeObserver;
});

beforeEach(() => {
  focused.length = 0;
  opened = 0;
});

function OpenTwo() {
  const { open } = useTerminals();
  useEffect(() => {
    open({ kind: "shell", path: "/code/a" }, "Uno");
    open({ kind: "shell", path: "/code/b" }, "Dos");
  }, [open]);
  return null;
}

function setup() {
  const user = userEvent.setup();
  render(
    <ApiProvider api={new MockApi()}>
      <QueryClientProvider client={new QueryClient()}>
        <I18nProvider setting="es">
          <TerminalProvider>
            <OpenTwo />
            <TerminalPanel />
          </TerminalProvider>
        </I18nProvider>
      </QueryClientProvider>
    </ApiProvider>,
  );
  return user;
}

describe("terminal focus", () => {
  it("takes the keyboard when opened or clicked, never while the arrow keys move between tabs", async () => {
    const user = setup();
    // The newest terminal ("Dos", the second xterm) took the keyboard when it opened.
    await waitFor(() => expect(focused.at(-1)).toBe("t2"));

    const one = screen.getByRole("tab", { name: "Uno" });
    const two = screen.getByRole("tab", { name: "Dos" });
    two.focus();
    focused.length = 0;
    await user.keyboard("{ArrowLeft}");
    expect(one).toHaveAttribute("aria-selected", "true");
    expect(one).toHaveFocus();
    await user.keyboard("{ArrowRight}");
    expect(two).toHaveAttribute("aria-selected", "true");
    expect(two).toHaveFocus();
    expect(focused).toEqual([]);

    await user.click(one);
    expect(one).toHaveAttribute("aria-selected", "true");
    expect(focused).toEqual(["t1"]);
  });
});
