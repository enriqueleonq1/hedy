function isMac(): boolean {
  const platform = typeof navigator === "undefined" ? "" : `${navigator.platform} ${navigator.userAgent}`;
  return /Mac|iPhone|iPad/.test(platform);
}

/** The modifier of the app's shortcuts as shown: "⌘" on a Mac, "Ctrl" elsewhere. */
export function modKey(): string {
  return isMac() ? "⌘" : "Ctrl";
}

/** The Alt key as shown: "⌥" on a Mac, "Alt" elsewhere. */
export function altKey(): string {
  return isMac() ? "⌥" : "Alt";
}

/** The palette's shortcut as shown: "⌘K" on a Mac, "Ctrl K" elsewhere. */
export function paletteKeys(): string {
  return isMac() ? "⌘K" : "Ctrl K";
}
