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
