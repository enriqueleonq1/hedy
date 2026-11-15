import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";
import { insertMention, mentionAt, rankFiles } from "./mentions";

// xterm needs a real canvas; the Terminal tab is not exercised here.
vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

function setup(api = MockApi.withSampleData()) {
  api.delayMs = 0;
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<App api={api} queryClient={queryClient} />);
  return { api, user: userEvent.setup() };
}

describe("mentions", () => {
  it("finds the @ word with the caret", () => {
    expect(mentionAt("Revisa @ser", 11)).toEqual({ start: 7, query: "ser" });
    expect(mentionAt("@", 1)).toEqual({ start: 0, query: "" });
    expect(mentionAt("Revisa @ser y más", 11)).toEqual({ start: 7, query: "ser" });
    expect(mentionAt("Revisa @ser y más", 17)).toBeNull();
    expect(mentionAt("escribe a ana@correo.com", 24)).toBeNull();
    expect(mentionAt("hola", 4)).toBeNull();
  });

  it("ranks file names that start with the query first, then names, then paths", () => {
    const files = ["src/servidor/app.py", "docs/observar.md", "servicio.py", "citas/servicio.py", "README.md"];
    expect(rankFiles(files, "serv")).toEqual(["servicio.py", "citas/servicio.py", "docs/observar.md", "src/servidor/app.py"]);
    expect(rankFiles(files, "")).toEqual(files);
    expect(rankFiles(files, "zzz")).toEqual([]);
    expect(rankFiles(files, "S", 2)).toHaveLength(2);
  });

  it("inserts the path with one space after it", () => {
    expect(insertMention("Revisa @ser", 7, 11, "citas/servicio.py")).toEqual({
      text: "Revisa @citas/servicio.py ",
      caret: 26,
    });
    expect(insertMention("Mira @ser y dime", 5, 9, "a.py")).toEqual({ text: "Mira @a.py y dime", caret: 11 });
  });
});

describe("@file mentions in the composer", () => {
  it("lists the project's files after @ and inserts the one picked", async () => {
    const { api, user } = setup();
    const files = vi.spyOn(api, "projectFiles");
    await user.click(await screen.findByRole("button", { name: /^hedy/ }));
    const composer = await screen.findByLabelText("Pídele algo a hedy");

    await user.type(composer, "Revisa @recor");
    const list = await screen.findByRole("listbox", { name: "Archivos del proyecto" });
    expect(files).toHaveBeenCalledWith("/code/hedy");
    const options = within(list).getAllByRole("option");
    expect(options.map((o) => o.getAttribute("aria-label"))).toEqual([
      "notificaciones/recordatorios.py",
      "tests/test_recordatorios.py",
    ]);
    expect(options[0]).toHaveAttribute("aria-selected", "true");
    expect(composer).toHaveAttribute("aria-activedescendant", options[0]!.id);

    await user.keyboard("{ArrowDown}{Enter}");
    expect(composer).toHaveValue("Revisa @tests/test_recordatorios.py ");
    expect(screen.queryByRole("listbox", { name: "Archivos del proyecto" })).toBeNull();

    // Esc closes it; the text stays.
    await user.type(composer, "y @");
    expect(await screen.findByRole("listbox", { name: "Archivos del proyecto" })).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("listbox", { name: "Archivos del proyecto" })).toBeNull();
    expect(composer).toHaveValue("Revisa @tests/test_recordatorios.py y @");
  });

  it("says when no file matches, and @ is plain text in a quick question", async () => {
    const { api, user } = setup();
    await user.click(await screen.findByRole("button", { name: /^hedy/ }));
    const composer = await screen.findByLabelText("Pídele algo a hedy");
    await user.type(composer, "@zzz");
    expect(await screen.findByText("Ningún archivo coincide")).toBeInTheDocument();

    const files = vi.spyOn(api, "projectFiles");
    await user.click(screen.getByRole("button", { name: "Pregunta rápida" }));
    const question = await screen.findByRole("textbox", { name: "¿Qué quieres saber?" });
    await user.type(question, "¿Qué significa @deprecated?");
    await waitFor(() => expect(question).toHaveValue("¿Qué significa @deprecated?"));
    expect(screen.queryByRole("listbox", { name: "Archivos del proyecto" })).toBeNull();
    expect(files).not.toHaveBeenCalled();
  });
});
