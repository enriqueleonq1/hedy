// The README stills: crops of the 2x captures, quantized to keep them small.
//   node capture/stills.mjs   (needs ImageMagick's `magick`)
import { spawnSync } from "node:child_process";
import { statSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const shots = resolve(here, "../build/shots");
const media = resolve(here, "../..");

// [output, capture, crop in CSS px of the 1600x1000 capture, output scale]
const STILLS = [
  ["home.png", "home", { x: 0, y: 0, w: 1600, h: 578 }, 1.5],
  ["review.png", "review-comment", { x: 740, y: 62, w: 860, h: 912 }, 2],
  ["explorer.png", "blame", { x: 701, y: 62, w: 899, h: 640 }, 2],
  ["ship.png", "ship-pr", { x: 496, y: 166, w: 608, h: 676 }, 2],
];

for (const [name, shot, c, scale] of STILLS) {
  const out = resolve(media, name);
  const crop = `${c.w * 2}x${c.h * 2}+${c.x * 2}+${c.y * 2}`;
  const resize = scale === 2 ? [] : ["-resize", `${(scale / 2) * 100}%`];
  const r = spawnSync("magick", [
    resolve(shots, `${shot}.png`),
    "-crop", crop, "+repage",
    ...resize,
    "-strip", "-dither", "None", "-colors", "256",
    "-define", "png:compression-level=9",
    `PNG8:${out}`,
  ]);
  if (r.status !== 0) throw new Error(`magick failed for ${name}: ${r.stderr}`);
  console.log(name, `${Math.round(statSync(out).size / 1024)} KB`);
}
