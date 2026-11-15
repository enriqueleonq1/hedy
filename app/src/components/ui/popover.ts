// Where a popover (a select's list, a menu) goes next to the control that opens
// it: below when it fits, above otherwise (the composer sits at the bottom of the
// window). Fixed to the viewport, so scrolling containers and dialogs never clip it.

import { useLayoutEffect, type RefObject } from "react";

/** Space between the control and the popover, and the popover and the window's edge. */
const GAP = 6;
const MARGIN = 8;
/** Never squeeze a list below this, even in a tiny window. */
const MIN_HEIGHT = 120;

export type PopoverSide = "top" | "bottom";

export interface PopoverPlacement {
  side: PopoverSide;
  style: { left: number; minWidth: number; maxHeight: number; top?: number; bottom?: number };
}

/** Pure placement math (exported for tests): `anchor` and the viewport in CSS pixels. */
export function placePopover(
  anchor: { top: number; bottom: number; left: number; right: number },
  popover: { width: number; height: number },
  viewport: { width: number; height: number },
  align: "start" | "end" = "start",
): PopoverPlacement {
  const below = viewport.height - anchor.bottom - GAP - MARGIN;
  const above = anchor.top - GAP - MARGIN;
  const up = popover.height > below && above > below;
  const room = Math.max(MIN_HEIGHT, up ? above : below);
  const wanted = align === "end" ? anchor.right - popover.width : anchor.left;
  const left = Math.max(MARGIN, Math.min(wanted, viewport.width - MARGIN - popover.width));
  const minWidth = Math.round(anchor.right - anchor.left);
  const style = up
    ? { left, minWidth, bottom: viewport.height - anchor.top + GAP, maxHeight: room }
    : { left, minWidth, top: anchor.bottom + GAP, maxHeight: room };
  return { side: up ? "top" : "bottom", style };
}

/**
 * Places `popover` next to `anchor` while `open`, before the first paint, and
 * again when the window resizes or anything scrolls. It writes the position
 * straight on the element (and its side in `data-side`, for the animation's
 * direction). The popover itself is the scrolling box (`overflow-y: auto`), so
 * its `scrollHeight` is its full height.
 */
export function useAnchoredPopover(
  open: boolean,
  anchor: RefObject<HTMLElement | null>,
  popover: RefObject<HTMLElement | null>,
  align: "start" | "end" = "start",
): void {
  useLayoutEffect(() => {
    if (!open) return;
    const update = () => {
      const a = anchor.current;
      const p = popover.current;
      if (!a || !p) return;
      const rect = a.getBoundingClientRect();
      const { side, style } = placePopover(
        rect,
        { width: Math.max(p.offsetWidth, rect.width), height: p.scrollHeight },
        { width: window.innerWidth, height: window.innerHeight },
        align,
      );
      p.setAttribute("data-side", side);
      const px = (n: number | undefined) => (n === undefined ? "" : `${n}px`);
      p.style.setProperty("top", px(style.top));
      p.style.setProperty("bottom", px(style.bottom));
      p.style.setProperty("left", px(style.left));
      p.style.setProperty("min-width", px(style.minWidth));
      p.style.setProperty("max-height", px(style.maxHeight));
    };
    update();
    window.addEventListener("resize", update);
    window.addEventListener("scroll", update, true);
    return () => {
      window.removeEventListener("resize", update);
      window.removeEventListener("scroll", update, true);
    };
  }, [open, anchor, popover, align]);
}
