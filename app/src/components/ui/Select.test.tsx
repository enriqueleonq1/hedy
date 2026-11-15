import { useState } from "react";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { Dialog } from "./Dialog";
import { placePopover } from "./popover";
import { Select, type SelectOption } from "./Select";

const MODELS: SelectOption[] = [
  { value: "", label: "Modelo de la cuenta", detail: "El que la cuenta usa por defecto" },
  { value: "opus", label: "Opus", detail: "El más capaz" },
  { value: "old", label: "Obsoleto", disabled: true },
  { value: "sonnet", label: "Sonnet", hint: "⌘2" },
  { value: "haiku", label: "Haiku" },
];

function Picker({ onChange = () => {}, initial = "opus" }: { onChange?: (value: string) => void; initial?: string }) {
  const [value, setValue] = useState(initial);
  return (
    <>
      <button type="button">Antes</button>
      <Select
        label="Modelo"
        title="Modelos"
        value={value}
        options={MODELS}
        footnote="Solo sugerencias"
        onChange={(next) => {
          setValue(next);
          onChange(next);
        }}
      />
      <p>Fuera</p>
    </>
  );
}

const control = () => screen.getByRole("combobox", { name: "Modelo" });
const activeOption = () => document.getElementById(control().getAttribute("aria-activedescendant") ?? "");

