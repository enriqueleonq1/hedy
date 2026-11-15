import { useEffect, useId, useRef, useState, type KeyboardEvent, type ReactNode } from "react";

const ENABLED_ITEM = "[role=menuitem]:not(:disabled)";

export interface MenuItem {
  label: string;
  onSelect: () => void;
  disabled?: boolean;
  /** A short second line: what the action is for. */
  hint?: string;
}

/**
 * A button that opens a menu of actions (the WAI-ARIA menu button): arrows,
 * Home and End move, Escape closes and gives the focus back, Tab leaves.
 */
export function MenuButton({
  label,
  items,
  children,
  className = "menu-button",
}: {
  /** Accessible name of the button. */
  label: string;
  items: MenuItem[];
  /** What the button shows (`⋯` by default). */
  children?: ReactNode;
  className?: string;
}) {
  const [open, setOpen] = useState(false);
  const id = useId();
  const wrap = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);

  const enabled = () => [...(menu.current?.querySelectorAll<HTMLButtonElement>(ENABLED_ITEM) ?? [])];

  useEffect(() => {
    if (!open) return;
    menu.current?.querySelector<HTMLButtonElement>(ENABLED_ITEM)?.focus();
    const onDown = (e: MouseEvent) => {
      if (!wrap.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  const close = (focus = true) => {
    setOpen(false);
    if (focus) trigger.current?.focus();
  };

  const onMenuKey = (e: KeyboardEvent<HTMLDivElement>) => {
    const list = enabled();
    const at = list.indexOf(document.activeElement as HTMLButtonElement);
    const move = (to: number) => {
      e.preventDefault();
      list[(to + list.length) % list.length]?.focus();
    };
    if (e.key === "ArrowDown") move(at + 1);
    else if (e.key === "ArrowUp") move(at - 1);
    else if (e.key === "Home") move(0);
    else if (e.key === "End") move(list.length - 1);
    else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close();
    } else if (e.key === "Tab") close(false);
  };

  return (
    <div className="menu" ref={wrap}>
      <button
        ref={trigger}
        type="button"
        className={className}
        aria-label={label}
        title={label}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? id : undefined}
        onClick={() => setOpen((o) => !o)}
        onKeyDown={(e) => {
          if (e.key === "ArrowDown" && !open) {
            e.preventDefault();
            setOpen(true);
          }
        }}
      >
        {children ?? <span aria-hidden>⋯</span>}
      </button>
      {open && (
        <div ref={menu} id={id} className="menu__list" role="menu" aria-label={label} onKeyDown={onMenuKey}>
          {items.map((item) => (
            <button
              key={item.label}
              type="button"
              role="menuitem"
              tabIndex={-1}
              className={item.hint ? "menu__item menu__item--hinted" : "menu__item"}
              disabled={item.disabled}
              onClick={() => {
                close();
                item.onSelect();
              }}
            >
              {item.hint ? (
                <>
                  <span className="menu__label">{item.label}</span>
                  <span className="menu__hint">{item.hint}</span>
                </>
              ) : (
                item.label
              )}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
