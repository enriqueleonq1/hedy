# Intro video and README stills

Source of `docs/media/hedy-intro.mp4`, `hedy-intro.gif`, `poster.png` and the
stills (`home.png`, `review.png`, `explorer.png`, `ship.png`). The video is an
HTML page (`scene/`) whose `seek(t)` places everything at second `t`; it is
rendered frame by frame with headless Chromium and encoded with ffmpeg. The app
screens in it are real captures of the app with its sample data.

## Needs

- `npm install` in `app/` (the dev server and the fonts come from there)
- `npm install` here (only `playwright-core`; it is not added to the app)
- A Chromium: the newest one under `~/.cache/ms-playwright` (the headless shell
  is preferred), or `CHROME=/path/to/chrome`
- `ffmpeg` and ImageMagick (`magick`)

## Re-render

From this folder:

```fish
npm run serve &        # the app's dev server on :5237, sample data in English
npm run capture        # app screens -> build/shots/*.png (+ boxes.json)
npm run render         # -> ../hedy-intro.mp4 (56 s, 1920x1080, 30 fps; a few minutes)
npm run poster         # -> ../poster.png (the title card)
npm run gif            # -> ../hedy-intro.gif (a shorter cut of the video)
npm run stills         # -> ../home.png, review.png, explorer.png, ship.png
```

`build/` (captures, browser profile, test frames) is not committed.

Checking a moment without rendering everything:

```fish
node render.mjs --frames 12.5,30,47 --dir build/frames   # PNGs of those seconds
node render.mjs --from 27 --to 36 --out build/review.mp4 # one stretch
```

## What lives where

- `capture/vite.config.mjs`: the app's dev server plus `sample-en.mjs`, an
  overlay that turns the sample data (task titles, the sample repo's code,
  account names) into English while capturing. The app's own sample data is not
  changed. If a text in the overlay is no longer in the source, Vite prints a
  `sample-en: not found` warning.
- `capture/capture.mjs`: drives the app (review comment, Ctrl+P, blame, the
  accept → publish → pull request flow) and saves each screen at 2x, plus the
  boxes of the things the camera and the pointer go to.
- `scene/`: the timeline. `scene.js` holds the beats (cold open, title,
  accounts, parallel tasks, review, explorer, ship, outro), the camera keys, the
  pointer path and the captions; `blob.js` draws Blob from the same shapes as
  `app/src/components/Mascot`. Open `scene/index.html?t=30` in a browser to see
  a moment, or `?play` to play it (after `npm run capture`).
- `render.mjs`, `gif.mjs`, `capture/stills.mjs`: output.

If the app's layout changes, run `capture` again: the pointer and the
highlights follow `boxes.json`; the camera keys in `scene.js` may need a nudge.
