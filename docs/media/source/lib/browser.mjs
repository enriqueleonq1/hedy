// Headless Chromium through playwright-core. Set CHROME to pick the binary;
// otherwise the newest Playwright headless shell (or Chromium) under
// ~/.cache/ms-playwright is used (the headless shell first: it is lighter).
import { existsSync, mkdirSync, readdirSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { chromium } from "playwright-core";

const CANDIDATES = [
  [/^chromium_headless_shell-(\d+)$/, "chrome-headless-shell-linux64/chrome-headless-shell"],
  [/^chromium-(\d+)$/, "chrome-linux64/chrome"],
];

function findChrome() {
  if (process.env.CHROME) return process.env.CHROME;
  const cache = join(homedir(), ".cache/ms-playwright");
  const dirs = existsSync(cache) ? readdirSync(cache) : [];
  for (const [pattern, bin] of CANDIDATES) {
    const found = dirs
      .filter((d) => pattern.test(d))
      .sort((a, b) => Number(b.match(pattern)[1]) - Number(a.match(pattern)[1]))
      .map((d) => join(cache, d, bin))
      .find((path) => existsSync(path));
    if (found) return found;
  }
  throw new Error("No Chromium found: set CHROME=/path/to/chrome");
}

export function launch() {
  // The browser profile goes under build/, not /tmp: a full /tmp makes Chromium
  // crash on large images without saying why.
  const tmp = resolve(dirname(fileURLToPath(import.meta.url)), "../build/tmp");
  mkdirSync(tmp, { recursive: true });
  process.env.TMPDIR = tmp;
  return chromium.launch({ executablePath: findChrome(), args: ["--force-color-profile=srgb", "--hide-scrollbars"] });
}
