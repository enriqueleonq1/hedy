import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";
import type { ProjectStatus } from "../../api/types";
import { translator } from "../../i18n";
import { projectSuggestions, starter } from "./Suggestions";

// xterm needs a real canvas; the Terminal tab is not exercised here.
vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

function setup(api = MockApi.withSampleData()) {
  api.delayMs = 0;
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<App api={api} queryClient={queryClient} />);
  return { api, user: userEvent.setup() };
}

const t = translator("es");

const status = (extra: Partial<ProjectStatus>): ProjectStatus => ({
  branch: "main",
  uncommitted: 0,
  lastTests: null,
  agents: [],
  codeMap: false,
  memoryNotes: 0,
  memoryStale: 0,
  memoryProposed: 0,
  ...extra,
});

describe("projectSuggestions", () => {
  const labels = (s: { label: string }[]) => s.map((x) => x.label);

  it("puts failing tests and uncommitted changes first, and always offers three", () => {
    const failing = status({ uncommitted: 2, lastTests: { command: "pytest -q", passed: false, at: 0 } });
    const picked = projectSuggestions(failing, "claude", t);
    expect(labels(picked)).toEqual([
      "Arregla los tests que fallan",
      "Revisa mis cambios sin commit",
      "Explícame cómo está organizado este repo",
    ]);
    expect(picked[0]?.text).toContain("pytest -q");
    expect(picked[1]?.mode).toBe("ask");

    expect(labels(projectSuggestions(status({}), "claude", t))).toEqual([
      "Explícame cómo está organizado este repo",
      "Agrega tests a lo que no tiene",
      "Busca código muerto o duplicado",
    ]);
  });

  it("does not offer a review of uncommitted changes to agents that work in a worktree", () => {
    const labelsFor = (provider: "codex" | "gemini") => labels(projectSuggestions(status({ uncommitted: 5 }), provider, t));
    expect(labelsFor("codex")).not.toContain("Revisa mis cambios sin commit");
    expect(labelsFor("gemini")).toHaveLength(3);
  });

  it("puts the caret where the starter says", () => {
    expect(starter("x", "Explícame {caret} con un ejemplo.")).toEqual({ label: "x", text: "Explícame  con un ejemplo.", caret: 10 });
    expect(starter("x", "Sin marca")).toEqual({ label: "x", text: "Sin marca", caret: 9 });
  });
});

describe("Suggestions in an empty task", () => {
  it("fills a project's composer without sending, and they go once the task has messages", async () => {
    const { api, user } = setup();
    const start = vi.spyOn(api, "startTask");
    await user.click(await screen.findByRole("button", { name: /^hedy/ }));
    const chips = await screen.findByRole("list", { name: "Sugerencias" });
    // The sample status has uncommitted changes and passing tests.
    expect(within(chips).getAllByRole("button").map((b) => b.textContent)).toEqual([
      "Revisa mis cambios sin commit",
      "Explícame cómo está organizado este repo",
      "Agrega tests a lo que no tiene",
    ]);

    await user.click(within(chips).getByRole("button", { name: "Revisa mis cambios sin commit" }));
    const composer = screen.getByLabelText("Pídele algo a hedy");
    expect(composer).toHaveValue("Revisa mis cambios sin commit y propón un mensaje de commit.");
    await waitFor(() => expect(composer).toHaveFocus());
    expect(screen.getByRole("combobox", { name: "Modo" })).toHaveTextContent("Pregunta");
    expect(start).not.toHaveBeenCalled();

    await user.click(screen.getByRole("button", { name: "Enviar" }));
    expect(start).toHaveBeenCalledTimes(1);
    await screen.findByRole("heading", { name: /Revisa mis cambios sin commit/ });
    expect(screen.queryByRole("list", { name: "Sugerencias" })).toBeNull();
  });

  it("offers starters in a quick question with the caret in place", async () => {
    const { user } = setup();
    await user.click(await screen.findByRole("button", { name: "Pregunta rápida" }));
    const chips = await screen.findByRole("list", { name: "Sugerencias" });
    expect(within(chips).getAllByRole("button").map((b) => b.textContent)).toEqual([
      "Explícame un concepto",
      "Compara dos opciones",
      "Ayúdame a redactar",
    ]);
    await user.click(within(chips).getByRole("button", { name: "Explícame un concepto" }));
    const composer = screen.getByRole<HTMLTextAreaElement>("textbox", { name: "¿Qué quieres saber?" });
    expect(composer).toHaveValue("Explícame  con un ejemplo sencillo.");
    await waitFor(() => expect(composer).toHaveFocus());
    expect(composer.selectionStart).toBe(10);
    await user.keyboard("la recursión");
    expect(composer).toHaveValue("Explícame la recursión con un ejemplo sencillo.");
  });
});
