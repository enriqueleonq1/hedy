import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../../App";
import { MockApi } from "../../../api/MockApi";
import { withSubject } from "./CommitMessage";
import { firstStep } from "./ShipDialog";

vi.mock("../../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

const SLOW = { timeout: 8000 };
const TASK = "Arregla el cálculo de impuestos en las facturas";
const BRANCH = "hedy/arregla-el-calculo-de-impuestos-s5";

function setup(language: "es" | "en" = "es") {
  const api = MockApi.withSampleData().withSampleActivity();
  api.delayMs = 0;
  if (language === "en") void api.setSetting("ui.language", "en");
  render(<App api={api} queryClient={new QueryClient({ defaultOptions: { queries: { retry: false } } })} />);
  return { api, user: userEvent.setup() };
}

async function openReviewTask(user: ReturnType<typeof userEvent.setup>, board = /^Actividad: /) {
  await user.click(await screen.findByRole("button", { name: board }, SLOW));
  await user.click(await screen.findByRole("button", { name: new RegExp(TASK) }, SLOW));
  await screen.findByRole("list", { name: /Archivos cambiados|Changed files/ }, SLOW);
}

async function openSheet(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole("button", { name: "Aceptar cambios" }));
  const sheet = await screen.findByRole("dialog", { name: "Revisar antes de integrar" }, SLOW);
  await within(sheet).findByLabelText("Mensaje del commit");
  return sheet;
}

