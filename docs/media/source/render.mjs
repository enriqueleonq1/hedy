// Renders scene/index.html frame by frame and encodes the video with ffmpeg.
//   node render.mjs                         -> ../hedy-intro.mp4
//   node render.mjs --from 27 --to 36 --out build/review.mp4
//   node render.mjs --frames 3,10.5,30 --dir build/frames   (PNG stills)
//   node render.mjs --poster 10.6           -> ../poster.png
import { spawn } from "node:child_process";
import { mkdirSync, readFileSync, existsSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import { launch } from "./lib/browser.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const args = Object.fromEntries(
  process.argv
    .slice(2)
    .join(" ")
    .split("--")
    .filter(Boolean)
    .map((a) => {
      const [k, ...v] = a.trim().split(" ");
      return [k, v.join(" ") || true];
    }),
);
const FPS = Number(args.fps ?? 30);

const boxesFile = resolve(here, "build/shots/boxes.json");
const boxes = existsSync(boxesFile) ? JSON.parse(readFileSync(boxesFile, "utf8")) : {};

const browser = await launch();
const page = await browser.newPage({ viewport: { width: 1920, height: 1080 }, deviceScaleFactor: 1 });
page.on("console", (m) => m.type() === "warning" && console.warn(m.text()));
page.on("pageerror", (e) => console.error("scene error:", e.message));
await page.addInitScript((b) => {
  window.BOXES = b;
}, boxes);
await page.goto(pathToFileURL(resolve(here, "scene/index.html")).href);
await page.evaluate(() => window.ready);
const duration = await page.evaluate(() => window.DURATION);

async function frame(t) {
  await page.evaluate((time) => window.seek(time), t);
  return page.screenshot({ type: "png" });
}

if (args.frames || args.poster) {
  const { writeFileSync } = await import("node:fs");
  if (args.poster) {
    writeFileSync(resolve(here, "../poster.png"), await frame(Number(args.poster)));
    console.log("poster.png");
  } else {
    const dir = resolve(here, args.dir ?? "build/frames");
    mkdirSync(dir, { recursive: true });
    for (const t of String(args.frames).split(",").map(Number)) {
      writeFileSync(`${dir}/t${t.toFixed(2)}.png`, await frame(t));
      console.log("frame", t);
    }
  }
} else {
  const from = Number(args.from ?? 0);
  const to = Number(args.to ?? duration);
  const out = resolve(here, args.out ?? "../hedy-intro.mp4");
  mkdirSync(dirname(out), { recursive: true });
  const ff = spawn(
    "ffmpeg",
    [
      "-v", "error", "-y",
      "-f", "image2pipe", "-framerate", String(FPS), "-i", "-",
      "-c:v", "libx264", "-preset", "slow", "-crf", String(args.crf ?? 23), "-tune", "animation",
      "-pix_fmt", "yuv420p", "-movflags", "+faststart",
      out,
    ],
    { stdio: ["pipe", "inherit", "inherit"] },
  );
  const total = Math.round((to - from) * FPS);
  for (let i = 0; i < total; i++) {
    const png = await frame(from + i / FPS);
    if (!ff.stdin.write(png)) await new Promise((r) => ff.stdin.once("drain", r));
    if (i % 60 === 0) process.stdout.write(`\r${i}/${total}`);
  }
  ff.stdin.end();
  await new Promise((r) => ff.on("close", r));
  console.log(`\n${out}`);
}
await browser.close();
