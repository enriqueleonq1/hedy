import { useId, type CSSProperties } from "react";

import { useT, type MessageKey } from "../../i18n";
import "./Mascot.css";

/** `idle` is the resting pose from the visual identity; the other four are the spec's states. */
export type MascotState = "idle" | "sleeping" | "thinking" | "working" | "done";

export const MASCOT_STATE_LABEL: Record<MascotState, MessageKey> = {
  idle: "mascot.idle",
  sleeping: "mascot.sleeping",
  thinking: "mascot.thinking",
  working: "mascot.working",
  done: "mascot.done",
};

/** Below this size droplets, halo, cheeks and props are dropped for legibility. */
const TINY_SIZE = 32;

/**
 * The body, traced from the visual identity (0-identidad-03.png): a broad dome,
 * nearly straight sides and a wavy lava hem with a lobe on each side.
 */
const BODY =
  "M80 48.5C97.5 48.5 111 58.5 114.8 71C116.8 77.5 116.4 83 116.5 88.5C116.6 93.5 115.6 96.8 117.5 100.5" +
  "C119.5 105 118 109 112.5 110C106.5 111 102 112 97 114.5C91 117.5 84 118.5 77 117.5C70 116.5 65 113 59 111.5" +
  "C53 110 46.5 110.5 44.5 106C43 102.5 46.4 98 46 93C45.6 89 44.6 83.5 45.4 77C47 60.5 61.5 48.5 80 48.5Z";

interface MascotProps {
  /** Account color. */
  color: string;
  state: MascotState;
  /** Rendered width and height in px. */
  size?: number;
  /** Accessible name; defaults to the state name. */
  label?: string;
  className?: string;
}

/**
 * The lava blob. Drawn in two passes: the goo filter only melts droplets into
 * the body (its alpha threshold leaves jagged edges), and crisp copies of the
 * body and droplets go on top, so every visible edge stays anti-aliased.
 * Animations use only transform, opacity and fill.
 */
export function Mascot({ color, state, size = 120, label, className }: MascotProps) {
  const t = useT();
  const uid = useId().replace(/[^a-zA-Z0-9]/g, "");
  const tiny = size < TINY_SIZE;
  const classes = ["mascot", `mascot--${state}`, tiny && "mascot--tiny", className]
    .filter(Boolean)
    .join(" ");

  return (
    <svg
      className={classes}
      viewBox="0 0 160 160"
      width={size}
      height={size}
      role="img"
      aria-label={label ?? t(MASCOT_STATE_LABEL[state])}
      data-state={state}
      style={{ "--mascot-color": color } as CSSProperties}
    >
      <defs>
        <filter id={`goo${uid}`} x="-30%" y="-30%" width="160%" height="160%" colorInterpolationFilters="sRGB">
          <feGaussianBlur in="SourceGraphic" stdDeviation="4" result="blur" />
          {/* Cut above half alpha, so the melted outline stays inside the crisp
              body and never shows a soft fringe; only the necks peek out. */}
          <feColorMatrix in="blur" mode="matrix" values="1 0 0 0 0  0 1 0 0 0  0 0 1 0 0  0 0 0 20 -11.5" />
        </filter>
        {/* Soft light from above: a hint of volume on the flat identity color.
            It fades out before the hem, where droplets melt in, so the melted
            necks match the body exactly. */}
        <linearGradient id={`shade${uid}`} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#fff" stopOpacity="0.1" />
          <stop offset="0.55" stopColor="#fff" stopOpacity="0" />
        </linearGradient>
      </defs>

      {!tiny && <circle className="m-halo" cx="80" cy="88.6" r="56.6" />}

      <g className="m-jump">
        <g className="m-squish">
          {!tiny && (
            <g className="m-goo" filter={`url(#goo${uid})`} aria-hidden>
              <path d={BODY} />
              <Droplets state={state} />
            </g>
          )}
          <path className="m-body" d={BODY} />
          {!tiny && <Droplets state={state} />}
          <path d={BODY} fill={`url(#shade${uid})`} />
          <ellipse className="m-shine" cx="66.3" cy="66.3" rx="10.3" ry="5.7" />
          {!tiny && (
            <g className="m-cheeks">
              <circle cx="60.9" cy="96.6" r="4.3" />
              <circle cx="98.6" cy="96.6" r="4.3" />
            </g>
          )}
          <Face state={state} />
        </g>
      </g>

      {!tiny && <Props state={state} />}
    </svg>
  );
}

/**
 * The droplets of each state. The top one floats above the head, apart from
 * the body, as in the visual identity; the side ones melt into it.
 */
function Droplets({ state }: { state: MascotState }) {
  switch (state) {
    case "idle":
      return (
        <>
          <circle className="m-drop m-drop-top" cx="80" cy="30" r="6.6" />
          <circle className="m-drop m-drop-2" cx="119" cy="103" r="6" />
          <circle className="m-drop m-drop-3" cx="43.5" cy="106" r="5.5" />
        </>
      );
    case "sleeping":
      // A single slow drip on the side.
      return <circle className="m-drop m-drop-2 m-drop--slow" cx="118" cy="106" r="6" />;
    case "thinking":
    case "done":
      return <circle className="m-drop m-drop-3" cx="43.5" cy="106" r="5.5" />;
    case "working":
      return null;
  }
}

/** Eye centers and size, measured on the visual identity. */
const EYES = { left: 67.4, right: 90.4, y: 88.6, r: 4.8 };

