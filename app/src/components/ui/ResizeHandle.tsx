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
   * Which way grows the area: `-1` when it sits to the right of the handle
   * (dragging left makes it wider), `1` when it sits to the left.
   */
  direction?: 1 | -1;
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
  title,
  className,
}: ResizeHandleProps) {
  const drag = useRef<{ x: number; start: number } | null>(null);

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const step = e.shiftKey ? BIG_STEP : STEP;
    // Arrows point where the edge goes: left widens an area on the right.
    const grow = (towardLeft: boolean) => onChange(clampSize(value + (towardLeft ? -direction : direction) * step, min, max));
    if (e.key === "ArrowLeft") grow(true);
    else if (e.key === "ArrowRight") grow(false);
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
    drag.current = { x: e.clientX, start: value };
    document.documentElement.classList.add("resizing");
  };

  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (!d) return;
    onChange(clampSize(d.start + (e.clientX - d.x) * direction, min, max));
  };

  const end = (e: PointerEvent<HTMLDivElement>) => {
    if (!drag.current) return;
    drag.current = null;
    e.currentTarget.releasePointerCapture?.(e.pointerId);
    document.documentElement.classList.remove("resizing");
  };

  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      aria-valuenow={Math.round(value)}
      aria-valuemin={Math.round(min)}
      aria-valuemax={Math.round(max)}
      tabIndex={0}
      title={title}
      className={["resize-handle", className].filter(Boolean).join(" ")}
      onKeyDown={onKeyDown}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={end}
      onPointerCancel={end}
      onDoubleClick={onToggleMax}
    />
  );
}
