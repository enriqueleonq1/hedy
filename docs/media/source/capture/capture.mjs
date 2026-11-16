// Captures the app screens the intro video and the README stills are made of.
// Needs the dev server from `vite.config.mjs` running (port 5237 by default).
//   node capture/capture.mjs            -> build/shots/*.png + build/shots/boxes.json
// Every screen is 1600x1000 CSS px at 2x, dark theme, English UI.
import { spawnSync } from "node:child_process";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { launch } from "../lib/browser.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const out = resolve(here, "../build/shots");
mkdirSync(out, { recursive: true });
const base = `http://localhost:${process.env.PORT ?? 5237}/`;
const W = 1600;
const H = 1000;

const browser = await launch();
const boxes = {};

/** A fresh page (fresh sample data) at `hash`, optionally in another theme. */
async function open(hash, { theme = "dark", panel = null } = {}) {
  const context = await browser.newContext({
    viewport: { width: W, height: H },
    deviceScaleFactor: 2,
    reducedMotion: "reduce",
  });
  await context.addInitScript((panelWidth) => {
    try {
      localStorage.clear();
      if (panelWidth) localStorage.setItem("hedy.sidePanel.width", String(panelWidth));
    } catch {
      // Storage may be unavailable; the defaults are fine.
    }
    // No spell-check squiggles and no blinking caret in the pictures.
    const style = document.createElement("style");
    style.textContent = "*{caret-color:transparent!important}";
    document.addEventListener("DOMContentLoaded", () => document.head.append(style));
    new MutationObserver(() => {
      for (const el of document.querySelectorAll("textarea,input,[contenteditable]")) el.spellcheck = false;
    }).observe(document, { subtree: true, childList: true });
  }, panel);
  const page = await context.newPage();
  page.on("pageerror", (e) => console.warn("page error:", e.message));
  await page.goto(`${base}?lang=en&theme=${theme}${hash}`);
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(1200);
  return page;
}

/** Saves the page as `name.png`, with the pointer parked where it hovers nothing. */
async function shot(page, name) {
  await page.mouse.move(W / 2, 4);
  await page.waitForTimeout(350);
  // In two halves, stacked with ffmpeg: a full 2x capture of some dialogs
  // (the Ctrl+P palette) crashes the headless renderer, halves never do.
  const half = H / 2;
  const parts = [0, 1].map((i) => `${out}/.${name}.${i}.png`);
  for (const [i, path] of parts.entries()) {
    await page.screenshot({ path, clip: { x: 0, y: i * half, width: W, height: half } });
  }
  const ff = spawnSync("ffmpeg", ["-v", "error", "-y", "-i", parts[0], "-i", parts[1], "-filter_complex", "vstack", `${out}/${name}.png`]);
  if (ff.status !== 0) throw new Error(`ffmpeg vstack failed: ${ff.stderr}`);
  for (const path of parts) rmSync(path);
  console.log("shot", name);
}

/** Records the CSS-px box of the first match, for the camera and the pointer. */
async function box(page, name, locator) {
  const b = await locator
    .first()
    .boundingBox({ timeout: 3000 })
    .catch(() => null);
  if (!b) {
    console.warn(`no box for ${name}`);
    return;
  }
  boxes[name] = { x: Math.round(b.x), y: Math.round(b.y), w: Math.round(b.width), h: Math.round(b.height) };
}

const text = (page, t) => page.getByText(t);

/** The box of the nearest ancestor of `t`'s text that is at least `minHeight` tall (a card, a row). */
async function boxUp(page, name, t, minHeight) {
  const b = await text(page, t)
    .first()
    .evaluate((el, min) => {
      let node = el;
      while (node.parentElement && node.getBoundingClientRect().height < min) node = node.parentElement;
      const r = node.getBoundingClientRect();
      return { x: Math.round(r.x), y: Math.round(r.y), w: Math.round(r.width), h: Math.round(r.height) };
    }, minHeight)
    .catch(() => null);
  if (b) boxes[name] = b;
  else console.warn(`no box for ${name}`);
}

// Home: plans, the sidebar's projects and accounts.
{
  const page = await open("");
  await box(page, "home.plans", page.locator(".plan-card, [class*=plan]").first().locator("xpath=.."));
  await box(page, "home.projects", text(page, "tienda"));
  await box(page, "home.accounts", text(page, "Claude work"));
  await shot(page, "home");
  await page.context().close();
}
{
  const page = await open("", { theme: "light" });
  await shot(page, "home-light");
  await page.context().close();
}

