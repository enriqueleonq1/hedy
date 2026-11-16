// Blob, the mascot, drawn as in app/src/components/Mascot/Mascot.tsx (same
// body path, droplets, faces and goo filter) but driven frame by frame from a
// time value instead of CSS animations, so every render is identical.
(function () {
  const BODY =
    "M80 48.5C97.5 48.5 111 58.5 114.8 71C116.8 77.5 116.4 83 116.5 88.5C116.6 93.5 115.6 96.8 117.5 100.5" +
    "C119.5 105 118 109 112.5 110C106.5 111 102 112 97 114.5C91 117.5 84 118.5 77 117.5C70 116.5 65 113 59 111.5" +
    "C53 110 46.5 110.5 44.5 106C43 102.5 46.4 98 46 93C45.6 89 44.6 83.5 45.4 77C47 60.5 61.5 48.5 80 48.5Z";
  const SPARKLE = "M0-7C.6-2.2 2.2-.6 7 0 2.2.6.6 2.2 0 7-.6 2.2-2.2.6-7 0-2.2-.6-.6-2.2 0-7z";
  const FACE = "#1A1B26";
  const LINE = `fill="none" stroke="${FACE}" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"`;
  const eye = (x) =>
    `<ellipse cx="${x}" cy="88.6" rx="4.8" ry="4.8" fill="${FACE}"/><circle cx="${x + 1.4}" cy="87.2" r="1.65" fill="#fff"/>`;
  // A small spiral centered on (x, 88.6), for the dizzy eyes.
  const spiral = (x) =>
    `<g class="spin" data-x="${x}"><path ${LINE} stroke-width="1.7" d="M${x} 88.6a1.3 1.3 0 0 1 2.6 0a2.6 2.6 0 0 1-5.2 0a3.9 3.9 0 0 1 7.8 0a5.2 5.2 0 0 1-10.4 0"/></g>`;

  function markup(color) {
    return `<svg viewBox="0 0 160 160" xmlns="http://www.w3.org/2000/svg">
  <defs>
    <filter id="goo" x="-30%" y="-30%" width="160%" height="160%" color-interpolation-filters="sRGB">
      <feGaussianBlur in="SourceGraphic" stdDeviation="4" result="blur"/>
      <feColorMatrix in="blur" mode="matrix" values="1 0 0 0 0  0 1 0 0 0  0 0 1 0 0  0 0 0 20 -11.5"/>
    </filter>
    <linearGradient id="shade" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#fff" stop-opacity="0.14"/>
      <stop offset="0.55" stop-color="#fff" stop-opacity="0"/>
    </linearGradient>
  </defs>
  <circle class="halo" cx="80" cy="88.6" r="56.6" fill="${color}" opacity="0.18"/>
  <g class="stars">${[0, 1, 2].map(() => `<path d="${SPARKLE}" fill="#E0AF68"/>`).join("")}</g>
  <g class="body">
    <g filter="url(#goo)">
      <path d="${BODY}" fill="${color}"/>
      <circle class="d1" cx="119" cy="103" r="6" fill="${color}"/>
      <circle class="d2" cx="43.5" cy="106" r="5.5" fill="${color}"/>
    </g>
    <path d="${BODY}" fill="${color}"/>
    <circle class="d1" cx="119" cy="103" r="6" fill="${color}"/>
    <circle class="d2" cx="43.5" cy="106" r="5.5" fill="${color}"/>
    <path d="${BODY}" fill="url(#shade)"/>
    <ellipse cx="66.3" cy="66.3" rx="10.3" ry="5.7" fill="#fff" opacity="0.3"/>
    <g class="cheeks" fill="#F7768E" opacity="0.4"><circle cx="60.9" cy="96.6" r="4.3"/><circle cx="98.6" cy="96.6" r="4.3"/></g>
    <g class="f-open"><g class="eyes">${eye(67.4)}${eye(90.4)}</g><path ${LINE} d="M72.4 98.6q6.6 6.4 13.4 0"/></g>
    <g class="f-dizzy">${spiral(67.4)}${spiral(90.4)}<path ${LINE} d="M73 101.5q3.25-2.4 6.5 0t6.5 0"/></g>
    <g class="f-closed"><path ${LINE} d="M62.6 89.4q4.8 3.4 9.6 0"/><path ${LINE} d="M85.6 89.4q4.8 3.4 9.6 0"/><ellipse cx="79" cy="101.4" rx="3.2" ry="3.4" fill="${FACE}"/></g>
    <g class="f-happy"><path ${LINE} d="M62.6 90q4.8-5.8 9.6 0"/><path ${LINE} d="M85.6 90q4.8-5.8 9.6 0"/><path d="M71.2 97q7.8 12 15.6 0z" fill="${FACE}"/><ellipse cx="79" cy="102.8" rx="3.8" ry="2.4" fill="#FF7AB6"/></g>
  </g>
  <circle class="dtop" cx="80" cy="30" r="6.6" fill="${color}"/>
  <g class="sparkles">${[0, 1, 2, 3].map(() => `<path d="${SPARKLE}" fill="#B4A0FF"/>`).join("")}</g>
</svg>`;
  }

  /** Mounts a Blob in `el`; returns `draw(state)`. */
  function mount(el, color) {
    el.innerHTML = markup(color);
    const q = (s) => el.querySelector(s);
    const qa = (s) => [...el.querySelectorAll(s)];
    const faces = { open: q(".f-open"), dizzy: q(".f-dizzy"), closed: q(".f-closed"), happy: q(".f-happy") };
    const body = q(".body");
    const eyes = q(".eyes");
    const halo = q(".halo");
    const dtop = q(".dtop");
    const d1 = qa(".d1");
    const d2 = qa(".d2");
    const stars = qa(".stars path");
    const sparkles = qa(".sparkles path");
    const spins = qa(".spin");

    return function draw(s) {
      const t = s.t;
      // Idle breathing, as the app's m-squish (3.2 s), plus extra squash.
      const breathe = Math.sin((t / 3.2) * Math.PI * 2) * 0.025;
      const sy = 1 + breathe + (s.squash || 0);
      const sx = 1 - breathe * 0.8 - (s.squash || 0) * 0.6;
      body.setAttribute(
        "transform",
        `rotate(${s.tilt || 0} 80 118) translate(80 118) scale(${sx} ${sy}) translate(-80 -118)`,
      );
      for (const [name, g] of Object.entries(faces)) g.setAttribute("opacity", String(s.face[name] || 0));
      // Blink every 4 s (a quick close, a softer open).
      const ph = t % 4;
      const blink = ph > 3.82 ? 1 - Math.sin(((ph - 3.82) / 0.18) * Math.PI) * 0.92 : 1;
      eyes.setAttribute("transform", `translate(0 88.6) scale(1 ${blink}) translate(0 -88.6)`);
      halo.setAttribute("opacity", String(0.14 + 0.06 * Math.sin((t / 3) * Math.PI * 2)));
      halo.setAttribute("transform", `translate(80 88.6) scale(${(s.halo || 1) * (1 + 0.03 * Math.sin((t / 3) * Math.PI * 2))}) translate(-80 -88.6)`);
      // Droplets bob; the top one floats above the head.
      const bob = (p, a) => Math.sin((t / p) * Math.PI * 2 + a);
      dtop.setAttribute("transform", `translate(0 ${bob(4.5, 0) * 3 - (s.squash || 0) * 30})`);
      dtop.setAttribute("opacity", String(s.dropTop ?? 1));
      for (const d of d1) d.setAttribute("transform", `translate(0 ${bob(5, 1.6) * 2.2})`);
      for (const d of d2) d.setAttribute("transform", `translate(0 ${bob(4, 2.8) * 2.2})`);
      for (const g of spins) {
        const x = g.dataset.x;
        g.setAttribute("transform", `rotate(${t * 420} ${x} 88.6)`);
      }
      // Dizzy stars orbit the head.
      stars.forEach((p, i) => {
        const a = t * 3.2 + (i * Math.PI * 2) / 3;
        const x = 80 + Math.cos(a) * 38;
        const y = 40 + Math.sin(a) * 9;
        const k = (s.stars || 0) * (0.75 + 0.25 * Math.sin(a));
        p.setAttribute("transform", `translate(${x} ${y}) scale(${k})`);
        p.setAttribute("opacity", String(Math.min(1, (s.stars || 0) * 1.4)));
      });
      // Celebration sparkles.
      const spots = [
        [24, 52, 1.1],
        [138, 40, 0.9],
        [144, 104, 0.7],
        [16, 108, 0.8],
      ];
      sparkles.forEach((p, i) => {
        const [x, y, size] = spots[i];
        const k = (s.sparkles || 0) * size * (0.8 + 0.25 * Math.sin(t * 4 + i * 1.7));
        p.setAttribute("transform", `translate(${x} ${y}) rotate(${t * 40 + i * 30}) scale(${k})`);
      });
    };
  }

  window.HedyBlob = { mount };
})();
