// The side panel's shortcuts (VS Code's): Ctrl/⌘ Shift E, F and G focus the
// explorer, the search and the review. By physical key, so layouts agree.

import { modKey } from "../components/palette/keys";

export type PanelShortcut = "explorer" | "search" | "review";

const CODES: Record<string, PanelShortcut> = { KeyE: "explorer", KeyF: "search", KeyG: "review" };
const LETTERS: Record<PanelShortcut, string> = { explorer: "E", search: "F", review: "G" };

/** Which panel view a key press asks for, or `null`. */
export function panelShortcut(e: KeyboardEvent): PanelShortcut | null {
  if (!(e.ctrlKey || e.metaKey) || !e.shiftKey || e.altKey) return null;
  return CODES[e.code] ?? CODES[`Key${e.key.toUpperCase()}`] ?? null;
}

/** As shown in tooltips: "Ctrl+Shift+E" / "⌘⇧E". */
export function shortcutLabel(view: PanelShortcut): string {
  const mod = modKey();
  return mod === "⌘" ? `⌘⇧${LETTERS[view]}` : `${mod}+Shift+${LETTERS[view]}`;
}

/** For `aria-keyshortcuts`. */
export function shortcutAria(view: PanelShortcut): string {
  return `Control+Shift+${LETTERS[view]} Meta+Shift+${LETTERS[view]}`;
}

/** "Ctrl+F" / "⌘F", for a key with the modifier. */
export function modLabel(key: string): string {
  const mod = modKey();
  return mod === "⌘" ? `⌘${key}` : `${mod}+${key}`;
}