describe("Select", () => {
  it("shows the chosen option and opens a list with a title, a check and a footnote", async () => {
    const user = userEvent.setup();
    render(<Picker />);
    expect(control()).toHaveTextContent("Opus");
    expect(control()).toHaveAttribute("aria-haspopup", "listbox");
    expect(control()).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("listbox")).toBeNull();

    await user.click(control());
    const list = screen.getByRole("listbox", { name: "Modelos" });
    expect(control()).toHaveAttribute("aria-expanded", "true");
    expect(control()).toHaveAttribute("aria-controls", list.id);
    expect(screen.getAllByRole("option").map((o) => o.getAttribute("aria-selected"))).toEqual([
      "false",
      "true",
      "false",
      "false",
      "false",
    ]);
    expect(screen.getByRole("option", { name: "Opus" })).toHaveAccessibleDescription("El más capaz");
    expect(screen.getByRole("option", { name: "Sonnet" })).toHaveAccessibleDescription("⌘2");
    expect(screen.getByRole("option", { name: "Obsoleto" })).toHaveAttribute("aria-disabled", "true");
    expect(screen.getByText("Solo sugerencias")).toBeInTheDocument();
    // The focus stays on the control, which points at the active option.
    expect(control()).toHaveFocus();
    expect(activeOption()).toHaveTextContent("Opus");
  });

  it("chooses with the mouse and closes", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<Picker onChange={onChange} />);
    await user.click(control());
    await user.click(screen.getByRole("option", { name: "Haiku" }));
    expect(onChange).toHaveBeenCalledWith("haiku");
    expect(screen.queryByRole("listbox")).toBeNull();
    expect(control()).toHaveTextContent("Haiku");
    expect(control()).toHaveFocus();

    // A disabled option does nothing.
    await user.click(control());
    await user.click(screen.getByRole("option", { name: "Obsoleto" }));
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("listbox")).toBeInTheDocument();
  });

  it("moves with the arrows, Home and End, skipping disabled options, and chooses with Enter", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<Picker onChange={onChange} />);
    control().focus();

    await user.keyboard("{ArrowDown}");
    expect(screen.getByRole("listbox")).toBeInTheDocument();
    expect(activeOption()).toHaveTextContent("Opus");
    await user.keyboard("{ArrowDown}");
    expect(activeOption()).toHaveTextContent("Sonnet");
    await user.keyboard("{ArrowUp}");
    expect(activeOption()).toHaveTextContent("Opus");
    await user.keyboard("{End}");
    expect(activeOption()).toHaveTextContent("Haiku");
    // The ends stop the arrows.
    await user.keyboard("{ArrowDown}");
    expect(activeOption()).toHaveTextContent("Haiku");
    await user.keyboard("{Home}");
    expect(activeOption()).toHaveTextContent("Modelo de la cuenta");
    await user.keyboard("{ArrowDown}{ArrowDown}{Enter}");
    expect(onChange).toHaveBeenCalledWith("sonnet");
    expect(screen.queryByRole("listbox")).toBeNull();
    expect(control()).toHaveFocus();

    // Space opens and chooses too.
    await user.keyboard(" ");
    expect(activeOption()).toHaveTextContent("Sonnet");
    await user.keyboard("{ArrowUp} ");
    expect(onChange).toHaveBeenLastCalledWith("opus");
  });

  it("jumps to the option whose name starts with the typed letters", async () => {
    const user = userEvent.setup();
    render(<Picker />);
    control().focus();
    await user.keyboard("h");
    expect(screen.getByRole("listbox")).toBeInTheDocument();
    expect(activeOption()).toHaveTextContent("Haiku");
    // Letters typed in a row make one word.
    await user.keyboard("a");
    expect(activeOption()).toHaveTextContent("Haiku");
    await user.keyboard("{Escape}so");
    expect(activeOption()).toHaveTextContent("Sonnet");
    // Disabled options are never reached by typing ("Obsoleto").
    await user.keyboard("{Escape}o");
    expect(activeOption()).toHaveTextContent("Opus");
  });

  it("closes with Escape, keeping the value and the focus, without closing a dialog around it", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    const onClose = vi.fn();
    render(
      <Dialog title="Fases" onClose={onClose}>
        <Picker onChange={onChange} />
      </Dialog>,
    );
    await user.click(control());
    await user.keyboard("{ArrowDown}{Escape}");
    expect(screen.queryByRole("listbox")).toBeNull();
    expect(control()).toHaveFocus();
    expect(control()).toHaveTextContent("Opus");
    expect(onChange).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
    // A second Escape, with the list closed, belongs to the dialog.
    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalled();
  });

  it("closes on a click outside or when the focus leaves", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<Picker onChange={onChange} />);
    await user.click(control());
    await user.click(screen.getByText("Fuera"));
    expect(screen.queryByRole("listbox")).toBeNull();

    await user.click(control());
    await user.tab();
    expect(screen.queryByRole("listbox")).toBeNull();
    expect(onChange).not.toHaveBeenCalled();
  });

  it("does not open when disabled, and a label element can name it", async () => {
    const user = userEvent.setup();
    render(
      <>
        <label htmlFor="where">Para</label>
        <Select id="where" value="a" options={[{ value: "a", label: "Global" }]} onChange={() => {}} disabled />
      </>,
    );
    const where = screen.getByRole("combobox", { name: "Para" });
    expect(where).toBeDisabled();
    await user.click(where);
    expect(screen.queryByRole("listbox")).toBeNull();
  });

  it("opens upward when there is no room below, and stays inside the window", () => {
    const viewport = { width: 1000, height: 800 };
    const low = placePopover({ top: 740, bottom: 772, left: 950, right: 1000 }, { width: 260, height: 300 }, viewport);
    expect(low.side).toBe("top");
    expect(low.style.bottom).toBe(800 - 740 + 6);
    expect(low.style.left).toBe(1000 - 8 - 260);

    const high = placePopover({ top: 100, bottom: 132, left: 20, right: 120 }, { width: 260, height: 300 }, viewport);
    expect(high.side).toBe("bottom");
    expect(high.style.top).toBe(132 + 6);
    expect(high.style.left).toBe(20);
    expect(high.style.minWidth).toBe(100);

    const end = placePopover({ top: 100, bottom: 132, left: 600, right: 700 }, { width: 260, height: 300 }, viewport, "end");
    expect(end.style.left).toBe(700 - 260);
  });
});
