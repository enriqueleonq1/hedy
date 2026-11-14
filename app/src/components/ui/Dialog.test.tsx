import { useState } from "react";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { Dialog } from "./Dialog";

/** Its state lives in the parent, which passes `onClose` inline like the screens do. */
function Form() {
  const [name, setName] = useState("");
  const [note, setNote] = useState("");
  return (
    <Dialog title="Prueba" onClose={() => setName("")} footer={<button>Guardar</button>}>
      <input aria-label="Nombre" value={name} onChange={(e) => setName(e.target.value)} />
      <input aria-label="Nota" value={note} onChange={(e) => setNote(e.target.value)} />
    </Dialog>
  );
}

describe("Dialog", () => {
  it("keeps the focus in the field being typed while the parent re-renders", async () => {
    const user = userEvent.setup();
    render(<Form />);
    expect(screen.getByLabelText("Nombre")).toHaveFocus();
    await user.type(screen.getByLabelText("Nota"), "hola");
    expect(screen.getByLabelText("Nota")).toHaveValue("hola");
    expect(screen.getByLabelText("Nota")).toHaveFocus();
    expect(screen.getByLabelText("Nombre")).toHaveValue("");
  });

  it("keeps Tab and Shift+Tab inside", async () => {
    const user = userEvent.setup();
    render(
      <>
        <button>Fuera</button>
        <Form />
      </>,
    );
    await user.click(screen.getByRole("button", { name: "Guardar" }));
    await user.tab();
    expect(screen.getByRole("button", { name: "Cerrar" })).toHaveFocus();
    await user.tab({ shift: true });
    expect(screen.getByRole("button", { name: "Guardar" })).toHaveFocus();
  });
});
