import { useState } from "react";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { I18nProvider } from "../../i18n";
import { accountPalette } from "../../theme/tokens";
import { ColorPicker, Segmented } from "./controls";

function Mode() {
  const [value, setValue] = useState("system");
  return (
    <>
      <button type="button">Antes</button>
      <Segmented
        label="Modo"
        value={value}
        onChange={setValue}
        options={[
          { value: "system", label: "Sistema" },
          { value: "light", label: "Claro" },
          { value: "dark", label: "Oscuro" },
        ]}
      />
      <button type="button">Después</button>
    </>
  );
}

function Colors({ initial }: { initial: string }) {
  const [value, setValue] = useState(initial);
  return (
    <I18nProvider setting="es">
      <button type="button">Antes</button>
      <ColorPicker label="Color" value={value} onChange={setValue} />
    </I18nProvider>
  );
}

describe("Segmented", () => {
  it("is one Tab stop, and the arrow keys move the choice (wrapping), Home and End jump", async () => {
    const user = userEvent.setup();
    render(<Mode />);
    await user.click(screen.getByRole("button", { name: "Antes" }));
    await user.tab();
    expect(screen.getByRole("radio", { name: "Sistema" })).toHaveFocus();

    await user.keyboard("{ArrowRight}");
    const light = screen.getByRole("radio", { name: "Claro" });
    expect(light).toHaveFocus();
    expect(light).toBeChecked();
    expect(light).toHaveAttribute("tabindex", "0");
    expect(screen.getByRole("radio", { name: "Sistema" })).toHaveAttribute("tabindex", "-1");

    await user.keyboard("{End}");
    expect(screen.getByRole("radio", { name: "Oscuro" })).toBeChecked();
    await user.keyboard("{ArrowDown}");
    expect(screen.getByRole("radio", { name: "Sistema" })).toBeChecked();
    await user.keyboard("{ArrowLeft}");
    expect(screen.getByRole("radio", { name: "Oscuro" })).toBeChecked();

    // Tab leaves the group at once.
    await user.tab();
    expect(screen.getByRole("button", { name: "Después" })).toHaveFocus();
  });

  it("lets Alt, Ctrl and ⌘ with an arrow, Home or End pass through", async () => {
    const user = userEvent.setup();
    render(<Mode />);
    const system = screen.getByRole("radio", { name: "Sistema" });
    system.focus();
    await user.keyboard("{Alt>}{ArrowRight}{/Alt}");
    await user.keyboard("{Control>}{End}{/Control}");
    await user.keyboard("{Meta>}{ArrowLeft}{/Meta}");
    expect(system).toHaveFocus();
    expect(system).toBeChecked();
  });
});

describe("ColorPicker", () => {
  it("moves between swatches with the arrow keys, then Tab goes on to the hex field", async () => {
    const user = userEvent.setup();
    render(<Colors initial={accountPalette[0]!.hex} />);
    await user.click(screen.getByRole("button", { name: "Antes" }));
    await user.tab();
    const swatches = screen.getAllByRole("radio");
    expect(swatches[0]).toHaveFocus();
    await user.keyboard("{ArrowRight}");
    expect(swatches[1]).toHaveFocus();
    expect(swatches[1]).toBeChecked();
    expect(swatches[1]).toHaveAttribute("tabindex", "0");
    expect(swatches[0]).toHaveAttribute("tabindex", "-1");
    await user.tab();
    expect(screen.getByRole("textbox")).toHaveFocus();
  });

  it("still enters on the first swatch when the color is a custom one", async () => {
    const user = userEvent.setup();
    render(<Colors initial="#123456" />);
    await user.click(screen.getByRole("button", { name: "Antes" }));
    await user.tab();
    expect(screen.getAllByRole("radio")[0]).toHaveFocus();
  });
});