describe("Ship: commit, push and pull request", () => {
  afterEach(() => {
    history.replaceState(null, "", "#");
    localStorage.clear();
  });

  it("keeps the work on its branch, publishes it with -u and opens a draft PR", async () => {
    const { api, user } = setup();
    const openLink = vi.spyOn(api, "openLink");
    await openReviewTask(user);
    const sheet = await openSheet(user);

    await user.click(within(sheet).getByRole("radio", { name: "Dejar en su rama (PR)" }));
    expect(within(sheet).getByText(/tu carpeta no cambia/)).toBeInTheDocument();
    expect(within(sheet).queryByText(/Tu carpeta principal está en la rama/)).toBeNull();
    await user.click(within(sheet).getByRole("button", { name: "Hacer commit en la rama" }));
    expect(await screen.findByText(`Commit hecho en ${BRANCH}.`, undefined, SLOW)).toBeInTheDocument();
    expect(api.acceptModes.get("s5")).toBe("branch");

    // The publish step follows by itself.
    const publish = await screen.findByRole("dialog", { name: "Publicar" }, SLOW);
    expect(within(publish).getByText(/se crea origin\/hedy\/arregla.*git push -u/)).toBeInTheDocument();
    expect(within(publish).queryByText(/ya no puede deshacer/)).toBeNull();
    await user.click(within(publish).getByRole("button", { name: "Publicar" }));

    const prDialog = await screen.findByRole("dialog", { name: "Crear pull request" }, SLOW);
    const title = await within(prDialog).findByLabelText("Título");
    expect(title).toHaveValue(TASK);
    expect(within(prDialog).getByLabelText("Rama base")).toHaveValue("main");
    expect(within(prDialog).getByText(`Desde ${BRANCH}`)).toBeInTheDocument();
    const body = within(prDialog).getByLabelText("Descripción") as HTMLTextAreaElement;
    expect(body.value).toContain("## Qué se pidió");
    expect(body.value).toContain("`facturas/impuestos.py`");
    expect(body.value).toContain("## Tests");
    await user.clear(title);
    await user.type(title, "fix(facturas): IVA por línea");
    await user.click(within(prDialog).getByRole("switch", { name: "Como borrador" }));
    await user.click(within(prDialog).getByRole("button", { name: "Crear PR" }));

    const done = await screen.findByRole("dialog", { name: "Listo" }, SLOW);
    expect(api.pullRequests[0]?.input).toMatchObject({ title: "fix(facturas): IVA por línea", base: "main", draft: true });
    expect(within(done).getByText(`Publicado en origin/${BRANCH}.`)).toBeInTheDocument();
    await user.click(within(done).getByRole("button", { name: /Abrir PR #42/ }));
    expect(openLink).toHaveBeenCalledWith("https://github.com/enrique/hedy/pull/42");
    await user.click(within(done).getByRole("button", { name: "Listo" }));
    expect(screen.queryByRole("dialog")).toBeNull();

    // The accepted view keeps the link and offers to reopen the task.
    expect(screen.getByText(/sus cambios tienen commit en su rama/)).toBeInTheDocument();
    expect(await screen.findByRole("button", { name: /PR #42 · borrador/ })).toBeInTheDocument();
    expect(screen.getByText(`Publicada en origin/${BRANCH}`)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Reabrir la tarea" }));
    expect(await screen.findByText("Tarea reabierta en su rama.", undefined, SLOW)).toBeInTheDocument();
    expect(await screen.findByRole("list", { name: "Archivos cambiados" })).toBeInTheDocument();
  });

  it("suggests the subject with the task's AI and shows why it could not", async () => {
    const { api, user } = setup();
    await openReviewTask(user);
    const sheet = await openSheet(user);
    const suggest = within(sheet).getByRole("button", { name: "Sugerir con IA" });
    expect(suggest).toHaveAttribute("title", expect.stringContaining("Gasta algunos tokens"));
    await user.click(suggest);
    const message = within(sheet).getByLabelText("Mensaje del commit");
    await waitFor(() =>
      expect(message).toHaveValue("fix(facturas): round VAT per line, not on the total\n\nEl IVA se redondea por línea y no al total."),
    );

    api.suggestion = { error: "la cuenta «Personal» está en su límite. Hedy no usa otra cuenta: escribe el mensaje tú o espera a que se renueve" };
    await user.click(suggest);
    expect(await within(sheet).findByRole("alert")).toHaveTextContent(/Hedy no usa otra cuenta/);
    // The message stays as it was.
    expect(message).toHaveValue("fix(facturas): round VAT per line, not on the total\n\nEl IVA se redondea por línea y no al total.");

    // Ctrl+Enter accepts from the message.
    message.focus();
    await user.keyboard("{Control>}{Enter}{/Control}");
    expect(await screen.findByText("Cambios integrados en main.", undefined, SLOW)).toBeInTheDocument();
  });

  it("explains a rejected push, offers a fast-forward pull and never forces", async () => {
    const { api, user } = setup();
    await openReviewTask(user);
    const sheet = await openSheet(user);
    await user.click(within(sheet).getByRole("button", { name: "Aceptar e integrar" }));
    const publish = await screen.findByRole("dialog", { name: "Publicar" }, SLOW);
    expect(within(publish).getByText("Se publica main en origin/main.")).toBeInTheDocument();
    expect(within(publish).getByText(/Hedy ya no puede deshacer la integración/)).toBeInTheDocument();

    api.pushFailure = {
      kind: "rejected",
      message: "el remoto tiene commits que «main» no tiene. Trae los cambios (Pull) y vuelve a publicar; Hedy nunca fuerza un push",
    };
    await user.click(within(publish).getByRole("button", { name: "Publicar" }));
    const failure = await within(publish).findByRole("alert");
    expect(failure).toHaveTextContent(/nunca fuerza un push/);
    await user.click(within(failure).getByRole("button", { name: "Traer cambios" }));
    await waitFor(() => expect(api.shipLog).toContain("pull /code/hedy"));
    await waitFor(() => expect(within(publish).queryByRole("alert")).toBeNull());

    await user.click(within(publish).getByRole("button", { name: "Publicar" }));
    // main is the default branch: no PR to open, the flow ends.
    const done = await screen.findByRole("dialog", { name: "Listo" }, SLOW);
    expect(within(done).getByText("Publicado en origin/main.")).toBeInTheDocument();
    expect(api.shipLog.filter((l) => l === "push task s5")).toHaveLength(2);
  });

  it("without a remote it merges as before and offers nothing to publish", async () => {
    const { api, user } = setup();
    api.noRemote.add("/code/hedy");
    await openReviewTask(user);
    const sheet = await openSheet(user);
    expect(within(sheet).queryByRole("radiogroup", { name: "Cómo integrar" })).toBeNull();
    await user.click(within(sheet).getByRole("button", { name: "Aceptar e integrar" }));
    expect(await screen.findByText("Cambios integrados en main.", undefined, SLOW)).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(await screen.findByText(/no tiene remoto/, undefined, SLOW)).toBeInTheDocument();
  });

  it("asks to log in to gh before offering PRs", async () => {
    const { api, user } = setup();
    api.gh = "loggedOut";
    await openReviewTask(user);
    const sheet = await openSheet(user);
    await user.click(within(sheet).getByRole("radio", { name: "Dejar en su rama (PR)" }));
    await user.click(within(sheet).getByRole("button", { name: "Hacer commit en la rama" }));
    const publish = await screen.findByRole("dialog", { name: "Publicar" }, SLOW);
    await user.click(within(publish).getByRole("button", { name: "Publicar" }));
    const done = await screen.findByRole("dialog", { name: "Listo" }, SLOW);
    await user.click(within(done).getByRole("button", { name: "Listo" }));
    expect(await screen.findByText(/inicia sesión con gh auth login/, undefined, SLOW)).toBeInTheDocument();
  });
});

describe("Folder status in the status bar", () => {
  afterEach(() => {
    history.replaceState(null, "", "#");
  });

  it("shows the branch and counters once, in the status bar, and pulls or pushes on click", async () => {
    const { api, user } = setup();
    await openReviewTask(user);
    const folder = await screen.findByRole("group", { name: "Tu carpeta" });
    expect(screen.getAllByRole("group", { name: "Tu carpeta" })).toHaveLength(1);
    expect(folder.closest(".statusbar")).not.toBeNull();
    expect(within(folder).getByRole("button", { name: "Tu carpeta: rama main" })).toBeInTheDocument();
    // The changes item next to it counts the copy on screen: here, the task's files.
    expect(within(folder).queryByText(/Cambios sin commit/)).toBeNull();
    const bar = screen.getByRole("contentinfo", { name: "Barra de estado" });
    expect(within(bar).getByRole("button", { name: /^\d+ archivos? cambiados? por la tarea\. Revisar/ })).toBeInTheDocument();

    await user.click(within(folder).getByRole("button", { name: /Commits por traer: 2/ }));
    expect(await screen.findByText("Cambios traídos a main.", undefined, SLOW)).toBeInTheDocument();
    expect(api.shipLog).toEqual(["pull /code/hedy"]);
    await waitFor(() => expect(within(folder).queryByRole("button", { name: /Commits por traer/ })).toBeNull());
  });

  it("has a menu (also on right-click) to pull, push, open VS Code and copy the branch", async () => {
    const { api, user } = setup();
    const openInEditor = vi.spyOn(api, "openInEditor");
    await openReviewTask(user);
    const folder = await screen.findByRole("group", { name: "Tu carpeta" });

    fireEvent.contextMenu(within(folder).getByText("main"));
    const menu = await screen.findByRole("menu", { name: "Tu carpeta: rama main" });
    expect(within(menu).getByRole("menuitem", { name: /Publicar.*En origin\/main; nunca se fuerza/ })).toBeInTheDocument();
    await user.click(within(menu).getByRole("menuitem", { name: /Copiar nombre de rama/ }));
    expect(await screen.findByText("Copiado: main", undefined, SLOW)).toBeInTheDocument();
    await expect(navigator.clipboard.readText()).resolves.toBe("main");

    await user.click(within(folder).getByRole("button", { name: "Tu carpeta: rama main" }));
    await user.click(await screen.findByRole("menuitem", { name: /Abrir en VS Code/ }));
    expect(openInEditor).toHaveBeenCalledWith("/code/hedy");

    // A divergence is explained and points to VS Code; nothing is merged.
    api.pullFailure = {
      kind: "diverged",
      message: "«main» y origin/main tienen commits distintos: no se puede avanzar sin fusionar. Resuélvelo en VS Code (o con git)",
    };
    await user.click(within(folder).getByRole("button", { name: "Tu carpeta: rama main" }));
    await user.click(await screen.findByRole("menuitem", { name: /Traer cambios/ }));
    const toast = (await screen.findByText(/no se puede avanzar sin fusionar/)).closest(".toast") as HTMLElement;
    await user.click(within(toast).getByRole("button", { name: "Abrir en VS Code" }));
    expect(openInEditor).toHaveBeenCalledTimes(2);
  });

  it("works in English", async () => {
    const { user } = setup("en");
    await openReviewTask(user, /^Activity: /);
    const folder = await screen.findByRole("group", { name: "Your folder" });
    expect(within(folder).getByRole("button", { name: /Commits to pull: 2/ })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Accept changes" }));
    const sheet = await screen.findByRole("dialog", { name: "Review before merging" }, SLOW);
    await user.click(await within(sheet).findByRole("radio", { name: "Keep on its branch (PR)" }));
    await user.click(within(sheet).getByRole("button", { name: "Commit on the branch" }));
    const publish = await screen.findByRole("dialog", { name: "Publish" }, SLOW);
    expect(within(publish).getByRole("button", { name: "Not now" })).toBeInTheDocument();
  });
});

describe("ship helpers", () => {
  it("replaces only the subject and picks the first step left", () => {
    expect(withSubject("Old\n\nBody", "feat: new")).toBe("feat: new\n\nBody");
    expect(withSubject("Old", "feat: new")).toBe("feat: new");
    const base = {
      branch: "x",
      kept: true,
      remote: "origin",
      remoteHost: "github.com",
      upstream: null,
      ahead: 1,
      behind: 0,
      pushed: false,
      gh: "ready" as const,
      defaultBranch: "main",
      pullRequest: null,
      canOpenPr: true,
    };
    expect(firstStep(base)).toBe("publish");
    expect(firstStep({ ...base, pushed: true })).toBe("pr");
    expect(firstStep({ ...base, pushed: true, pullRequest: { url: "https://x/pull/1", number: 1, draft: false } })).toBe("done");
    expect(firstStep({ ...base, remote: null, gh: null, canOpenPr: false })).toBe("done");
  });
});
