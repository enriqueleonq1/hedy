/** The palette's shortcut as shown: "⌘K" on a Mac, "Ctrl K" elsewhere. */
export function paletteKeys(): string {
  const platform = typeof navigator === "undefined" ? "" : `${navigator.platform} ${navigator.userAgent}`;
  return /Mac|iPhone|iPad/.test(platform) ? "⌘K" : "Ctrl K";
}
