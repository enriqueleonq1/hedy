import { useEffect, useId, useRef, type ReactNode } from "react";

import { useT } from "../../i18n";

interface DialogProps {
  title: string;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  /** `false`: no ×, and Escape and the backdrop do nothing (a step that must finish). */
  dismissible?: boolean;
}

const FOCUSABLE = "a[href], button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex='-1'])";

/** Modal dialog: Escape and backdrop close it; focus moves in, stays in and comes back. */
export function Dialog({ title, onClose, children, footer, dismissible = true }: DialogProps) {
  const t = useT();
  const titleId = useId();
  const panel = useRef<HTMLDivElement>(null);
  // Callers pass `onClose` inline: the effect below must not rerun (and move
  // the focus) on every render of the parent.
  const onCloseRef = useRef(onClose);
  const dismissibleRef = useRef(dismissible);
  useEffect(() => {
    onCloseRef.current = onClose;
    dismissibleRef.current = dismissible;
  });

  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    // The first field or action; the close button when there is nothing else.
    (
      panel.current?.querySelector<HTMLElement>("input, select, textarea, button:not(.dialog__close)") ??
      panel.current?.querySelector<HTMLElement>(".dialog__close")
    )?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        if (dismissibleRef.current) onCloseRef.current();
        return;
      }
      if (e.key !== "Tab" || !panel.current) return;
      // In document order (a selector list alone does not guarantee it everywhere).
      const focusable = [...panel.current.querySelectorAll<HTMLElement>("*")].filter((el) => el.matches(FOCUSABLE));
      const first = focusable[0];
      const last = focusable.at(-1);
      if (!first || !last) return;
      const inside = panel.current.contains(document.activeElement);
      if (e.shiftKey && (document.activeElement === first || !inside)) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && (document.activeElement === last || !inside)) {
        e.preventDefault();
        first.focus();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      previous?.focus();
    };
  }, []);

  return (
    <div className="dialog-backdrop" onMouseDown={(e) => dismissible && e.target === e.currentTarget && onClose()}>
      <div
        ref={panel}
        className="dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        data-locked={dismissible ? undefined : "true"}
      >
        <header className="dialog__header">
          <h2 id={titleId}>{title}</h2>
          {dismissible && (
            <button className="dialog__close" aria-label={t("common.close")} onClick={onClose}>
              ×
            </button>
          )}
        </header>
        <div className="dialog__body">{children}</div>
        {footer && <footer className="dialog__footer">{footer}</footer>}
      </div>
    </div>
  );
}
