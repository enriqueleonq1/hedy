// What the keyboard hints read like on this computer: ⌘ on macOS, Ctrl elsewhere.

type NavigatorWithData = Navigator & { userAgentData?: { platform?: string } };

/** Whether the app runs on macOS (or another Apple system with a ⌘ key). */
export function isMac(): boolean {
  if (typeof navigator === "undefined") return false;
  const nav = navigator as NavigatorWithData;
  const platform = nav.userAgentData?.platform || nav.platform || nav.userAgent || "";
  return /mac|iphone|ipad/i.test(platform);
}

/** Key labels for the hints next to buttons (key caps, the same in every language). */
export function keyLabels(mac = isMac()) {
  return {
    /** Enter (Return on a Mac). */
    enter: mac ? "⏎" : "Enter",
    /** Shift + Enter: a new line where Enter sends. */
    newline: mac ? "⇧⏎" : "Shift+Enter",
    /** Ctrl/⌘ + Enter: sends the composer. */
    submit: mac ? "⌘⏎" : "Ctrl+Enter",
    /** Ctrl/⌘ + a digit: jumps to that task. */
    jump: (n: number) => (mac ? `⌘${n}` : `Ctrl+${n}`),
  };
}
