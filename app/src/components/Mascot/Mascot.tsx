import { useEffect, useId, useState, type CSSProperties, type ReactNode } from "react";

import { useT, type MessageKey } from "../../i18n";
import { ACTIVITY_LABEL, ACTIVITY_TIMING, DEFAULT_ACTIVITY, type WorkingActivity } from "./activity";
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
  /** What it does while `working`; the laptop (`edit`) when left out. */
  activity?: WorkingActivity;
  /** Rendered width and height in px. */
  size?: number;
  /** Accessible name; defaults to the activity, or else the state. */
  label?: string;
  className?: string;
}

/**
 * The value that is fading out after `value` changed, for `ms`; `null` once
 * the change settles. Two layers overlap meanwhile, so props cross-fade.
 */
function useLeaving<T>(value: T, ms: number): T | null {
  const [current, setCurrent] = useState(value);
  const [leaving, setLeaving] = useState<T | null>(null);
  if (value !== current) {
    setLeaving(current);
    setCurrent(value);
  }
  useEffect(() => {
    if (leaving === null) return;
    const timer = setTimeout(() => setLeaving(null), ms);
    return () => clearTimeout(timer);
  }, [leaving, ms]);
  return leaving;
}

/**
 * The lava blob. Drawn in two passes: the goo filter only melts droplets into
 * the body (its alpha threshold leaves jagged edges), and crisp copies of the
 * body and droplets go on top, so every visible edge stays anti-aliased.
 * Animations use only transform, opacity and fill.
 */
export function Mascot({ color, state, activity, size = 120, label, className }: MascotProps) {
  const t = useT();
  const uid = useId().replace(/[^a-zA-Z0-9]/g, "");
  const tiny = size < TINY_SIZE;
  const act = state === "working" ? (activity ?? DEFAULT_ACTIVITY) : null;
  const leaving = useLeaving(act, ACTIVITY_TIMING.fadeMs);
  const fading = !tiny && act !== null && leaving !== null;
  const classes = ["mascot", `mascot--${state}`, act && `mascot--act-${act}`, tiny && "mascot--tiny", className]
    .filter(Boolean)
    .join(" ");
  const name = label ?? (activity && act ? t(ACTIVITY_LABEL[act]) : t(MASCOT_STATE_LABEL[state]));

  /** One part of the pose, cross-faded between the old activity and the new. */
  const layers = (part: (a: WorkingActivity) => ReactNode, base: string) => {
    if (!act) return null;
    return (
      <>
        {fading && leaving && (
          <g key={leaving} className={`${base} m-leave`} aria-hidden>
            {part(leaving)}
          </g>
        )}
        <g key={act} className={fading ? `${base} m-enter` : base}>
          {part(act)}
        </g>
      </>
    );
  };

  return (
    <svg
      className={classes}
      viewBox="0 0 160 160"
      width={size}
      height={size}
      role="img"
      aria-label={name}
      data-state={state}
      data-activity={act ?? undefined}
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
          {!tiny && layers((a) => <Behind activity={a} />, "m-behind")}
          {!tiny && (
            <g className="m-goo" filter={`url(#goo${uid})`} aria-hidden>
              <path d={BODY} />
              <Droplets state={state} activity={act} />
            </g>
          )}
          <path className="m-body" d={BODY} />
          {!tiny && <Droplets state={state} activity={act} />}
          <path d={BODY} fill={`url(#shade${uid})`} />
          <ellipse className="m-shine" cx="66.3" cy="66.3" rx="10.3" ry="5.7" />
          {!tiny && (
            <g className="m-cheeks">
              <circle cx="60.9" cy="96.6" r="4.3" />
              <circle cx="98.6" cy="96.6" r="4.3" />
            </g>
          )}
          {act && !tiny ? (
            layers((a) => <WorkingFace activity={a} />, "m-face")
          ) : (
            <Face state={state} />
          )}
        </g>
      </g>

      {!tiny && (act ? layers((a) => <WorkingProps activity={a} />, "m-props") : <Props state={state} />)}
    </svg>
  );
}

/**
 * The droplets of each state. The top one floats above the head, apart from
 * the body, as in the visual identity; the side ones melt into it.
 */
