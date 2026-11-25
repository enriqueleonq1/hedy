import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../App";
import { MockApi } from "../api/MockApi";

function renderApp() {
  const api = MockApi.withSampleData();
  api.delayMs = 0;
  return render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
}

/** Pretends the window is narrow (or not) for `(max-width: 1100px)`. */
function mockWidth(narrow: boolean) {
  vi.stubGlobal(
    "matchMedia",
    (query: string) =>
      ({
        matches: query.includes("max-width: 1100px") ? narrow : false,
        media: query,
        addEventListener: () => {},
        removeEventListener: () => {},
      }) as unknown as MediaQueryList,
  );
}

describe("Sidebar rail", () => {
  afterEach(() => {
    localStorage.clear();
    vi.unstubAllGlobals();
  });

  it("folds into an icon rail and remembers it", async () => {
    const user = userEvent.setup();
    const { unmount } = renderApp();
    await screen.findByRole("heading", { name: /¡Hola, Alex!/ });
    const sidebar = screen.getByRole("complementary", { name: "Navegación" });
    const toggle = within(sidebar).getByRole("button", { name: "Contraer la barra lateral" });
    expect(toggle).toHaveAttribute("aria-expanded", "true");

    await user.click(toggle);
    expect(sidebar).toHaveClass("sidebar--rail");
    expect(within(sidebar).getByRole("button", { name: "Expandir la barra lateral" })).toHaveAttribute("aria-expanded", "false");
    // Names are not on screen, but every control keeps one.
    expect(within(sidebar).getByRole("button", { name: "hedy" })).toHaveAttribute("title", "hedy");
    expect(within(sidebar).getByRole("button", { name: "Ajustes" })).toHaveAttribute("title", "Ajustes");
    expect(within(sidebar).getByRole("button", { name: "Memoria" })).toHaveAttribute("title", "Memoria");
    expect(within(sidebar).queryByRole("button", { name: "Habilidades" })).not.toBeInTheDocument();
    expect(within(sidebar).getByRole("button", { name: "Terminal" })).toBeInTheDocument();
    expect(within(sidebar).getByRole("button", { name: "Pregunta rápida" })).toBeInTheDocument();
    expect(within(sidebar).getByRole("button", { name: /^Inicio/ })).toHaveAttribute("title", "Inicio");
    // The account in use keeps its figure; the meter and the "+" of each project wait for the full sidebar.
    const account = within(sidebar).getByRole("button", { name: "Claude personal: Sesión 5 h · 58%, Semana · 83%. Abrir la cuenta" });
    expect(within(account).queryByRole("meter")).not.toBeInTheDocument();
    expect(within(sidebar).queryByRole("button", { name: "Nueva tarea en hedy" })).not.toBeInTheDocument();
    expect(localStorage.getItem("hedy.sidebar")).toBe("rail");

    unmount();
    renderApp();
    await screen.findByRole("heading", { name: /¡Hola, Alex!/ });
    expect(screen.getByRole("complementary", { name: "Navegación" })).toHaveClass("sidebar--rail");
  });

  it("starts as a rail on a narrow window unless the user chose otherwise", async () => {
    mockWidth(true);
    const user = userEvent.setup();
    const { unmount } = renderApp();
    await screen.findByRole("heading", { name: /¡Hola, Alex!/ });
    const sidebar = screen.getByRole("complementary", { name: "Navegación" });
    expect(sidebar).toHaveClass("sidebar--rail");
    await user.click(within(sidebar).getByRole("button", { name: "Expandir la barra lateral" }));
    expect(sidebar).not.toHaveClass("sidebar--rail");
    expect(localStorage.getItem("hedy.sidebar")).toBe("full");

    unmount();
    renderApp();
    await screen.findByRole("heading", { name: /¡Hola, Alex!/ });
    expect(screen.getByRole("complementary", { name: "Navegación" })).not.toHaveClass("sidebar--rail");
  });

  it("keeps working when storage is blocked", async () => {
    const user = userEvent.setup();
    const setItem = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    renderApp();
    await screen.findByRole("heading", { name: /¡Hola, Alex!/ });
    const sidebar = screen.getByRole("complementary", { name: "Navegación" });
    await user.click(within(sidebar).getByRole("button", { name: "Contraer la barra lateral" }));
    expect(sidebar).toHaveClass("sidebar--rail");
    setItem.mockRestore();
  });
});
