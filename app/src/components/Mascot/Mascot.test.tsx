import { render, screen } from "@testing-library/react";

import { Mascot, MASCOT_STATE_LABEL, type MascotState } from "./Mascot";

const states = Object.keys(MASCOT_STATE_LABEL) as MascotState[];

describe("Mascot", () => {
  it.each(states)("renders the %s state with its accessible name", (state) => {
    render(<Mascot color="#7AA2F7" state={state} />);
    const svg = screen.getByRole("img", { name: MASCOT_STATE_LABEL[state] });
    expect(svg).toHaveAttribute("data-state", state);
    expect(svg.classList.contains(`mascot--${state}`)).toBe(true);
  });

  it("uses the account color", () => {
    render(<Mascot color="#FF7AB6" state="sleeping" />);
    expect(screen.getByRole("img").style.getPropertyValue("--mascot-color")).toBe("#FF7AB6");
  });

  it("drops droplets, halo, cheeks and props below 32px", () => {
    const { container } = render(<Mascot color="#7AA2F7" state="idle" size={20} />);
    expect(container.querySelector(".mascot--tiny")).not.toBeNull();
    for (const selector of [".m-drop", ".m-halo", ".m-cheeks"]) {
      expect(container.querySelector(selector), selector).toBeNull();
    }
    expect(container.querySelector(".m-eyes")).not.toBeNull();
  });

  it("shows each state's props at full size", () => {
    const props: Record<Exclude<MascotState, "idle">, string> = {
      sleeping: ".m-zzz",
      thinking: ".m-cloud",
      working: ".m-laptop",
      done: ".m-party",
    };
    for (const [state, selector] of Object.entries(props)) {
      const { container, unmount } = render(<Mascot color="#7AA2F7" state={state as MascotState} size={120} />);
      expect(container.querySelector(selector), state).not.toBeNull();
      unmount();
    }
  });

  it("gives every instance its own goo filter", () => {
    const { container } = render(
      <>
        <Mascot color="#7AA2F7" state="done" />
        <Mascot color="#9ECE6A" state="done" />
      </>,
    );
    const ids = [...container.querySelectorAll("filter")].map((f) => f.id);
    expect(new Set(ids).size).toBe(2);
  });
});
