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

async function openReviewTask(user: ReturnType<typeof userEvent.setup>, board = /^Inicio/) {
  await user.click(await screen.findByRole("button", { name: board }, SLOW));
  await user.click(await screen.findByRole("button", { name: new RegExp(TASK) }, SLOW));
  await screen.findByRole("list", { name: /Archivos cambiados|Changed files/ }, SLOW);
}

async function openSheet(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole("button", { name: "Aceptar cambios…" }));
  const sheet = await screen.findByRole("dialog", { name: "Aceptar cambios" }, SLOW);
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

    expect(within(sheet).getByRole("radio", { name: "Unir a main ahora" })).toBeChecked();
    await user.click(within(sheet).getByRole("radio", { name: "Guardar en su rama" }));
    expect(within(sheet).getByText(`Commit en ${BRANCH}; tu carpeta no cambia.`)).toBeInTheDocument();
    expect(within(sheet).queryByText(/Tu carpeta principal está en la rama/)).toBeNull();
    // "Publicar y abrir un PR" starts off: the publish step comes after the commit.
    expect(within(sheet).getByRole("switch", { name: "Publicar y abrir un PR" })).toHaveAttribute("aria-checked", "false");
    expect(within(sheet).queryByLabelText("Título")).toBeNull();
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
    expect(openLink).toHaveBeenCalledWith("https://github.com/alex/hedy/pull/42");
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

  it("saves on its branch, publishes and opens the PR in one go from the sheet", async () => {
    const { api, user } = setup();
    const accept = vi.spyOn(api, "acceptTask");
    const push = vi.spyOn(api, "pushTask");
    const create = vi.spyOn(api, "createPullRequest");
    const openLink = vi.spyOn(api, "openLink");
    await openReviewTask(user);
    const sheet = await openSheet(user);
    // Merging needs no PR fields.
    expect(within(sheet).queryByRole("switch", { name: "Publicar y abrir un PR" })).toBeNull();

    await user.click(within(sheet).getByRole("radio", { name: "Guardar en su rama" }));
    // Publishing is chosen, never preset.
    const publishPr = within(sheet).getByRole("switch", { name: "Publicar y abrir un PR" });
    expect(publishPr).toHaveAttribute("aria-checked", "false");
    expect(within(sheet).queryByLabelText("Título")).toBeNull();
    expect(within(sheet).getByRole("button", { name: "Hacer commit en la rama" })).toBeInTheDocument();
    await user.click(publishPr);
    expect(publishPr).toHaveAttribute("aria-checked", "true");
    const title = within(sheet).getByLabelText("Título");
    expect(title).toHaveValue("");
    expect(title).toHaveAttribute("placeholder", TASK);
    const body = within(sheet).getByLabelText("Descripción");
    expect(body).toHaveAttribute("placeholder", expect.stringMatching(/^Vacía: Hedy la escribe/));
    const draft = within(sheet).getByRole("switch", { name: "Como borrador" });
    expect(draft).toHaveAttribute("aria-checked", "false");

    await user.type(title, "fix(facturas): IVA por línea");
    await user.type(body, "Redondeo por línea.");
    await user.click(draft);
    await user.click(within(sheet).getByRole("button", { name: "Guardar y abrir PR" }));

    const toast = (await screen.findByText(`PR abierto desde ${BRANCH}.`, undefined, SLOW)).closest(".toast") as HTMLElement;
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(accept).toHaveBeenCalledWith("s5", expect.any(String), "branch");
    expect(push).toHaveBeenCalledWith("s5");
    expect(create).toHaveBeenCalledWith("s5", { title: "fix(facturas): IVA por línea", body: "Redondeo por línea.", base: "", draft: true });
    const order = (spy: { mock: { invocationCallOrder: number[] } }) => spy.mock.invocationCallOrder[0] ?? 0;
    expect(order(accept)).toBeLessThan(order(push));
    expect(order(push)).toBeLessThan(order(create));
    expect(api.acceptModes.get("s5")).toBe("branch");

    await user.click(within(toast).getByRole("button", { name: "Ver el PR" }));
    expect(openLink).toHaveBeenCalledWith("https://github.com/alex/hedy/pull/42");
    // Done: no publish step opens by itself.
    expect(screen.queryByRole("dialog", { name: "Publicar" })).toBeNull();
    expect(await screen.findByRole("button", { name: /PR #42 · borrador/ }, SLOW)).toBeInTheDocument();
  });

  it("never publishes on Enter in the PR title, and accepts once on a double Ctrl+Enter", async () => {
    const { api, user } = setup();
    // Accepting waits until the test lets it go.
    let finish = () => {};
    const gate = new Promise<void>((resolve) => (finish = resolve));
    const original = api.acceptTask.bind(api);
    const accept = vi.spyOn(api, "acceptTask").mockImplementation(async (...args) => {
      await gate;
      return original(...args);
    });
    await openReviewTask(user);
    const sheet = await openSheet(user);
    await user.click(within(sheet).getByRole("radio", { name: "Guardar en su rama" }));
    await user.click(within(sheet).getByRole("switch", { name: "Publicar y abrir un PR" }));
    const title = within(sheet).getByLabelText("Título");
    await user.type(title, "fix(facturas): IVA{Enter}");
    expect(title).toHaveValue("fix(facturas): IVA");
    expect(accept).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog", { name: "Aceptar cambios" })).toBeInTheDocument();

    // Ctrl+Enter accepts; a second one while it runs is ignored.
    const message = within(sheet).getByLabelText("Mensaje del commit");
    message.focus();
    await user.keyboard("{Control>}{Enter}{Enter}{/Control}");
    await waitFor(() => expect(accept).toHaveBeenCalledTimes(1));
    expect(accept).toHaveBeenCalledWith("s5", expect.any(String), "branch");
    fireEvent.keyDown(message, { key: "Enter", ctrlKey: true });
    expect(accept).toHaveBeenCalledTimes(1);
    finish();
    expect(await screen.findByText(`PR abierto desde ${BRANCH}.`, undefined, SLOW)).toBeInTheDocument();
    expect(accept).toHaveBeenCalledTimes(1);
  });

  it("remembers how the user accepts in each project", async () => {
    const { user } = setup();
    await openReviewTask(user);
    let sheet = await openSheet(user);
    expect(within(sheet).getByRole("radio", { name: "Unir a main ahora" })).toBeChecked();
    await user.click(within(sheet).getByRole("radio", { name: "Guardar en su rama" }));
    expect(localStorage.getItem("hedy.acceptMode./code/hedy")).toBe("branch");
    await user.click(within(sheet).getByRole("button", { name: "Cancelar" }));
    expect(screen.queryByRole("dialog", { name: "Aceptar cambios" })).toBeNull();

    sheet = await openSheet(user);
    expect(within(sheet).getByRole("radio", { name: "Guardar en su rama" })).toBeChecked();
    expect(within(sheet).getByRole("button", { name: "Hacer commit en la rama" })).toBeInTheDocument();
    await user.click(within(sheet).getByRole("radio", { name: "Unir a main ahora" }));
    expect(localStorage.getItem("hedy.acceptMode./code/hedy")).toBe("merge");
  });

  it("remembers in each project whether to publish and open a PR", async () => {
    // Chosen in another project: it does not carry over to this one.
    localStorage.setItem("hedy.acceptPr./code/tienda", "on");
    const { user } = setup();
    await openReviewTask(user);
    let sheet = await openSheet(user);
    await user.click(within(sheet).getByRole("radio", { name: "Guardar en su rama" }));
    let publishPr = within(sheet).getByRole("switch", { name: "Publicar y abrir un PR" });
    expect(publishPr).toHaveAttribute("aria-checked", "false");
    await user.click(publishPr);
    expect(localStorage.getItem("hedy.acceptPr./code/hedy")).toBe("on");
    await user.click(within(sheet).getByRole("button", { name: "Cancelar" }));
    expect(screen.queryByRole("dialog", { name: "Aceptar cambios" })).toBeNull();

    // Reopened: still on, with the PR fields and button.
    sheet = await openSheet(user);
    publishPr = within(sheet).getByRole("switch", { name: "Publicar y abrir un PR" });
    expect(publishPr).toHaveAttribute("aria-checked", "true");
    expect(within(sheet).getByLabelText("Título")).toBeInTheDocument();
    expect(within(sheet).getByLabelText("Descripción")).toBeInTheDocument();
    expect(within(sheet).getByRole("switch", { name: "Como borrador" })).toBeInTheDocument();
    expect(within(sheet).getByRole("button", { name: "Guardar y abrir PR" })).toBeInTheDocument();

    // Turned off, it stays off.
    await user.click(publishPr);
    expect(localStorage.getItem("hedy.acceptPr./code/hedy")).toBe("off");
    expect(localStorage.getItem("hedy.acceptPr./code/tienda")).toBe("on");
    await user.click(within(sheet).getByRole("button", { name: "Cancelar" }));
    sheet = await openSheet(user);
    expect(within(sheet).getByRole("switch", { name: "Publicar y abrir un PR" })).toHaveAttribute("aria-checked", "false");
    expect(within(sheet).getByRole("button", { name: "Hacer commit en la rama" })).toBeInTheDocument();
  });

  it("leaves the branch committed when the push fails, and opens no PR", async () => {
    const { api, user } = setup();
    const create = vi.spyOn(api, "createPullRequest");
    api.pushFailure = {
      kind: "rejected",
      message: "el remoto tiene commits que la rama no tiene. Trae los cambios (Pull) y vuelve a publicar; Hedy nunca fuerza un push",
    };
    await openReviewTask(user);
    const sheet = await openSheet(user);
    await user.click(within(sheet).getByRole("radio", { name: "Guardar en su rama" }));
    await user.click(within(sheet).getByRole("switch", { name: "Publicar y abrir un PR" }));
    await user.click(within(sheet).getByRole("button", { name: "Guardar y abrir PR" }));

    expect(await screen.findByText(/Hedy nunca fuerza un push/, undefined, SLOW)).toBeInTheDocument();
    expect(api.acceptModes.get("s5")).toBe("branch");
    expect(api.shipLog).toContain("push task s5");
    expect(create).not.toHaveBeenCalled();
    expect(api.pullRequests).toHaveLength(0);
    expect(screen.queryByText(/PR abierto desde/)).toBeNull();
    // No publish dialog by itself: the side panel offers each step again.
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("keeps the commit when the PR cannot be created", async () => {
    const { api, user } = setup();
    api.prFailure = "gh: no se pudo crear el PR";
    await openReviewTask(user);
    const sheet = await openSheet(user);
    await user.click(within(sheet).getByRole("radio", { name: "Guardar en su rama" }));
    await user.click(within(sheet).getByRole("switch", { name: "Publicar y abrir un PR" }));
    await user.click(within(sheet).getByRole("button", { name: "Guardar y abrir PR" }));

    expect(await screen.findByText(/no se pudo crear el PR/, undefined, SLOW)).toBeInTheDocument();
    expect(api.acceptModes.get("s5")).toBe("branch");
    expect(api.shipLog).toContain("push task s5");
    expect(api.pullRequests).toHaveLength(0);
    expect(screen.queryByText(/PR abierto desde/)).toBeNull();
    expect(screen.queryByRole("dialog")).toBeNull();
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
    await user.click(within(sheet).getByRole("button", { name: "Aceptar y unir" }));
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
    expect(within(sheet).queryByRole("radiogroup", { name: "Cómo aceptar" })).toBeNull();
    await user.click(within(sheet).getByRole("button", { name: "Aceptar y unir" }));
    expect(await screen.findByText("Cambios integrados en main.", undefined, SLOW)).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(await screen.findByText(/no tiene remoto/, undefined, SLOW)).toBeInTheDocument();
  });

  it("asks to log in to gh before offering PRs", async () => {
    const { api, user } = setup();
    api.gh = "loggedOut";
    await openReviewTask(user);
    const sheet = await openSheet(user);
    await user.click(within(sheet).getByRole("radio", { name: "Guardar en su rama" }));
    // Without gh logged in, the sheet offers no PR: the side panel says what is missing.
    expect(within(sheet).queryByRole("switch", { name: "Publicar y abrir un PR" })).not.toBeInTheDocument();
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
    localStorage.clear();
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
    await openReviewTask(user, /^Home/);
    const folder = await screen.findByRole("group", { name: "Your folder" });
    expect(within(folder).getByRole("button", { name: /Commits to pull: 2/ })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Accept changes…" }));
    const sheet = await screen.findByRole("dialog", { name: "Accept changes" }, SLOW);
    await user.click(await within(sheet).findByRole("radio", { name: "Keep on its branch" }));
    expect(within(sheet).getByRole("switch", { name: "Publish and open a PR" })).toHaveAttribute("aria-checked", "false");
    await user.click(within(sheet).getByRole("switch", { name: "Publish and open a PR" }));
    expect(within(sheet).getByRole("button", { name: "Save and open PR" })).toBeInTheDocument();
    await user.click(within(sheet).getByRole("switch", { name: "Publish and open a PR" }));
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
