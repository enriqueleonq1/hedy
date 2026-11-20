// Arrow keys inside a group of tabs or radios (WAI-ARIA, automatic
// activation): ←/→ (and ↑/↓) move to the next or previous item and choose it,
// Home/End go to the ends. Each item sets `tabIndex={selected ? 0 : -1}`, so
// Tab enters the group on the chosen one and leaves it with the next Tab.

import type { KeyboardEvent } from "react";

const ITEMS = '[role="tab"]:not(:disabled), [role="radio"]:not(:disabled)';

/** The group's `onKeyDown`: put it on the element with `role="tablist"` or `role="radiogroup"`. */
export function rovingKeyDown(e: KeyboardEvent<HTMLElement>) {
  const items = [...e.currentTarget.querySelectorAll<HTMLElement>(ITEMS)];
  const index = items.indexOf(document.activeElement as HTMLElement);
  if (index < 0) return;
  let next = -1;
  if (e.key === "ArrowRight" || e.key === "ArrowDown") next = (index + 1) % items.length;
  else if (e.key === "ArrowLeft" || e.key === "ArrowUp") next = (index - 1 + items.length) % items.length;
  else if (e.key === "Home") next = 0;
  else if (e.key === "End") next = items.length - 1;
  const target = items[next];
  if (!target) return;
  e.preventDefault();
  target.focus();
  target.click();
}
