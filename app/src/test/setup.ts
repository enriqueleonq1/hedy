import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

afterEach(() => {
  cleanup();
  // The app keeps the current screen in the URL hash.
  window.history.replaceState(null, "", "/");
});
