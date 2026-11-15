import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient } from "@tanstack/react-query";

import { App } from "../../App";
import { MockApi } from "../../api/MockApi";
import { chooseMode } from "../../test/select";
import { sortImageFiles } from "./ComposerImages";

// xterm needs a real canvas; the Terminal tab is not exercised here.
vi.mock("../../components/Terminal/TerminalView", () => ({ TerminalView: () => null }));

function setup(api = MockApi.withSampleData()) {
  api.delayMs = 0;
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<App api={api} queryClient={queryClient} />);
  return { api, user: userEvent.setup() };
}

const PNG_BYTES = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0]);
const png = (name: string) => new File([PNG_BYTES], name, { type: "image/png" });

/** What a paste or a drop of `files` carries. */
function transfer(files: File[], text = "") {
  return {
    files,
    items: files.map((file) => ({ kind: "file", type: file.type, getAsFile: () => file })),
    types: ["Files"],
    getData: (type: string) => (type === "text/plain" ? text : ""),
    dropEffect: "none",
  };
}

async function newTaskInHedy(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole("button", { name: /^hedy/ }));
  return screen.findByLabelText("Pídele algo a hedy");
}

describe("Images in the composer", () => {
  it("pastes an image, sends it with the message and shows it in the chat", async () => {
    const { user } = setup();
    const composer = await newTaskInHedy(user);
    await chooseMode(user, "Pregunta");

    fireEvent.paste(composer, { clipboardData: transfer([png("captura.png")]) });
    const tray = await screen.findByRole("list", { name: "Imágenes adjuntas" });
    expect(within(tray).getByRole("img", { name: "captura.png" })).toBeInTheDocument();

    await user.type(composer, "¿Qué falla en esta pantalla?");
    await user.click(screen.getByRole("button", { name: "Enviar" }));

    const sent = await screen.findByRole("list", { name: "Imágenes del mensaje" });
    expect(await within(sent).findByRole("img", { name: "captura.png" })).toHaveAttribute(
      "src",
      expect.stringMatching(/^data:image\/png;base64,iVBORw0KGgo/),
    );
    expect(screen.queryByRole("list", { name: "Imágenes adjuntas" })).toBeNull();
  });

  it("drops images and removes them with the keyboard", async () => {
    const { user } = setup();
    const composer = await newTaskInHedy(user);
    const form = composer.closest("form")!;

    fireEvent.dragOver(form, { dataTransfer: transfer([png("a.png")]) });
    expect(screen.getByText("Suelta para adjuntar")).toBeInTheDocument();
    fireEvent.drop(form, { dataTransfer: transfer([png("a.png"), png("b.png")]) });
    const tray = await screen.findByRole("list", { name: "Imágenes adjuntas" });
    await waitFor(() => expect(within(tray).getAllByRole("img")).toHaveLength(2));
    expect(screen.queryByText("Suelta para adjuntar")).toBeNull();

    within(tray).getByRole("button", { name: "Quitar a.png" }).focus();
    await user.keyboard("{Delete}");
    await waitFor(() => expect(within(tray).queryByRole("img", { name: "a.png" })).toBeNull());
    const left = within(tray).getByRole("button", { name: "Quitar b.png" });
    await waitFor(() => expect(left).toHaveFocus());

    await user.keyboard("{Enter}");
    await waitFor(() => expect(screen.queryByRole("list", { name: "Imágenes adjuntas" })).toBeNull());
    await waitFor(() => expect(composer).toHaveFocus());
  });

  it("says why a file is not attached", async () => {
    const { user } = setup();
    const composer = await newTaskInHedy(user);
    const notes = new File(["hola"], "notas.txt", { type: "text/plain" });
    fireEvent.drop(composer.closest("form")!, { dataTransfer: transfer([notes]) });
    expect(await screen.findByText("«notas.txt» no es una imagen PNG, JPEG, GIF o WebP")).toBeInTheDocument();
    expect(screen.queryByRole("list", { name: "Imágenes adjuntas" })).toBeNull();
  });

  it("disables attaching for an account that takes no images, and a paste says so", async () => {
    const api = MockApi.withSampleData();
    api.setImageSupport("claude-personal", "no");
    const { user } = setup(api);
    const composer = await newTaskInHedy(user);

    const attach = screen.getByRole("button", { name: "Adjuntar imágenes" });
    await waitFor(() => expect(attach).toHaveAttribute("aria-disabled", "true"));
    expect(attach).toHaveAccessibleDescription("Esta cuenta no acepta imágenes");
    expect(attach.title).toContain("Esta cuenta no acepta imágenes");

    fireEvent.paste(composer, { clipboardData: transfer([png("captura.png")]) });
    expect(await screen.findByText("Esta cuenta no acepta imágenes", { selector: ".toast *, .toast" })).toBeInTheDocument();
    expect(screen.queryByRole("list", { name: "Imágenes adjuntas" })).toBeNull();
  });

  it("does not let images wait in the queue while the agent works", async () => {
    const { user } = setup(MockApi.withSampleData().withSampleActivity());
    await user.click(await screen.findByRole("button", { name: /^homming/ }));
    await screen.findByRole("region", { name: "Mensajes en cola" });
    const attach = screen.getByRole("button", { name: "Adjuntar imágenes" });
    await waitFor(() => expect(attach).toHaveAttribute("aria-disabled", "true"));
    expect(attach).toHaveAccessibleDescription(/no esperan en cola/);
  });
});

describe("sortImageFiles", () => {
  it("keeps images that fit and explains the rest", () => {
    const big = new File([new Uint8Array(5 * 1024 * 1024 + 1)], "big.png", { type: "image/png" });
    const svg = new File(["<svg/>"], "logo.svg", { type: "image/svg+xml" });
    const { accepted, problems } = sortImageFiles([png("a.png"), big, svg, png("b.png"), png("c.png")], 2);
    expect(accepted.map((f) => f.name)).toEqual(["a.png", "b.png"]);
    expect(problems).toEqual([
      { key: "composer.images.tooLarge", params: { name: "big.png" } },
      { key: "composer.images.unsupported", params: { name: "logo.svg" } },
      { key: "composer.images.tooMany", params: { max: 4 } },
    ]);
  });
});
