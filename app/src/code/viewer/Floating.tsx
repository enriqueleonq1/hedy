// A small card floating next to a point (a blame entry, a right-clicked line),
// kept inside the window. Escape or a click outside closes it.

import { useLayoutEffect, useRef, type ReactNode } from "react";
import { createPortal } from "react-dom";

const MARGIN = 8;

export function Floating({
  at,
  label,
  onClose,
  children,
  className,
  onPointerEnter,
  onPointerLeave,
  autoFocus = false,
}: {
  at: { x: number; y: number };
  label: string;
  onClose: () => void;
  children: ReactNode;
  className?: string;
  onPointerEnter?: () => void;
  onPointerLeave?: () => void;
  /** Moves the focus into the card (its first field or button). */
  autoFocus?: boolean;
}) {
  const card = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    const el = card.current;
    if (!el) return;
    const left = Math.max(MARGIN, Math.min(at.x, window.innerWidth - el.offsetWidth - MARGIN));
    const below = at.y + el.offsetHeight + MARGIN <= window.innerHeight;
    const top = below ? at.y : Math.max(MARGIN, at.y - el.offsetHeight - 24);
    el.style.left = `${left}px`;
    el.style.top = `${top}px`;
    if (autoFocus) el.querySelector<HTMLElement>("textarea, input, button")?.focus();
    const onDown = (e: MouseEvent) => {
      if (!el.contains(e.target as Node)) onClose();
    };
    document.addEventListener("mousedown", onDown, true);
    return () => document.removeEventListener("mousedown", onDown, true);
  }, [at.x, at.y, autoFocus, onClose]);

  return createPortal(
    <div
      ref={card}
      className={["floating-card", className].filter(Boolean).join(" ")}
      role="dialog"
      aria-label={label}
      style={{ left: at.x, top: at.y }}
      onPointerEnter={onPointerEnter}
      onPointerLeave={onPointerLeave}
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.preventDefault();
          e.stopPropagation();
          onClose();
        }
      }}
    >
      {children}
    </div>,
    document.body,
  );
}
