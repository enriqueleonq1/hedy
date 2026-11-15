import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { ApiProvider } from "../api/context";
import { MockApi } from "../api/MockApi";
import { Markdown } from "./Markdown";

function renderMd(text: string, api = new MockApi()) {
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiProvider api={api}>
        <Markdown text={text} />
      </ApiProvider>
    </QueryClientProvider>,
  );
  return api;
}

const ANSWER = `**Napoleón Bonaparte** (1769–1821) fue un militar.

### Datos clave
- **Nacimiento:** 1769
- Muerte: 1821

| Año | Hecho |
| --- | --- |
| 1804 | Emperador |

\`Code\` y un [enlace](https://es.wikipedia.org/wiki/Napoleon).
Segunda línea.

<script>alert(1)</script>
![retrato](https://example.com/n.png)`;

describe("Markdown", () => {
  it("renders an agent's Markdown instead of showing the marks", () => {
    renderMd(ANSWER);
    expect(screen.getByText("Napoleón Bonaparte").tagName).toBe("STRONG");
    expect(screen.getByRole("heading", { name: "Datos clave" })).toBeInTheDocument();
    expect(screen.getAllByRole("listitem")).toHaveLength(2);
    expect(screen.getByRole("table")).toBeInTheDocument();
    expect(screen.getByText("Code").tagName).toBe("CODE");
    expect(document.body.textContent).not.toContain("**");
    expect(document.body.textContent).not.toContain("###");
    // A single line break stays a line break.
    expect(document.querySelector("br")).not.toBeNull();
  });

  it("never renders raw HTML or loads remote images", () => {
    renderMd(ANSWER);
    expect(document.querySelector("script")).toBeNull();
    expect(document.querySelector("img")).toBeNull();
    expect(screen.getByText("[Imagen: retrato]")).toBeInTheDocument();
  });

  it("opens https links in the browser through Hedy, not in the app", async () => {
    const api = new MockApi();
    const open = vi.spyOn(api, "openLink");
    renderMd("[docs](https://tauri.app) y [local](file:///etc/passwd)", api);
    await userEvent.setup().click(screen.getByRole("link", { name: "docs" }));
    expect(open).toHaveBeenCalledWith("https://tauri.app");
    expect(screen.queryByRole("link", { name: "local" })).toBeNull();
    expect(screen.getByText("local")).toBeInTheDocument();
  });
});
