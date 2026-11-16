// The README's looping preview: a few beats of the video, cut together.
//   node gif.mjs   (reads ../hedy-intro.mp4, writes ../hedy-intro.gif)
import { spawnSync } from "node:child_process";
import { statSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const input = resolve(here, "../hedy-intro.mp4");
const output = resolve(here, "../hedy-intro.gif");
const WIDTH = Number(process.env.WIDTH ?? 800);
const FPS = Number(process.env.FPS ?? 12);

// [from, to] in seconds: title, parallel tasks, review, ship, outro.
const CUTS = [
  [9.4, 11.3],
  [19.6, 24.9],
  [27.4, 33.35],
  [42.1, 48.6],
  [49.0, 54.6],
];

const parts = CUTS.map(([a, b], i) => `[0:v]trim=${a}:${b},setpts=PTS-STARTPTS[c${i}]`);
const concat = `${CUTS.map((_, i) => `[c${i}]`).join("")}concat=n=${CUTS.length}:v=1:a=0[cat]`;
const look = `[cat]fps=${FPS},scale=${WIDTH}:-1:flags=lanczos,split[a][b]`;
const palette = "[a]palettegen=max_colors=128:stats_mode=diff[p]";
const use = "[b][p]paletteuse=dither=none:diff_mode=rectangle";
const graph = [...parts, concat, look, palette, use].join(";");

const r = spawnSync("ffmpeg", ["-v", "error", "-y", "-i", input, "-filter_complex", graph, "-loop", "0", output], {
  stdio: "inherit",
});
if (r.status !== 0) process.exit(r.status ?? 1);
console.log(output, `${(statSync(output).size / 1024 / 1024).toFixed(2)} MB`);
