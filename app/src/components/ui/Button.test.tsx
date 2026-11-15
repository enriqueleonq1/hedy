import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { ConfirmButton } from "./Button";

describe("ConfirmButton", () => {
  it("is a quiet destructive ghost until armed, then asks to confirm", async () => {
    const onConfirm = vi.fn();
    render(
      <ConfirmButton className="extra" confirmLabel="¿Seguro?" onConfirm={onConfirm}>
        Quitar
      </ConfirmButton>,
    );
    const button = screen.getByRole("button", { name: "Quitar" });
    expect(button).toHaveClass("btn--ghost", "btn--destructive", "extra");

    await userEvent.click(button);
    const armed = screen.getByRole("button", { name: "¿Seguro?" });
    expect(armed).toHaveClass("btn--danger", "extra");
    expect(armed).not.toHaveClass("btn--destructive");
    expect(onConfirm).not.toHaveBeenCalled();

    await userEvent.click(armed);
    expect(onConfirm).toHaveBeenCalledOnce();
  });
});
