// Helpers for our own select (components/ui/Select.tsx) in tests.

import { screen, within } from "@testing-library/react";
import type { UserEvent } from "@testing-library/user-event";

/** The labels of the open list's options, in order. */
export function optionLabels(): string[] {
  return within(screen.getByRole("listbox"))
    .getAllByRole("option")
    .map((o) => o.querySelector(".popover__label")?.textContent ?? "");
}

/** Opens a select and picks the option with that name (its label; the second line describes it). */
export async function chooseOption(user: UserEvent, control: HTMLElement, option: string | RegExp) {
  await user.click(control);
  await user.click(within(screen.getByRole("listbox")).getByRole("option", { name: option }));
}

/** Picks the composer's mode ("Proyecto", "Pregunta", "Investigar"; "Mode" in English). */
export async function chooseMode(user: UserEvent, mode: string, label = "Modo") {
  await chooseOption(user, await screen.findByRole("combobox", { name: label }), mode);
}