// The task board.
{
  const page = await open("#activity");
  await box(page, "activity.limit", text(page, "Blocked by a limit"));
  await box(page, "activity.running", text(page, "Check the cart tests"));
  await boxUp(page, "activity.limitRow", "Translate the confirmation emails", 60);
  await shot(page, "activity");
  await page.context().close();
}

// The running task with its queue.
{
  const page = await open("#p=%2Fcode%2Ftienda&t=s4");
  await shot(page, "running");
  await page.context().close();
}

// Review: comment on a line and send it back.
const COMMENT = "Also test an invoice with three lines.";
{
  const page = await open("#p=%2Fcode%2Fhedy&t=s5", { panel: 860 });
  await box(page, "review.tasks", page.getByRole("button", { name: /Tasks \(5\)|^5$/ }));
  await box(page, "review.diff", page.locator(".rv-filehead, .rd-gap__btn").first());
  const add = page.locator('button[aria-label="Comment on line 18"]');
  await box(page, "review.plus", add);
  await shot(page, "review");
  await add.click();
  await page.waitForTimeout(300);
  await box(page, "review.composer", page.locator("textarea").filter({ hasNot: page.locator(".composer__input") }).last());
  await shot(page, "review-compose-empty");
  await page.keyboard.type(COMMENT);
  await box(page, "review.add", page.getByRole("button", { name: "Add comment" }));
  await shot(page, "review-compose");
  await page.keyboard.press("Control+Enter");
  await page.waitForTimeout(300);
  await box(page, "review.send", page.getByRole("button", { name: "Send to the AI", exact: true }));
  await shot(page, "review-comment");
  await page.getByRole("button", { name: "Send to the AI", exact: true }).click();
  await page.waitForTimeout(2500);
  await shot(page, "review-sent");
  await page.context().close();
}

// Explorer, Ctrl+P and blame.
{
  const page = await open("#p=%2Fcode%2Fhedy&t=s5", { panel: 900 });
  await page.keyboard.press("Control+Shift+E");
  await page.locator('button[aria-label="Changed only"]').click();
  await page.waitForTimeout(400);
  await box(page, "explorer.panel", page.locator(".vtree__row").first());
  await shot(page, "explorer-tree");
  await page.keyboard.press("Control+p");
  await page.waitForTimeout(300);
  await shot(page, "palette-empty");
  await page.keyboard.type("tax");
  await page.waitForTimeout(300);
  await box(page, "palette.input", page.locator("[role=dialog] input, .palette input").first());
  await shot(page, "palette");
  await page.keyboard.press("Enter");
  await page.waitForTimeout(600);
  await shot(page, "explorer-file");
  const blame = page.locator('button[aria-label="Blame: who changed each line"]');
  await box(page, "explorer.blame", blame);
  await blame.click();
  await page.waitForTimeout(700);
  await box(page, "blame.uncommitted", text(page, "Not committed"));
  await shot(page, "blame");
  await page.context().close();
}

// Ship: commit on the branch, publish, pull request.
{
  const page = await open("#p=%2Fcode%2Fhedy&t=s5", { panel: 860 });
  const accept = page.getByRole("button", { name: "Accept changes", exact: true });
  await box(page, "ship.accept", accept);
  await accept.click();
  await page.waitForTimeout(400);
  const keep = page.getByText("Keep on its branch (PR)", { exact: true });
  await box(page, "ship.keep", keep);
  await shot(page, "ship-merge");
  await keep.click();
  const commit = page.getByRole("button", { name: "Commit on the branch" });
  await box(page, "ship.commit", commit);
  await shot(page, "ship-branch");
  await commit.click();
  await page.waitForTimeout(900);
  const publish = page.locator("[role=dialog] .btn--primary");
  await box(page, "ship.publish", publish);
  await shot(page, "ship-publish");
  await publish.click();
  await page.waitForTimeout(2500);
  const create = page.getByRole("button", { name: "Create PR", exact: true });
  await box(page, "ship.create", create);
  await shot(page, "ship-pr");
  await create.click();
  await page.waitForTimeout(2500);
  await box(page, "ship.openpr", page.getByText(/Open PR #/));
  await shot(page, "ship-done");
  await page.context().close();
}

writeFileSync(`${out}/boxes.json`, JSON.stringify(boxes, null, 1));
await browser.close();
console.log("boxes", boxes);