function Droplets({ state, activity }: { state: MascotState; activity: WorkingActivity | null }) {
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
      switch (activity) {
        case "search":
          // Both side droplets hop, as if sniffing around.
          return (
            <>
              <circle className="m-drop m-drop--hop" cx="119" cy="103" r="6" />
              <circle className="m-drop m-drop--hop m-drop--late" cx="43.5" cy="106" r="5.5" />
            </>
          );
        case "think":
          return <circle className="m-drop m-drop-3" cx="43.5" cy="106" r="5.5" />;
        case "waiting":
          // Slow lava: blobs swell out of the head, stretch and drift up.
          return (
            <>
              <circle className="m-drop m-lava m-lava1" cx="70" cy="50" r="5" />
              <circle className="m-drop m-lava m-lava2" cx="89" cy="51" r="4.2" />
              <circle className="m-drop m-lava m-lava3" cx="80" cy="49" r="6" />
            </>
          );
        default:
          return null;
      }
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

/** Brows drawn in: concentrating. */
const FOCUSED_BROWS = (
  <>
    <path className="m-line" d="M61 80.6l10 3" />
    <path className="m-line" d="M97 80.6l-10 3" />
  </>
);

function Face({ state }: { state: MascotState }) {
  switch (state) {
    case "idle":
      return (
        <g className="m-face">
          <OpenEyes />
          {/* Closed, smiling eyes, shown on hover. */}
          <g className="m-happy" aria-hidden>
            <path className="m-line" d="M62.6 90q4.8-5.8 9.6 0" />
            <path className="m-line" d="M85.6 90q4.8-5.8 9.6 0" />
          </g>
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
      return (
        <g className="m-face">
          <ThinkingFace />
        </g>
      );
    case "working":
      // Tiny mascots keep the plain working face, whatever the activity.
      return (
        <g className="m-face">
          <WorkingFace activity={DEFAULT_ACTIVITY} />
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

/** Looks up and to the right, one eyebrow raised, a small "hmm" mouth. */
function ThinkingFace() {
  return (
    <>
      <OpenEyes dx={1.6} dy={-1.6} />
      <path className="m-line m-brow" d="M61.5 79.5l8.5-3.2" />
      <path className="m-line" d="M75 100.6q4.5 1.6 9.5-.8" />
    </>
  );
}

/** The face of each activity; eyes always look at what it holds. */
function WorkingFace({ activity }: { activity: WorkingActivity }) {
  switch (activity) {
    case "reading":
      // Round glasses; the eyes run along the lines of the book.
      return (
        <>
          <g className="m-look">
            <OpenEyes dy={2} squint={0.9} />
          </g>
          <g className="m-glasses">
            <circle cx="67.4" cy="90" r="7.8" />
            <circle cx="90.4" cy="90" r="7.8" />
            <path d="M75.2 89q3.7-2.4 7.4 0M59.6 88.6l-5-2M98.2 88.6l5-2" />
          </g>
          <path className="m-line" d="M75 100.8q4.4 3.2 8.8 0" />
        </>
      );
    case "search":
      return (
        <>
          <OpenEyes dx={2.2} dy={-1.6} />
          <path className="m-line" d="M62.5 79.4q4.4-2.6 9 -.6" />
          <ellipse className="m-mouth" cx="80.5" cy="101" rx="2.2" ry="2.4" />
        </>
      );
    case "edit":
      // Focused: brows drawn in, eyes on the screen, tongue out.
      return (
        <>
          {FOCUSED_BROWS}
          <OpenEyes dy={1} squint={0.85} />
          <path className="m-line" d="M72.6 98.4q6.4 5 12.8 0" />
          <ellipse className="m-tongue" cx="83" cy="102" rx="2.8" ry="3.2" />
        </>
      );
    case "command":
      return (
        <>
          {FOCUSED_BROWS}
          <OpenEyes dx={2.4} dy={1.6} squint={0.8} />
          <path className="m-line" d="M75.5 100.8h8.5" />
        </>
      );
    case "test":
      // Wide eyes and an "oh": will they pass?
      return (
        <>
          <path className="m-line" d="M62.4 79.6q5-3.2 10 0" />
          <path className="m-line" d="M85.4 79.6q5-3.2 10 0" />
          <OpenEyes dx={1.4} squint={1.12} />
          <ellipse className="m-line" cx="80" cy="101" rx="2.6" ry="2.4" />
        </>
      );
    case "web":
      return (
        <>
          <OpenEyes dx={2} dy={1.4} />
          <path className="m-line" d="M73 98.6q6.6 6 13.2 0" />
        </>
      );
    case "think":
      return <ThinkingFace />;
    case "subagents":
      return (
        <>
          <OpenEyes />
          <path className="m-line" d="M72.4 98.6q6.6 6.4 13.4 0" />
        </>
      );
    case "waiting":
      // Heavy lids and a slow, wavy mouth: melting while it waits.
      return (
        <>
          <OpenEyes dy={1.6} squint={0.5} />
          <path className="m-line" d="M62.4 87.6h10M85.4 87.6h10" />
          <path className="m-line" d="M75 101q2-1.6 4 0t4 0" />
        </>
      );
    case "long":
      return (
        <>
          {FOCUSED_BROWS}
          <OpenEyes dy={1} squint={0.9} />
          <path className="m-line" d="M73 101q3.25-2.4 6.5 0t6.5 0" />
        </>
      );
  }
}

/** Drawn behind the body: only the antenna, which grows out of the head. */
function Behind({ activity }: { activity: WorkingActivity }) {
  if (activity !== "web") return null;
  return (
    <g className="m-antenna">
      <path className="m-antenna-stalk" d="M80 54V36" />
      <circle className="m-antenna-ball" cx="80" cy="32.5" r="4.5" />
      <path className="m-signal m-signal1" d="M73.6 25.4a9 9 0 0 1 12.8 0" />
      <path className="m-signal m-signal2" d="M69.6 21a15 15 0 0 1 20.8 0" />
    </g>
  );
}

/** A four-point sparkle centered on 0,0. */
const SPARKLE = "M0-7C.6-2.2 2.2-.6 7 0 2.2.6.6 2.2 0 7-.6 2.2-2.2.6-7 0-2.2-.6-.6-2.2 0-7z";

/** An eight-tooth gear centered on 0,0, with a hole. */
const GEAR =
  "M5.33-1.73 7.42-1.63 7.42 1.63 5.33 1.73 4.99 2.54 6.4 4.1 4.1 6.4 2.54 4.99 1.73 5.33 1.63 7.42-1.63 7.42" +
  "-1.73 5.33-2.54 4.99-4.1 6.4-6.4 4.1-4.99 2.54-5.33 1.73-7.42 1.63-7.42-1.63-5.33-1.73-4.99-2.54-6.4-4.1" +
  "-4.1-6.4-2.54-4.99-1.73-5.33-1.63-7.42 1.63-7.42 1.73-5.33 2.54-4.99 4.1-6.4 6.4-4.1 4.99-2.54Z" +
  "M2.4 0A2.4 2.4 0 1 0-2.4 0 2.4 2.4 0 1 0 2.4 0Z";

/** A page of the open book, from the spine (x 80) outwards; `side` is -1 or 1. */
const page = (side: number) =>
  `M80 116.5C${80 + side * 8} 113.5 ${80 + side * 18} 113.5 ${80 + side * 28} 115.5V139.5` +
  `C${80 + side * 18} 137.5 ${80 + side * 8} 137.5 80 140.5Z`;

/** Three lines of text on a page. */
const textLines = (x: number) => `M${x} 122q9-1.4 18 0M${x} 127.5q9-1.4 18 0M${x} 133q6-1 12 0`;

/** A mini blob of the subagents pose, standing on the floor at `cx`. */
function MiniBlob({ cx, side }: { cx: number; side: "left" | "right" }) {
  const scale = 0.34;
  return (
    <g className={`m-mini m-mini--${side}`}>
      <g transform={`translate(${cx - 80 * scale} ${118 - 117.5 * scale}) scale(${scale})`}>
        <path className="m-mini-body" d={BODY} />
        <ellipse className="m-shine" cx="66.3" cy="66.3" rx="10.3" ry="5.7" />
        <circle className="m-eye" cx="67.4" cy="88.6" r="6" />
        <circle className="m-eye" cx="90.4" cy="88.6" r="6" />
        <path className="m-line m-line--mini" d="M72.4 99q6.6 6.4 13.4 0" />
      </g>
    </g>
  );
}

function WorkingProps({ activity }: { activity: WorkingActivity }) {
  switch (activity) {
    case "reading":
      return (
        <g className="m-book">
          <path className="m-paper" d={page(-1)} />
          <path className="m-paper" d={page(1)} />
          <path className="m-print" d={`${textLines(57)}${textLines(85)}`} />
          {/* A page that turns from right to left over the spine. */}
          <path className="m-paper m-page-turn" d={page(1)} />
          <path className="m-spine" d="M80 116.5v24" />
          <circle className="m-hand" cx="52.5" cy="134" r="5.5" />
          <circle className="m-hand" cx="107.5" cy="134" r="5.5" />
        </g>
      );
    case "search":
      return (
        <g className="m-lens">
          <path className="m-lens-handle" d="M127 71l9.5 9.5" />
          <circle className="m-hand" cx="138" cy="82.5" r="5.5" />
          <circle className="m-lens-glass" cx="118" cy="62" r="12" />
          <path className="m-lens-glint" d="M110.6 59.4a8 8 0 0 1 4.6-5" />
        </g>
      );
    case "edit":
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
    case "command":
      return (
        <g className="m-term">
          <rect className="m-term-box" x="96" y="100" width="52" height="41" rx="6" />
          <circle className="m-term-dot m-term-dot1" cx="102.5" cy="106" r="1.6" />
          <circle className="m-term-dot m-term-dot2" cx="107.5" cy="106" r="1.6" />
          <circle className="m-term-dot m-term-dot3" cx="112.5" cy="106" r="1.6" />
          <path className="m-term-prompt" d="M102 113.5l4 3.2-4 3.2" />
          <rect className="m-cursor" x="109" y="114" width="5.5" height="6.5" rx="1" />
          <rect className="m-out m-out1" x="102" y="125" width="30" height="2.6" rx="1.3" />
          <rect className="m-out m-out2" x="102" y="130" width="22" height="2.6" rx="1.3" />
          <rect className="m-out m-out3" x="102" y="135" width="38" height="2.6" rx="1.3" />
          <circle className="m-hand" cx="96.5" cy="130" r="5.5" />
        </g>
      );
    case "test":
      return (
        <g className="m-flask">
          <path className="m-flask-glass" d="M115 92.5v12.5l-11.5 22.5a5 5 0 0 0 4.5 7.2h27a5 5 0 0 0 4.5-7.2L128 105V92.5z" />
          <path className="m-liquid" d="M108.6 121h26.8l4 8a3 3 0 0 1-2.7 4.3h-29.4a3 3 0 0 1-2.7-4.3z" />
          <circle className="m-bubble-up m-bub1" cx="117" cy="119" r="2.3" />
          <circle className="m-bubble-up m-bub2" cx="122" cy="121" r="1.8" />
          <circle className="m-bubble-up m-bub3" cx="126" cy="118.5" r="2.1" />
          <rect className="m-cork" x="112.5" y="88" width="18" height="5.5" rx="2" />
          <circle className="m-hand" cx="107" cy="114" r="5.5" />
        </g>
      );
    case "web":
      return (
        <g className="m-globe">
          <circle className="m-globe-ball" cx="124" cy="118" r="16" />
          <path className="m-globe-line" d="M108 118h32M110.5 109.5h27M110.5 126.5h27M124 102v32" />
          <ellipse className="m-meridian m-meridian1" cx="124" cy="118" rx="8" ry="16" />
          <ellipse className="m-meridian m-meridian2" cx="124" cy="118" rx="8" ry="16" />
          <circle className="m-hand" cx="107.5" cy="124" r="5.5" />
        </g>
      );
    case "think":
      return (
        <g className="m-cloud">
          <circle className="m-trail" cx="116" cy="62" r="3.6" />
          <circle className="m-trail" cx="123" cy="53" r="2.2" />
          <rect className="m-bubble" x="110" y="16" width="44" height="30" rx="15" />
          <g transform="translate(125 31)">
            <path className="m-gear" d={GEAR} fillRule="evenodd" />
          </g>
          <circle className="m-dot m-dot1" cx="138.5" cy="31" r="2.3" />
          <circle className="m-dot m-dot2" cx="145.5" cy="31" r="2.3" />
        </g>
      );
    case "subagents":
      return (
        <g className="m-team">
          <MiniBlob cx={27} side="left" />
          <MiniBlob cx={133} side="right" />
        </g>
      );
    case "waiting":
      return null;
    case "long":
      return (
        <g className="m-effort">
          <path className="m-sweat m-sweat1" d="M108 60c3 4.5 4.6 7.4 4.6 9.6a4.6 4.6 0 0 1-9.2 0c0-2.2 1.6-5.1 4.6-9.6z" />
          <path className="m-sweat m-sweat2" d="M53.5 70c2 3 3 5 3 6.4a3 3 0 0 1-6 0c0-1.4 1-3.4 3-6.4z" />
          {/* A quick wipe of the forehead. */}
          <circle className="m-hand m-hand--over m-wipe" cx="100" cy="70" r="5.5" />
        </g>
      );
  }
}

function Props({ state }: { state: MascotState }) {
  switch (state) {
    case "idle":
    case "working":
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