function OpenEyes({ dx = 0, dy = 0, squint = 1 }: { dx?: number; dy?: number; squint?: number }) {
  const y = EYES.y + dy;
  return (
    <g className="m-eyes">
      {[EYES.left, EYES.right].map((x) => (
        <g key={x}>
          <ellipse className="m-eye" cx={x + dx} cy={y} rx={EYES.r} ry={EYES.r * squint} />
          <circle className="m-glint" cx={x + dx + 1.4} cy={y - 1.4 * squint} r="1.65" />
        </g>
      ))}
    </g>
  );
}

function Face({ state }: { state: MascotState }) {
  switch (state) {
    case "idle":
      return (
        <g className="m-face">
          <OpenEyes />
          <path className="m-line" d="M72.4 98.6q6.6 6.4 13.4 0" />
        </g>
      );
    case "sleeping":
      return (
        <g className="m-face">
          <path className="m-line" d="M62.6 89.4q4.8 3.4 9.6 0" />
          <path className="m-line" d="M85.6 89.4q4.8 3.4 9.6 0" />
          <ellipse className="m-mouth" cx="79" cy="101" rx="2.4" ry="2" />
        </g>
      );
    case "thinking":
      // Looks up and to the right, one eyebrow raised, a small "hmm" mouth.
      return (
        <g className="m-face">
          <OpenEyes dx={1.6} dy={-1.6} />
          <path className="m-line m-brow" d="M61.5 79.5l8.5-3.2" />
          <path className="m-line" d="M75 100.6q4.5 1.6 9.5-.8" />
        </g>
      );
    case "working":
      // Focused: brows drawn in, eyes on the screen, tongue out.
      return (
        <g className="m-face">
          <path className="m-line" d="M61 80.6l10 3" />
          <path className="m-line" d="M97 80.6l-10 3" />
          <OpenEyes dy={1} squint={0.85} />
          <path className="m-line" d="M72.6 98.4q6.4 5 12.8 0" />
          <ellipse className="m-tongue" cx="83" cy="102" rx="2.8" ry="3.2" />
        </g>
      );
    case "done":
      return (
        <g className="m-face">
          <path className="m-line" d="M62.6 90q4.8-5.8 9.6 0" />
          <path className="m-line" d="M85.6 90q4.8-5.8 9.6 0" />
          <path className="m-mouth" d="M71.2 97q7.8 12 15.6 0z" />
          <ellipse className="m-tongue" cx="79" cy="102.8" rx="3.8" ry="2.4" />
        </g>
      );
  }
}

/** A four-point sparkle centered on 0,0. */
const SPARKLE = "M0-7C.6-2.2 2.2-.6 7 0 2.2.6.6 2.2 0 7-.6 2.2-2.2.6-7 0-2.2-.6-.6-2.2 0-7z";

function Props({ state }: { state: MascotState }) {
  switch (state) {
    case "idle":
      return null;
    case "sleeping":
      return (
        <g className="m-zzz">
          <text className="m-z m-z1" x="112" y="58">z</text>
          <text className="m-z m-z2" x="121" y="45">z</text>
          <text className="m-z m-z3" x="130" y="31">z</text>
        </g>
      );
    case "thinking":
      return (
        <g className="m-cloud">
          <circle className="m-trail" cx="116" cy="62" r="3.6" />
          <circle className="m-trail" cx="123" cy="53" r="2.2" />
          <rect className="m-bubble" x="112" y="22" width="42" height="24" rx="12" />
          <circle className="m-dot m-dot1" cx="123" cy="34" r="2.6" />
          <circle className="m-dot m-dot2" cx="133" cy="34" r="2.6" />
          <circle className="m-dot m-dot3" cx="143" cy="34" r="2.6" />
        </g>
      );
    case "working":
      return (
        <g className="m-laptop">
          <rect className="m-screen" x="54" y="111" width="52" height="29" rx="4" />
          <rect className="m-code m-code1" x="60" y="117.5" width="22" height="2.8" rx="1.4" />
          <rect className="m-code m-code2" x="60" y="124" width="33" height="2.8" rx="1.4" />
          <rect className="m-code m-code3" x="60" y="130.5" width="16" height="2.8" rx="1.4" />
          <path className="m-base" d="M44 139h72a2 2 0 0 1 2 2v.5a3.5 3.5 0 0 1-3.5 3.5h-69A3.5 3.5 0 0 1 42 141.5V141a2 2 0 0 1 2-2z" />
          <circle className="m-hand m-hand-left" cx="62" cy="139.5" r="5.5" />
          <circle className="m-hand m-hand-right" cx="98" cy="139.5" r="5.5" />
        </g>
      );
    case "done":
      return (
        <g className="m-party">
          {/* Positioned by a group: the CSS animation owns the path's transform. */}
          <g transform="translate(28 44)">
            <path className="m-sparkle m-sparkle1" d={SPARKLE} />
          </g>
          <g transform="translate(134 30) scale(.8)">
            <path className="m-sparkle m-sparkle2" d={SPARKLE} />
          </g>
          <g transform="translate(140 78) scale(.6)">
            <path className="m-sparkle m-sparkle3" d={SPARKLE} />
          </g>
          <rect className="m-confetti m-c1" x="22" y="72" width="4.5" height="2.6" rx="1.3" />
          <rect className="m-confetti m-c2" x="124" y="54" width="4.5" height="2.6" rx="1.3" />
          <rect className="m-confetti m-c3" x="54" y="26" width="4.5" height="2.6" rx="1.3" />
        </g>
      );
  }
}
