import { act, render, screen } from "@testing-library/react";

import { I18nProvider, translator } from "../../i18n";
import { ACTIVITY_LABEL, ACTIVITY_TIMING, WORKING_ACTIVITIES, type WorkingActivity } from "./activity";
import { Mascot, MASCOT_STATE_LABEL, type MascotState } from "./Mascot";
import { WorkingWord } from "./WorkingWord";

const states = Object.keys(MASCOT_STATE_LABEL) as MascotState[];

describe("Mascot", () => {
  it.each(states)("renders the %s state with its accessible name", (state) => {
    render(<Mascot color="#7AA2F7" state={state} />);
    const svg = screen.getByRole("img", { name: translator("es")(MASCOT_STATE_LABEL[state]) });
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

  it("keeps smiling eyes for hover only at rest", () => {
    const { container, unmount } = render(<Mascot color="#7AA2F7" state="idle" />);
    expect(container.querySelector(".m-happy")).toHaveAttribute("aria-hidden");
    unmount();
    const { container: working } = render(<Mascot color="#7AA2F7" state="working" />);
    expect(working.querySelector(".m-happy")).toBeNull();
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

  const props: Record<WorkingActivity, string> = {
    reading: ".m-book",
    search: ".m-lens",
    edit: ".m-laptop",
    command: ".m-term",
    test: ".m-flask",
    web: ".m-globe",
    think: ".m-gear",
    subagents: ".m-mini",
    waiting: ".m-lava",
    long: ".m-sweat",
  };

  it.each(WORKING_ACTIVITIES)("works on %s with its props and a bilingual name", (activity) => {
    const { container, unmount } = render(<Mascot color="#7AA2F7" state="working" activity={activity} />);
    const svg = screen.getByRole("img", { name: translator("es")(ACTIVITY_LABEL[activity]) });
    expect(svg).toHaveAttribute("data-activity", activity);
    expect(svg.classList.contains(`mascot--act-${activity}`)).toBe(true);
    expect(container.querySelector(props[activity]), activity).not.toBeNull();
    unmount();

    render(
      <I18nProvider setting="en">
        <Mascot color="#7AA2F7" state="working" activity={activity} />
      </I18nProvider>,
    );
    expect(screen.getByRole("img", { name: translator("en")(ACTIVITY_LABEL[activity]) })).toBeInTheDocument();
  });

  it("works on the laptop by default, and named as working", () => {
    const { container } = render(<Mascot color="#7AA2F7" state="working" />);
    expect(screen.getByRole("img", { name: "Trabajando" })).toHaveAttribute("data-activity", "edit");
    expect(container.querySelector(".m-laptop")).not.toBeNull();
  });

  it("ignores the activity outside the working state", () => {
    const { container } = render(<Mascot color="#7AA2F7" state="thinking" activity="reading" />);
    expect(screen.getByRole("img", { name: "Pensando" })).not.toHaveAttribute("data-activity");
    expect(container.querySelector(".m-book")).toBeNull();
  });

  it("keeps a tiny working mascot simple, whatever it does", () => {
    for (const activity of WORKING_ACTIVITIES) {
      const { container, unmount } = render(
        <Mascot color="#7AA2F7" state="working" activity={activity} size={24} />,
      );
      expect(container.querySelector(".mascot--tiny"), activity).not.toBeNull();
      for (const selector of [".m-props", ".m-behind", ".m-drop", ".m-hand", ".m-glasses", ".m-cheeks"]) {
        expect(container.querySelector(selector), `${activity} ${selector}`).toBeNull();
      }
      expect(container.querySelector(".m-eyes"), activity).not.toBeNull();
      unmount();
    }
  });

  it("cross-fades from one activity to the next", () => {
    vi.useFakeTimers();
    try {
      const { container, rerender } = render(<Mascot color="#7AA2F7" state="working" activity="reading" />);
      expect(container.querySelector(".m-leave")).toBeNull();
      rerender(<Mascot color="#7AA2F7" state="working" activity="test" />);
      expect(container.querySelector(".m-props.m-leave .m-book")).not.toBeNull();
      expect(container.querySelector(".m-props.m-enter .m-flask")).not.toBeNull();
      expect(container.querySelector(".m-face.m-leave")).toHaveAttribute("aria-hidden");
      act(() => {
        vi.advanceTimersByTime(ACTIVITY_TIMING.fadeMs);
      });
      expect(container.querySelector(".m-leave")).toBeNull();
      expect(container.querySelector(".m-book")).toBeNull();
      expect(container.querySelector(".m-props:not(.m-enter) .m-flask")).not.toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("WorkingWord", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  const word = (container: HTMLElement) => container.querySelector(".working-word__text")?.textContent;

  it("rotates the words of its activity, and starts again on a new one", () => {
    const { container, rerender } = render(<WorkingWord activity="reading" />);
    expect(container.querySelector(".working-word")).toHaveAttribute("aria-hidden");
    expect(word(container)).toBe("Leyendo");
    act(() => {
      vi.advanceTimersByTime(ACTIVITY_TIMING.wordEveryMs);
    });
    expect(word(container)).toBe("Hojeando");
    act(() => {
      vi.advanceTimersByTime(2 * ACTIVITY_TIMING.wordEveryMs);
    });
    expect(word(container)).toBe("Leyendo");
    act(() => {
      vi.advanceTimersByTime(ACTIVITY_TIMING.wordEveryMs);
    });
    rerender(<WorkingWord activity="long" />);
    expect(word(container)).toBe("Sudando la gota");
    rerender(<WorkingWord activity={undefined} />);
    expect(container.querySelector(".working-word")).toBeNull();
  });

  it("speaks English", () => {
    const { container } = render(
      <I18nProvider setting="en">
        <WorkingWord activity="search" />
      </I18nProvider>,
    );
    expect(word(container)).toBe("Sniffing around");
  });
});

