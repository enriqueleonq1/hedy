// The edge between two areas, to drag (or move with the arrows) to resize one of
// them: a focusable `separator` with its value, like the WAI-ARIA window
// splitter. Double-click (or Enter) toggles between the largest size and the
// previous one.

import { useRef, type KeyboardEvent, type PointerEvent } from "react";

/** Arrow keys move by this many pixels; with Shift, by `BIG_STEP`. */
const STEP = 16;
const BIG_STEP = 64;

export interface ResizeHandleProps {
  /** Accessible name ("Cambiar el ancho del panel"). */
  label: string;
  /** Current size of the area it resizes, in CSS pixels. */
  value: number;
  min: number;
  max: number;
  onChange: (value: number) => void;
  /** Double-click or Enter: maximize / restore. */
  onToggleMax?: () => void;
  /**
   * Which way grows the area: `-1` when it sits after the handle (to its right,
   * or below it: dragging left or up makes it bigger), `1` when it sits before.
   */
  direction?: 1 | -1;
  /** `vertical`: an edge between areas side by side (the default); `horizontal`: between areas stacked. */
  orientation?: "vertical" | "horizontal";
  /** Shown on hover. */
  title?: string;
  className?: string;
}

export function clampSize(value: number, min: number, max: number): number {
  return Math.round(Math.min(Math.max(value, min), Math.max(min, max)));
}

export function ResizeHandle({
  label,
  value,
  min,
  max,
  onChange,
  onToggleMax,
  direction = -1,
  orientation = "vertical",
  title,
  className,
}: ResizeHandleProps) {
  const drag = useRef<{ at: number; start: number } | null>(null);
  const stacked = orientation === "horizontal";
  const position = (e: PointerEvent<HTMLDivElement>) => (stacked ? e.clientY : e.clientX);

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const step = e.shiftKey ? BIG_STEP : STEP;
    // Arrows point where the edge goes: left (or up) grows an area after it.
    const move = (towardStart: boolean) => onChange(clampSize(value + (towardStart ? -direction : direction) * step, min, max));
    if (e.key === (stacked ? "ArrowUp" : "ArrowLeft")) move(true);
    else if (e.key === (stacked ? "ArrowDown" : "ArrowRight")) move(false);
    else if (e.key === "Home") onChange(direction === -1 ? max : min);
    else if (e.key === "End") onChange(direction === -1 ? min : max);
    else if (e.key === "Enter" && onToggleMax) onToggleMax();
    else return;
    e.preventDefault();
  };

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.currentTarget.setPointerCapture?.(e.pointerId);
    e.currentTarget.focus({ preventScroll: true });
    drag.current = { at: position(e), start: value };
    document.documentElement.classList.add("resizing");
    if (stacked) document.documentElement.classList.add("resizing--rows");
  };

  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (!d) return;
    onChange(clampSize(d.start + (position(e) - d.at) * direction, min, max));
  };

  const end = (e: PointerEvent<HTMLDivElement>) => {
    if (!drag.current) return;
    drag.current = null;
    e.currentTarget.releasePointerCapture?.(e.pointerId);
    document.documentElement.classList.remove("resizing", "resizing--rows");
  };

  return (
    <div
      role="separator"
      aria-orientation={orientation}
      aria-label={label}
      aria-valuenow={Math.round(value)}
      aria-valuemin={Math.round(min)}
      aria-valuemax={Math.round(max)}
      tabIndex={0}
      title={title}
      className={["resize-handle", stacked && "resize-handle--rows", className].filter(Boolean).join(" ")}
      onKeyDown={onKeyDown}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={end}
      onPointerCancel={end}
      onDoubleClick={onToggleMax}
    />
  );
}
