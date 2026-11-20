import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { ConfirmButton } from "./Button";

describe("ConfirmButton", () => {
  it("is a danger button that asks for a second click", async () => {
    const onConfirm = vi.fn();
    render(
      <ConfirmButton className="extra" confirmLabel="¿Seguro?" onConfirm={onConfirm}>
        Quitar
      </ConfirmButton>,
    );
    const button = screen.getByRole("button", { name: "Quitar" });
    expect(button).toHaveClass("btn--danger", "extra");
    expect(button).not.toHaveClass("btn--armed");

    await userEvent.click(button);
    const armed = screen.getByRole("button", { name: "¿Seguro?" });
    expect(armed).toHaveClass("btn--danger", "btn--armed", "extra");
    expect(onConfirm).not.toHaveBeenCalled();

    await userEvent.click(armed);
    expect(onConfirm).toHaveBeenCalledOnce();
  });
});
