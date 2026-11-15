import { Fragment, useEffect, useLayoutEffect, useRef, type KeyboardEvent } from "react";

import type { MenuItem } from "./Menu";

const ENABLED_ITEM = "[role=menuitem]:not(:disabled)";
const MARGIN = 8;

/** Where a context menu opens: the pointer, or the element it was opened for from the keyboard. */
export interface MenuPoint {
  x: number;
  y: number;
}

/** The point to open a context menu at when it comes from the keyboard (Shift+F10, the menu key). */
export function pointFor(element: Element): MenuPoint {
  const rect = element.getBoundingClientRect();
  return { x: rect.left + 24, y: rect.bottom };
}

/** Shift+F10 or the context-menu key: the keyboard's right click. */
export function isMenuKey(e: { key: string; shiftKey: boolean }): boolean {
  return e.key === "ContextMenu" || (e.shiftKey && e.key === "F10");
}

/**
 * A right-click menu at a point (same items and look as `MenuButton`): arrows,
 * Home and End move, Escape or a click outside closes it and gives the focus
 * back to where it was. It stays inside the window.
 */
export function ContextMenu({
  at,
  items,
  label,
  onClose,
}: {
  at: MenuPoint;
  items: MenuItem[];
  /** Accessible name of the menu. */
  label: string;
  onClose: () => void;
}) {
  const menu = useRef<HTMLDivElement>(null);
  const onCloseRef = useRef(onClose);
  useEffect(() => {
    onCloseRef.current = onClose;
  });

  useLayoutEffect(() => {
    const el = menu.current;
    if (!el) return;
    const { width, height } = el.getBoundingClientRect();
    const left = Math.max(MARGIN, Math.min(at.x, window.innerWidth - width - MARGIN));
    const up = at.y + height + MARGIN > window.innerHeight && at.y - height > MARGIN;
    el.style.left = `${left}px`;
    el.style.top = `${up ? at.y - height : Math.max(MARGIN, at.y)}px`;
    el.dataset.side = up ? "top" : "bottom";
  }, [at]);

  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    menu.current?.querySelector<HTMLButtonElement>(ENABLED_ITEM)?.focus();
    const onDown = (e: MouseEvent) => {
      if (!menu.current?.contains(e.target as Node)) onCloseRef.current();
    };
    const onScroll = (e: Event) => {
      if (!menu.current?.contains(e.target as Node)) onCloseRef.current();
    };
    document.addEventListener("mousedown", onDown);
    window.addEventListener("scroll", onScroll, true);
    window.addEventListener("resize", onCloseRef.current);
    return () => {
      document.removeEventListener("mousedown", onDown);
      window.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("resize", onCloseRef.current);
      if (previous?.isConnected) previous.focus();
    };
  }, []);

  const onKey = (e: KeyboardEvent<HTMLDivElement>) => {
    const list = [...(menu.current?.querySelectorAll<HTMLButtonElement>(ENABLED_ITEM) ?? [])];
    const at = list.indexOf(document.activeElement as HTMLButtonElement);
    const move = (to: number) => {
      e.preventDefault();
      list[(to + list.length) % list.length]?.focus();
    };
    if (e.key === "ArrowDown") move(at + 1);
    else if (e.key === "ArrowUp") move(at - 1);
    else if (e.key === "Home") move(0);
    else if (e.key === "End") move(list.length - 1);
    else if (e.key === "Escape" || e.key === "Tab") {
      e.preventDefault();
      e.stopPropagation();
      onClose();
    }
  };

  return (
    <div
      ref={menu}
      className="menu__list context-menu"
      role="menu"
      aria-label={label}
      style={{ left: at.x, top: at.y }}
      onKeyDown={onKey}
      onContextMenu={(e) => e.preventDefault()}
    >
      {items.map((item) => (
        <Fragment key={item.label}>
          {item.separatorBefore && <div role="separator" className="menu__separator" />}
          <button
            type="button"
            role="menuitem"
            tabIndex={-1}
            className={["menu__item", item.shortcut && "menu__item--shortcut", item.danger && "menu__item--danger"]
              .filter(Boolean)
              .join(" ")}
            disabled={item.disabled}
            onClick={() => {
              onClose();
              item.onSelect();
            }}
          >
            {item.shortcut ? (
              <>
                <span className="menu__label">{item.label}</span>
                <kbd className="kbd menu__shortcut" aria-hidden>
                  {item.shortcut}
                </kbd>
              </>
            ) : (
              item.label
            )}
          </button>
        </Fragment>
      ))}
    </div>
  );
}
