// The intro video as a pure function of time: `seek(t)` puts every element
// where it belongs at second `t`. `render.mjs` calls it once per frame.
// App coordinates (1600x1000 CSS px) come from build/shots/boxes.json, which
// render.mjs injects as window.BOXES; the fallbacks match the last capture.
(function () {
  const DURATION = 56;
  const WY = 505; // Stage y of the camera's center.
  const B = Object.assign(
    {
      "home.plans": { x: 352, y: 230, w: 1116, h: 333 },
      "review.plus": { x: 823, y: 463, w: 15, h: 15 },
      "review.composer": { x: 840, y: 490, w: 720, h: 77 },
      "review.add": { x: 1445, y: 575, w: 115, h: 40 },
      "review.send": { x: 757, y: 918, w: 114, h: 40 },
      "ship.accept": { x: 757, y: 918, w: 129, h: 40 },
      "ship.keep": { x: 683, y: 337, w: 194, h: 36 },
      "ship.commit": { x: 883, y: 695, w: 186, h: 40 },
      "ship.publish": { x: 983, y: 573, w: 86, h: 40 },
      "ship.create": { x: 969, y: 770, w: 100, h: 40 },
      "ship.openpr": { x: 548, y: 470, w: 124, h: 36 },
      "blame.uncommitted": { x: 961, y: 510, w: 160, h: 62 },
      "activity.limitRow": { x: 453, y: 398, w: 915, h: 69 },
    },
    window.BOXES || {},
  );
  const mid = (k) => ({ x: B[k].x + B[k].w / 2, y: B[k].y + B[k].h / 2 });
  const pad = (b, p) => ({ x: b.x - p, y: b.y - p, w: b.w + 2 * p, h: b.h + 2 * p });

  // ------------------------------------------------------------ helpers
  const clamp = (x, a = 0, b = 1) => Math.min(b, Math.max(a, x));
  const lerp = (a, b, p) => a + (b - a) * p;
  const prog = (t, a, b) => clamp((t - a) / (b - a));
  const ease = {
    out: (p) => 1 - Math.pow(1 - p, 3),
    in: (p) => p * p * p,
    inOut: (p) => (p < 0.5 ? 4 * p * p * p : 1 - Math.pow(-2 * p + 2, 3) / 2),
    back: (p) => {
      const c1 = 1.5;
      const c3 = c1 + 1;
      return 1 + c3 * Math.pow(p - 1, 3) + c1 * Math.pow(p - 1, 2);
    },
  };
  /** In over [a, a+fi], out over [b, b+fo]. */
  const window_ = (t, a, b, fi = 0.3, fo = 0.3) => Math.min(ease.out(prog(t, a, a + fi)), 1 - ease.in(prog(t, b, b + fo)));
  /** Piecewise moves: [[start, duration, value], ...], values are objects of numbers. */
  function keyed(keys, t, fn = ease.inOut) {
    let cur = { ...keys[0][2] };
    for (const [start, dur, value] of keys) {
      if (t < start) break;
      const p = dur > 0 ? fn(prog(t, start, start + dur)) : 1;
      for (const k of Object.keys(value)) cur[k] = lerp(cur[k], value[k], p);
    }
    return cur;
  }
  const $ = (id) => document.getElementById(id);
  const css = (el, props) => Object.assign(el.style, props);

  // ------------------------------------------------------------ cold open
  const TERMS = [
    { x: 70, y: 70, r: -3, at: 0.5, title: "claude — tienda", body: '<span class="p">$</span> claude\n<span class="w">Logged in as… wait, which one?</span>' },
    { x: 1290, y: 90, r: 2.5, at: 0.85, title: "codex", body: '<span class="p">$</span> codex login\n<span class="m">Opening browser…</span>' },
    { x: 40, y: 470, r: 2, at: 1.2, title: "zsh", body: '<span class="p">$</span> export CLAUDE_CONFIG_DIR=\\\n  ~/.claude-work <span class="m"># or -personal?</span>' },
    { x: 1320, y: 440, r: -2, at: 1.55, title: "gemini", body: '<span class="p">$</span> gemini\n<span class="e">✗ Not signed in</span>' },
    { x: 180, y: 780, r: -1.5, at: 1.9, title: "claude — hedy", body: '<span class="p">$</span> claude\n<span class="e">✗ Weekly limit reached</span>' },
    { x: 1180, y: 790, r: 3, at: 2.25, title: "kimi", body: '<span class="p">$</span> kimi\n&gt; where was I?' },
  ];
  for (const term of TERMS) {
    const el = document.createElement("div");
    el.className = "term";
    el.innerHTML = `<div class="term__bar"><i></i><i></i><i></i><span>${term.title}</span></div><div class="term__body">${term.body}</div>`;
    $("terms").append(el);
    term.el = el;
  }

  function coldOpen(t) {
    const away = ease.in(prog(t, 7.1, 7.8));
    for (const [i, term] of TERMS.entries()) {
      const p = prog(t, term.at, term.at + 0.45);
      const shake = t > 3 && t < 7.2 ? Math.sin(t * 23 + i * 2) * 2.2 : 0;
      const cx = term.x + 280;
      const cy = term.y + 80;
      const dx = (cx - 960) * away * 1.4;
      const dy = (cy - 540) * away * 1.4;
      css(term.el, {
        left: `${term.x}px`,
        top: `${term.y}px`,
        opacity: String(Math.min(ease.out(p), 1 - away)),
        transform: `translate(${dx + shake}px, ${dy}px) rotate(${term.r + shake * 0.3}deg) scale(${lerp(0.6, 1, ease.back(p)) * (1 - away * 0.3)})`,
      });
    }
    // The plan bar fills up and hits the limit.
    const show = window_(t, 4.8, 7.0, 0.45, 0.5);
    const fill = lerp(62, 100, ease.inOut(prog(t, 5.15, 6.0)));
    const full = fill >= 99.9;
    const shake = full ? Math.sin(t * 60) * 7 * (1 - prog(t, 6.0, 6.6)) : 0;
    css($("limit"), { opacity: String(show), transform: `translate(${shake}px, ${lerp(-40, 0, ease.out(prog(t, 4.8, 5.25)))}px)` });
    css($("limit-fill"), { width: `${fill}%`, background: full ? "var(--red)" : "var(--yellow)" });
    $("limit-pct").textContent = `${Math.round(fill)}%`;
    $("limit-pct").style.color = full ? "var(--red)" : "var(--yellow)";
    $("limit-note").style.opacity = String(ease.out(prog(t, 6.0, 6.3)));
  }

  // ------------------------------------------------------------ Blob
  const drawBlob = window.HedyBlob.mount($("blob-wrap"), "#7C5CFF");
  function blob(t) {
    // Position (center) and size on the stage.
    const pos = keyed(
      [
        [0, 0, { x: 960, y: 560, size: 330 }],
        [9.2, 0.9, { x: 680, y: 505, size: 300 }],
        [11.3, 0.7, { x: 960, y: 300, size: 120 }],
        [49.0, 0, { x: 960, y: 380, size: 380 }],
      ],
      t,
    );
    let scale = ease.back(prog(t, 0.15, 0.75));
    let opacity = Math.min(prog(t, 0.15, 0.4), 1 - prog(t, 11.4, 11.9));
    if (t >= 49) {
      scale = ease.back(prog(t, 49.1, 49.7));
      opacity = prog(t, 49.1, 49.35);
    }
    // Faces: open → dizzy (3 s) → breath (8 s) → happy (9 s); happy in the outro.
    const dz = prog(t, 2.9, 3.1) * (1 - prog(t, 7.9, 8.05));
    const cl = prog(t, 7.95, 8.1) * (1 - prog(t, 9.0, 9.15));
    const hp = t >= 49 ? 1 : prog(t, 9.0, 9.15);
    const face = { dizzy: dz, closed: cl, happy: hp, open: clamp(1 - dz - cl - hp) };
    const wobble = Math.sin(t * 5.2) * 9 * dz;
    // Deep breath: inhale, hold, exhale.
    const inhale = ease.inOut(prog(t, 8.05, 8.65)) * (1 - ease.inOut(prog(t, 8.75, 9.2)));
    // Title: a happy hop; outro: bounces.
    const hop = Math.sin(prog(t, 9.15, 9.6) * Math.PI) * 0.1;
    const bounce = t >= 49.6 ? Math.max(0, Math.sin((t - 49.6) * 4.2)) * 0.07 * Math.exp(-(t - 49.6) * 0.5) : 0;
    const lift = -(hop + bounce) * pos.size * 0.9;
    css($("blob-wrap"), {
      opacity: String(opacity),
      width: `${pos.size}px`,
      height: `${pos.size}px`,
      transform: `translate(${pos.x - pos.size / 2}px, ${pos.y - pos.size / 2 + lift}px) scale(${scale})`,
    });
    drawBlob({
      t,
      face,
      tilt: wobble,
      squash: inhale * 0.13 - hop * 0.5,
      stars: dz,
      sparkles: t >= 49 ? ease.out(prog(t, 49.6, 50.2)) : prog(t, 9.15, 9.4) * (1 - prog(t, 10.8, 11.3)),
      halo: 1 + inhale * 0.15,
    });
    // The wordmark next to Blob.
    const title = window_(t, 9.35, 11.25, 0.5, 0.45);
    css($("title"), { opacity: String(title), transform: `translate(${lerp(890, 860, ease.out(prog(t, 9.35, 9.95)))}px, 390px)` });
  }

  // ------------------------------------------------------------ app window
  const SCREENS = [
    [11.4, "home", 0],
    [19.3, "activity", 0.35],
    [26.95, "review", 0.4],
    [29.4, "review-compose-empty", 0.06],
    [30.95, "review-compose", 0],
    [31.6, "review-comment", 0.08],
    [32.7, "review-sent", 0.12],
    [35.3, "explorer-tree", 0.3],
    [37.55, "palette-empty", 0.1],
    [37.95, "palette", 0.06],
    [39.6, "blame", 0.3],
    [41.75, "review", 0.3],
    [42.75, "ship-merge", 0.15],
    [43.65, "ship-branch", 0.08],
    [44.45, "ship-publish", 0.12],
    [45.35, "ship-pr", 0.12],
    [46.35, "ship-done", 0.12],
  ];
  // Screenshots are 3200x2000: only the ones around the current moment stay
  // loaded (the screen before, the current one and the next), to keep memory low.
  const imgs = {};
  for (const name of new Set(SCREENS.map((s) => s[1]))) {
    const img = new Image();
    img.dataset.src = `../build/shots/${name}.png`;
    imgs[name] = img;
    $("screens").append(img);
  }
  const typingImg = new Image();
  typingImg.dataset.src = "../build/shots/review-compose.png";
  $("typing-clip").append(typingImg);

  function screenIndex(t) {
    let i = -1;
    for (let k = 0; k < SCREENS.length; k++) if (t >= SCREENS[k][0]) i = k;
    return i;
  }
  /** Loads what the frame at `t` needs and frees the rest. */
  async function prepare(t) {
    const i = Math.max(0, screenIndex(t));
    const near = new Set([SCREENS[Math.max(0, i - 1)][1], SCREENS[i][1], SCREENS[Math.min(SCREENS.length - 1, i + 1)][1]]);
    const wanted = Object.entries(imgs).map(([name, img]) => [img, near.has(name)]);
    wanted.push([typingImg, t > 28 && t < 31.5]);
    const loads = [];
    for (const [img, want] of wanted) {
      if (want && !img.getAttribute("src")) {
        img.src = img.dataset.src;
        loads.push(img.decode().catch(() => console.warn("missing", img.dataset.src)));
      } else if (!want && img.getAttribute("src")) {
        img.removeAttribute("src");
      }
    }
    await Promise.all(loads);
  }

  const FIT = { cx: 800, cy: 500, s: 0.86 };
  const CAMERA = [
    [0, 0, FIT],
    [12.5, 1.1, { cx: 910, cy: 400, s: 1.3 }],
    [15.0, 1.1, { cx: 505, cy: 330, s: 1.9 }],
    [17.7, 1.1, FIT],
    [19.4, 1.3, { cx: 905, cy: 470, s: 1.0 }],
    [21.3, 0.9, { cx: 1250, cy: 470, s: 1.0 }],
    [25.0, 1.0, { cx: 905, cy: 433, s: 1.75 }],
    [26.8, 1.2, { cx: 1175, cy: 440, s: 1.45 }],
    [31.55, 0.9, { cx: 1080, cy: 760, s: 1.3 }],
    [32.95, 1.2, { cx: 800, cy: 480, s: 0.95 }],
    [35.2, 0.9, { cx: 1150, cy: 400, s: 1.45 }],
    [37.4, 0.6, { cx: 800, cy: 250, s: 1.6 }],
    [39.5, 0.8, { cx: 1180, cy: 400, s: 1.4 }],
    [41.6, 0.8, { cx: 920, cy: 760, s: 1.2 }],
    [42.75, 0.7, { cx: 800, cy: 500, s: 1.45 }],
    [44.45, 0.5, { cx: 800, cy: 500, s: 1.65 }],
    [45.35, 0.5, { cx: 800, cy: 505, s: 1.3 }],
    [46.35, 0.6, { cx: 800, cy: 480, s: 1.75 }],
    [48.6, 0.9, { cx: 800, cy: 500, s: 0.62 }],
  ];
  let cam = FIT;
  const toStage = (p) => ({ x: 960 + (p.x - cam.cx) * cam.s, y: WY + (p.y - cam.cy) * cam.s + winDy });
  let winDy = 0;

  function appWindow(t) {
    cam = keyed(CAMERA, t);
    winDy = lerp(620, 0, ease.out(prog(t, 11.45, 12.35)));
    const opacity = Math.min(prog(t, 11.45, 11.8), 1 - ease.in(prog(t, 48.75, 49.45)));
    css($("win"), {
      opacity: String(opacity),
      transform: `translate(${960 - cam.cx * cam.s}px, ${WY - cam.cy * cam.s + winDy}px) scale(${cam.s})`,
    });
    // The screen on top fades in over the one before it.
    const i = screenIndex(t);
    for (const img of Object.values(imgs)) img.style.opacity = "0";
    if (i >= 0) {
      const [start, name, fade] = SCREENS[i];
      const p = fade > 0 ? ease.inOut(prog(t, start, start + fade)) : 1;
      if (i > 0 && p < 1) {
        const prev = imgs[SCREENS[i - 1][1]];
        prev.style.opacity = "1";
        prev.style.zIndex = "0";
      }
      imgs[name].style.opacity = String(p);
      imgs[name].style.zIndex = "1";
    }
    typing(t);
  }

  // Typing the review comment: the typed line is shown whole, and a patch of
  // the textarea's own color hides what is not typed yet (and the placeholder).
  function typing(t) {
    const box = B["review.composer"];
    const x = box.x + 10;
    const y = box.y + 6;
    const full = box.w - 24;
    const w = 300 * prog(t, 29.7, 30.9);
    const on = t >= 29.45 && t < 30.95;
    css($("typing"), { opacity: on ? "1" : "0", zIndex: "2" });
    css($("typing-clip"), { left: `${x}px`, top: `${y}px`, width: `${full}px`, height: "30px" });
    css(typingImg, { left: `${-x}px`, top: `${-y}px` });
    css($("typing-cover"), { left: `${x + w}px`, top: `${y}px`, width: `${full - w}px`, height: "30px" });
    const blink = Math.floor(t * 2.4) % 2 === 0 || (t > 29.7 && t < 30.9);
    css($("caret"), { left: `${x + w + 1}px`, top: `${y + 4}px`, height: "21px", opacity: blink ? "1" : "0" });
  }

  // Highlights, in app coordinates.
  const MARKS = [
    [12.9, 14.8, () => pad(B["home.plans"], 6)],
    [15.7, 17.75, () => ({ x: 12, y: 228, w: 195, h: 42 })],
    [16.75, 17.75, () => ({ x: 12, y: 184, w: 195, h: 42 })],
    [25.7, 26.9, () => pad(B["activity.limitRow"], 2)],
    [40.3, 41.5, () => ({ x: B["blame.uncommitted"].x - 10, y: B["blame.uncommitted"].y - 7, w: 1596 - B["blame.uncommitted"].x, h: 66 })],
    [46.6, 48.4, () => pad(B["ship.openpr"], 4)],
  ];
  for (const m of MARKS) {
    const el = document.createElement("div");
    el.className = "mark";
    $("marks").append(el);
    m.push(el);
  }
  function marks(t) {
    for (const [a, b, boxOf, el] of MARKS) {
      const o = window_(t, a, b, 0.3, 0.3);
      const box = boxOf();
      const pulse = 1 + 0.02 * Math.sin((t - a) * 6);
      css(el, {
        opacity: String(o),
        left: `${box.x}px`,
        top: `${box.y}px`,
        width: `${box.w}px`,
        height: `${box.h}px`,
        transform: `scale(${lerp(1.12, 1, ease.out(prog(t, a, a + 0.35))) * pulse})`,
      });
    }
  }

  // ------------------------------------------------------------ cursor
  const CURSOR = [
    { from: 28.3, to: 33.5, path: [[0, 0, { x: 1460, y: 820 }], [28.4, 0.9, mid("review.plus")], [29.6, 0.6, { x: 930, y: 560 }], [30.95, 0.55, mid("review.add")], [31.75, 0.85, mid("review.send")]], clicks: [29.35, 31.55, 32.65] },
    { from: 41.85, to: 46.55, path: [[0, 0, { x: 1150, y: 860 }], [41.95, 0.65, mid("ship.accept")], [42.9, 0.6, mid("ship.keep")], [43.75, 0.55, mid("ship.commit")], [44.6, 0.6, mid("ship.publish")], [45.5, 0.7, mid("ship.create")]], clicks: [42.7, 43.6, 44.4, 45.3, 46.3] },
  ];
  function cursor(t) {
    const c = CURSOR.find((k) => t >= k.from && t < k.to + 0.3);
    if (!c) {
      $("cursor").style.opacity = "0";
      return;
    }
    const p = keyed(c.path, t);
    const s = toStage(p);
    let press = 1;
    let ripple = 0;
    let rippleAge = 1;
    for (const click of c.clicks) {
      if (t >= click - 0.12 && t < click + 0.12) press = 0.82;
      if (t >= click && t < click + 0.45) {
        ripple = 1;
        rippleAge = (t - click) / 0.45;
      }
    }
    css($("cursor"), {
      opacity: String(window_(t, c.from, c.to, 0.25, 0.3)),
      transform: `translate(${s.x}px, ${s.y}px) scale(${press})`,
    });
    css($("ripple"), {
      opacity: String(ripple * (1 - rippleAge)),
      transform: `scale(${lerp(0.3, 1.7, ease.out(rippleAge))})`,
    });
  }

  // ------------------------------------------------------------ overlays
  const CHIPS = [
    ["Claude Code", "#BB9AF7"],
    ["Codex", "#9ECE6A"],
    ["Gemini CLI", "#7AA2F7"],
    ["Kimi Code", "#FF9E64"],
    ["Antigravity", "#73DACA"],
    ["API keys", "#E0AF68"],
  ].map(([name, color]) => {
    const el = document.createElement("div");
    el.className = "chip";
    el.innerHTML = `<i style="background:${color}"></i>${name}`;
    $("chips").append(el);
    return el;
  });
  function chips(t) {
    CHIPS.forEach((el, i) => {
      const a = 12.75 + i * 0.12;
      const o = window_(t, a, 14.75 + i * 0.04, 0.35, 0.3);
      css(el, { opacity: String(o), transform: `translateY(${lerp(-30, 0, ease.back(prog(t, a, a + 0.45)))}px)` });
    });
  }

  // Branches off main: one per task.
  const BRANCHES = [
    { y: 52, x0: 70, label: "hedy/fix-invoice-tax", color: "#BB9AF7" },
    { y: 122, x0: 150, label: "hedy/whatsapp-reminders", color: "#BB9AF7" },
    { y: 192, x0: 230, label: "tienda/cart-tests", color: "#7AA2F7" },
  ];
  $("graph").innerHTML = `<svg width="640" height="300" viewBox="0 0 640 300">
    ${BRANCHES.map(
      (b, i) => `<path class="br" data-i="${i}" d="M${b.x0} 262 C ${b.x0 + 40} 262, ${b.x0 + 20} ${b.y}, ${b.x0 + 70} ${b.y} L 620 ${b.y}" fill="none" stroke="${b.color}" stroke-width="5" stroke-linecap="round"/>
      ${[0, 1, 2].map((j) => `<circle class="cm" data-i="${i}" data-j="${j}" cx="${b.x0 + 130 + j * 110 + i * 10}" cy="${b.y}" r="10" fill="#16161e" stroke="${b.color}" stroke-width="4"/>`).join("")}
      <text class="bl" data-i="${i}" x="${b.x0 + 80}" y="${b.y - 16}">${b.label}</text>`,
    ).join("")}
    <path id="main-line" d="M20 262 L 620 262" stroke="#9AA5CE" stroke-width="6" stroke-linecap="round"/>
    <text class="lbl-main" x="20" y="296">main · your folder</text>
    <g id="untouched" opacity="0"><rect x="372" y="272" width="250" height="36" rx="18" fill="#20303B"/><text x="392" y="297" style="fill:#9ECE6A">✓ untouched</text></g>
  </svg>`;
  const brs = [...document.querySelectorAll("#graph .br")];
  for (const el of brs) {
    const len = el.getTotalLength();
    el.dataset.len = String(len);
    el.style.strokeDasharray = String(len);
  }
  function graph(t) {
    const o = window_(t, 21.55, 24.9, 0.4, 0.4);
    css($("graph"), { opacity: String(o), transform: `translateX(${lerp(60, 0, ease.out(prog(t, 21.55, 22.1)))}px)` });
    brs.forEach((el) => {
      const i = Number(el.dataset.i);
      const p = ease.out(prog(t, 21.8 + i * 0.25, 22.6 + i * 0.25));
      el.style.strokeDashoffset = String(Number(el.dataset.len) * (1 - p));
    });
    for (const el of document.querySelectorAll("#graph .cm")) {
      const i = Number(el.dataset.i);
      const j = Number(el.dataset.j);
      const a = 22.3 + i * 0.25 + j * 0.18;
      el.setAttribute("opacity", String(prog(t, a, a + 0.15)));
      const k = ease.back(prog(t, a, a + 0.35));
      el.setAttribute("r", String(10 * k));
    }
    for (const el of document.querySelectorAll("#graph .bl")) {
      const i = Number(el.dataset.i);
      el.setAttribute("opacity", String(prog(t, 22.1 + i * 0.25, 22.5 + i * 0.25)));
    }
    const u = ease.out(prog(t, 23.45, 23.8));
    $("untouched").setAttribute("opacity", String(u));
    $("main-line").setAttribute("stroke", u > 0.5 ? "#9ECE6A" : "#9AA5CE");
  }

  function keys(t) {
    const o = window_(t, 37.45, 39.25, 0.3, 0.3);
    const pressed = t >= 37.75 && t < 37.95;
    css($("keys"), {
      opacity: String(o),
      transform: `translate(${960 - 150}px, ${700 + lerp(30, 0, ease.back(prog(t, 37.45, 37.8)))}px)`,
    });
    const [ctrl, , p] = $("keys").children;
    for (const el of [ctrl, p]) {
      css(el, {
        transform: pressed ? "translateY(5px)" : "none",
        borderBottomWidth: pressed ? "2px" : "7px",
        background: pressed ? "#3a2f7a" : "#24263a",
      });
    }
  }

  // ------------------------------------------------------------ captions
  // `*word*` is violet, `!word!` red, `+word+` green. `at` sets each word's
  // entry time (relative to the start); otherwise they come in one by one.
  const CAPTIONS = [
    { a: 0.7, b: 2.7, text: "Three AIs. Five accounts.", y: 1000 },
    { a: 2.9, b: 4.75, text: "Which account was *this*?", y: 1000 },
    { a: 5.0, b: 7.2, text: "…and the limit hits !100%.!", y: 1000 },
    { a: 8.0, b: 8.95, text: "(deep breath)", y: 1000, small: true },
    { a: 9.6, b: 11.3, text: "Meet *Hedy.*", y: 880 },
    { a: 12.4, b: 14.9, text: "Every AI, every account, *one app.*" },
    { a: 15.2, b: 16.5, text: "Work stuff on the *work* account." },
    { a: 16.65, b: 17.75, text: "Yours on *yours.*" },
    { a: 17.9, b: 19.35, text: "Hedy never touches your *tokens.*" },
    { a: 19.7, b: 21.5, text: "Run tasks in *parallel.*" },
    { a: 21.7, b: 23.3, text: "Each on its own *branch.*" },
    { a: 23.45, b: 25.0, text: "Your folder stays *untouched.*" },
    { a: 25.35, b: 26.9, text: "Limit hit? It *waits.* No silent switch." },
    { a: 27.3, b: 29.3, text: "Review it like a *pull request.*" },
    { a: 29.5, b: 31.4, text: "Comment on any *line.*" },
    { a: 31.6, b: 33.25, text: "Send it back to the *AI.*" },
    { a: 33.45, b: 35.1, text: "One message, *every* comment." },
    { a: 35.45, b: 37.35, text: "Browse the *code.*" },
    { a: 37.6, b: 39.45, text: "Jump anywhere with *Ctrl+P.*" },
    { a: 39.7, b: 41.55, text: "*Blame,* right there." },
    { a: 41.85, b: 43.75, text: "Happy with it? *Ship* it." },
    { a: 44.25, b: 48.45, text: "+Commit.+ +Push.+ +Pull+ +request.+", at: [0.2, 1.1, 2.1, 2.16] },
  ];
  let shownCaption = null;
  function captions(t) {
    const el = $("caption");
    const c = CAPTIONS.find((k) => t >= k.a && t < k.b + 0.35);
    if (!c) {
      el.style.opacity = "0";
      return;
    }
    if (shownCaption !== c) {
      shownCaption = c;
      el.className = c.small ? "small" : "";
      el.innerHTML = c.text
        .split(" ")
        .map((w) => {
          let cls = "word";
          let word = w;
          const m = w.match(/^([*!+])(.*)\1$/) || w.match(/^([*!+])(.*)$/) || w.match(/^(.*?)([*!+])$/);
          if (m) {
            const mark = /[*!+]/.exec(w)[0];
            word = w.split(mark).join("");
            cls += mark === "*" ? " acc" : mark === "!" ? " bad" : " ok";
          }
          return `<span class="${cls}">${word}</span>`;
        })
        .join("");
    }
    const words = [...el.children];
    const out = ease.in(prog(t, c.b, c.b + 0.3));
    words.forEach((w, i) => {
      const at = c.a + (c.at ? c.at[i] : i * 0.07);
      const p = prog(t, at, at + 0.45);
      if (c.at) w.style.display = t >= at ? "inline-block" : "none";
      css(w, {
        opacity: String(Math.min(ease.out(prog(t, at, at + 0.25)), 1 - out)),
        transform: `translateY(${lerp(34, 0, ease.back(p)) - out * 14}px) scale(${lerp(0.85, 1, ease.out(p))})`,
      });
    });
    const width = el.offsetWidth;
    const height = el.offsetHeight;
    const y = c.y ?? 990;
    const pill = Math.min(ease.out(prog(t, c.a - 0.05, c.a + 0.25)), 1 - out);
    css(el, {
      opacity: String(pill),
      transform: `translate(${960 - width / 2}px, ${y - height / 2}px) scale(${lerp(0.94, 1, ease.out(prog(t, c.a, c.a + 0.3)))})`,
    });
  }

  // ------------------------------------------------------------ outro
  const OUTRO = ["Your AIs.", "Your accounts.", "*One place.*"];
  $("outro-line").innerHTML = OUTRO.map((w) => {
    const acc = w.startsWith("*");
    return `<span class="word${acc ? " acc" : ""}">${w.replace(/\*/g, "")}</span>`;
  }).join("");
  function outro(t) {
    [...$("outro-line").children].forEach((w, i) => {
      const a = 49.9 + i * 0.45;
      const p = prog(t, a, a + 0.5);
      css(w, { opacity: String(ease.out(prog(t, a, a + 0.3))), transform: `translateY(${lerp(40, 0, ease.back(p))}px)` });
    });
    const u = prog(t, 51.6, 52.1);
    css($("outro-url"), { opacity: String(ease.out(u)), transform: `translateX(-50%) translateY(${lerp(24, 0, ease.out(u))}px)` });
    $("outro-meta").style.opacity = String(ease.out(prog(t, 52.2, 52.7)));
  }

  // ------------------------------------------------------------ frame
  function seek(t) {
    document.getElementById("bg").style.setProperty("--gx", `${28 + Math.sin(t * 0.25) * 6}%`);
    document.getElementById("bg").style.setProperty("--gy", `${18 + Math.cos(t * 0.2) * 5}%`);
    coldOpen(t);
    blob(t);
    appWindow(t);
    marks(t);
    chips(t);
    graph(t);
    keys(t);
    cursor(t);
    captions(t);
    outro(t);
    $("fade").style.opacity = String(1 - ease.out(prog(t, 0, 0.45)));
  }

  window.DURATION = DURATION;
  window.seek = async (t) => {
    await prepare(t);
    seek(t);
  };
  window.ready = document.fonts.ready.then(() => window.seek(0));
  // Opened by hand: ?t=12.5 shows that moment, ?play plays it in real time.
  const params = new URLSearchParams(location.search);
  if (params.has("t")) window.ready.then(() => window.seek(Number(params.get("t"))));
  if (params.has("play")) {
    window.ready.then(() => {
      const start = performance.now() - Number(params.get("play") || 0) * 1000;
      const tick = () => {
        const t = (performance.now() - start) / 1000;
        window.seek(t % DURATION).then(() => requestAnimationFrame(tick));
      };
      tick();
    });
  }
})();
