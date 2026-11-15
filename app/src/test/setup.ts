import "@testing-library/jest-dom/vitest";
import { cleanup, configure } from "@testing-library/react";
import { afterEach } from "vitest";

afterEach(() => {
  cleanup();
  // The app keeps the current screen in the URL hash.
  window.history.replaceState(null, "", "/");
  // Conveniences remembered per viewer (like each project's composer mode) start fresh.
  try {
    localStorage.clear();
    sessionStorage.clear();
  } catch {
    // No storage: nothing to clear.
  }
});

// `findBy…` waits 1 s by default; whole-app tests need longer on a busy machine.
configure({ asyncUtilTimeout: 5000 });

// jsdom has no layout: CodeMirror measures text ranges, which jsdom leaves out.
if (typeof Range !== "undefined") {
  const empty = () => ({ x: 0, y: 0, width: 0, height: 0, top: 0, left: 0, right: 0, bottom: 0, toJSON: () => ({}) }) as DOMRect;
  Range.prototype.getClientRects ??= () => Object.assign([], { item: () => null }) as unknown as DOMRectList;
  Range.prototype.getBoundingClientRect ??= empty;
}
