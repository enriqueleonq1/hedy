// A right-click menu at the pointer (or under a row, from the keyboard): the
// same look and keys as `MenuButton`'s menu (arrows, Home/End, Escape gives the
// focus back to where it was, Tab or a click outside closes).

import { Fragment, useCallback, useLayoutEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { createPortal } from "react-dom";

import type { MenuItem } from "./Menu";

const ENABLED_ITEM = "[role=menuitem]:not(:disabled)";
const MARGIN = 8;

export interface ContextMenuItem extends MenuItem {
  icon?: ReactNode;
}

interface Opened {
  x: number;
  y: number;
  items: ContextMenuItem[];
  label: string;
  /** Gets the focus back when the menu closes. */
  returnTo: HTMLElement | null;
}

/**
 * `const menu = useContextMenu(); … onContextMenu={(e) => menu.open(e, items, label)} … {menu.element}`.
 */
export function useContextMenu() {
  const [opened, setOpened] = useState<Opened | null>(null);
  const open = useCallback((at: { clientX: number; clientY: number } | { x: number; y: number }, items: ContextMenuItem[], label: string) => {
    const x = "clientX" in at ? at.clientX : at.x;
    const y = "clientY" in at ? at.clientY : at.y;
    const active = document.activeElement;
    setOpened({ x, y, items, label, returnTo: active instanceof HTMLElement ? active : null });
  }, []);
  const returnTo = opened?.returnTo ?? null;
  const close = useCallback(
    (focus = true) => {
      setOpened(null);
      if (focus) returnTo?.focus({ preventScroll: true });
    },
    [returnTo],
  );
  const element = opened ? <ContextMenuList {...opened} onClose={close} /> : null;
  return { open, close, element, isOpen: opened !== null };
}

function ContextMenuList({ x, y, items, label, onClose }: Opened & { onClose: (focus?: boolean) => void }) {
  const menu = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    const el = menu.current;
    if (!el) return;
    // Inside the window: flipped left/up when it would overflow.
    const width = el.offsetWidth;
    const height = el.offsetHeight;
    const left = x + width + MARGIN > window.innerWidth ? Math.max(MARGIN, x - width) : x;
    const top = y + height + MARGIN > window.innerHeight ? Math.max(MARGIN, y - height) : y;
    el.style.left = `${left}px`;
    el.style.top = `${top}px`;
    el.querySelector<HTMLButtonElement>(ENABLED_ITEM)?.focus();
    const onDown = (e: MouseEvent) => {
      if (!el.contains(e.target as Node)) onClose(false);
    };
    const onBlur = () => onClose(false);
    document.addEventListener("mousedown", onDown, true);
    window.addEventListener("blur", onBlur);
    window.addEventListener("resize", onBlur);
    return () => {
      document.removeEventListener("mousedown", onDown, true);
      window.removeEventListener("blur", onBlur);
      window.removeEventListener("resize", onBlur);
    };
  }, [x, y, onClose]);

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
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
    else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onClose();
    } else if (e.key === "Tab") {
      e.preventDefault();
      onClose();
    }
  };

  return createPortal(
    <div
      ref={menu}
      className="menu__list context-menu"
      role="menu"
      aria-label={label}
      style={{ left: x, top: y }}
      onKeyDown={onKeyDown}
      onContextMenu={(e) => e.preventDefault()}
    >
      {items.map((item) => (
        <Fragment key={item.label}>
          {item.separatorBefore && <div role="separator" className="menu__separator" />}
          <button
            type="button"
            role="menuitem"
            tabIndex={-1}
            className={["menu__item", "context-menu__item", item.danger && "menu__item--danger"].filter(Boolean).join(" ")}
            disabled={item.disabled}
            onClick={() => {
              onClose();
              item.onSelect();
            }}
          >
            <span className="context-menu__icon" aria-hidden>
              {item.icon}
            </span>
            <span className="context-menu__label">{item.label}</span>
            {item.shortcut && (
              <kbd className="context-menu__shortcut" aria-hidden>
                {item.shortcut}
              </kbd>
            )}
          </button>
        </Fragment>
      ))}
    </div>,
    document.body,
  );
}
