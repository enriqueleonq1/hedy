import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { CodeCopy, CopyInfo } from "../../api/codeTypes";
import { renderCode, sampleApi } from "../testing";
import { Explorer } from "./Explorer";
import { draftProblem, nameProblem } from "./fileOps";

const HEDY: CodeCopy = { kind: "folder", project: "/code/hedy" };

async function setup(patch: Partial<CopyInfo> = {}) {
  const api = sampleApi();
  const info = { ...(await api.codeCopy(HEDY)), ...patch };
  const onOpen = vi.fn();
  const onCreated = vi.fn();
  const onMoved = vi.fn();
  const view = renderCode(
    <Explorer
      copy={HEDY}
      info={info}
      openPath={null}
      onOpen={onOpen}
      onShowDiff={vi.fn()}
      onHistory={vi.fn()}
      onCreated={onCreated}
      onMoved={onMoved}
    />,
    api,
  );
  const tree = await screen.findByRole("tree", { name: "Archivos" });
  return { ...view, tree, onOpen, onCreated, onMoved, user: userEvent.setup() };
}

const row = (tree: HTMLElement, name: RegExp) => within(tree).getByRole("treeitem", { name });

describe("Explorer file operations", () => {
  it("creates a file inline in the folder last clicked, checking the name as it is typed", async () => {
    const { api, tree, onCreated, user } = await setup();
    await user.click(row(tree, /^facturas/));
    await user.click(screen.getByRole("button", { name: "Nuevo archivo" }));
    const field = await screen.findByRole("textbox", { name: "Nombre del nuevo archivo" });
    await user.type(field, "impuestos.py");
    expect(await screen.findByText("Ya hay algo llamado «impuestos.py» aquí")).toBeInTheDocument();
    await user.clear(field);
    await user.type(field, "a?b");
    expect(screen.getByRole("alert")).toHaveTextContent(/Nombre no válido/);
    await user.clear(field);
    await user.type(field, "descuentos.py{Enter}");
    await waitFor(() => expect(onCreated).toHaveBeenCalledWith("facturas/descuentos.py"));
    const created = await api.readFile(HEDY, "facturas/descuentos.py");
    expect(created).toMatchObject({ kind: "text", text: "" });
    await waitFor(() => expect(row(tree, /^descuentos\.py/)).toBeInTheDocument());
  });

  it("creates a folder (shown while empty) and cancels a name with Escape", async () => {
    const { tree, user } = await setup();
    await user.click(screen.getByRole("button", { name: "Nueva carpeta" }));
    const field = await screen.findByRole("textbox", { name: "Nombre de la nueva carpeta" });
    await user.type(field, "{Escape}");
    expect(screen.queryByRole("textbox", { name: "Nombre de la nueva carpeta" })).toBeNull();

    await user.click(screen.getByRole("button", { name: "Nueva carpeta" }));
    await user.type(await screen.findByRole("textbox", { name: "Nombre de la nueva carpeta" }), "plantillas{Enter}");
    await waitFor(() => expect(row(tree, /^plantillas/)).toBeInTheDocument());
  });

  it("renames with F2 and deletes with the Delete key, after a confirmation that names the file", async () => {
    const { api, tree, onMoved, user } = await setup();
    await user.click(row(tree, /^README\.md/));
    fireEvent.keyDown(tree, { key: "F2" });
    const field = await screen.findByRole("textbox", { name: "Nuevo nombre de README.md" });
    expect(field).toHaveValue("README.md");
    await user.clear(field);
    await user.type(field, "LEEME.md{Enter}");
    await waitFor(() => expect(onMoved).toHaveBeenCalledWith("README.md", "LEEME.md"));
    expect(await api.readFile(HEDY, "README.md")).toEqual({ kind: "missing" });
    expect((await api.readFile(HEDY, "LEEME.md")).kind).toBe("text");

    await waitFor(() => expect(row(tree, /^LEEME\.md/)).toBeInTheDocument());
    await user.click(row(tree, /^LEEME\.md/));
    fireEvent.keyDown(tree, { key: "Delete" });
    const dialog = await screen.findByRole("dialog", { name: "¿Eliminar LEEME.md?" });
    expect(dialog).toHaveTextContent("papelera del sistema");
    await user.click(within(dialog).getByRole("button", { name: "Mover a la papelera" }));
    await waitFor(() => expect(onMoved).toHaveBeenCalledWith("LEEME.md", null));
    expect(api.code.trashed).toEqual(["/code/hedy/LEEME.md"]);
    expect(await screen.findByText("LEEME.md se movió a la papelera")).toBeInTheDocument();
  });

  it("duplicates from the context menu and keeps protected files out of it", async () => {
    const { api, tree, onCreated, user } = await setup();
    fireEvent.contextMenu(row(tree, /^pyproject\.toml/), { clientX: 10, clientY: 10 });
    await user.click(screen.getByRole("menuitem", { name: "Duplicar" }));
    await waitFor(() => expect(onCreated).toHaveBeenCalledWith("pyproject copia.toml"));
    expect((await api.readFile(HEDY, "pyproject copia.toml")).kind).toBe("text");

    fireEvent.contextMenu(row(tree, /^\.env\.example/), { clientX: 10, clientY: 10 });
    expect(screen.getByRole("menuitem", { name: /Renombrar/ })).toBeDisabled();
    expect(screen.getByRole("menuitem", { name: /Eliminar/ })).toBeDisabled();
  });

  it("says why nothing can change in a snapshot or while the agent works", async () => {
    const { user } = await setup({ live: false });
    const button = screen.getByRole("button", { name: "Nuevo archivo" });
    expect(button).toHaveAttribute("aria-disabled", "true");
    expect(button).toHaveAttribute("title", expect.stringMatching(/^Solo lectura: Instantánea de solo lectura/));
    await user.click(button);
    expect(screen.queryByRole("textbox", { name: "Nombre del nuevo archivo" })).toBeNull();
    expect(await screen.findByText(/^Solo lectura: Instantánea/)).toBeInTheDocument();
  });

  it("checks names like the backend", () => {
    const taken = (n: string) => n === "app.py";
    expect(nameProblem("", taken)).toBe("edit.name.empty");
    expect(nameProblem("app.py", taken)).toBe("edit.name.exists");
    expect(nameProblem("x.", taken)).toBe("edit.name.invalid");
    expect(nameProblem(".env", taken)).toBe("edit.name.protected");
    expect(nameProblem("ok.py", taken)).toBeNull();
    expect(draftProblem("api/v2/x.py", "src", () => false)).toBeNull();
    expect(draftProblem("api/x?.py", "src", () => false)).toBe("edit.name.invalid");
    expect(draftProblem("app.py", "", taken)).toBe("edit.name.exists");
  });
});
