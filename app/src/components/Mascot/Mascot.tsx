import { useId, type CSSProperties } from "react";

import "./Mascot.css";

/** `idle` is the resting pose from the visual identity; the other four are the spec's states. */
export type MascotState = "idle" | "sleeping" | "thinking" | "working" | "done";

export const MASCOT_STATE_LABEL: Record<MascotState, string> = {
  idle: "En espera",
  sleeping: "Durmiendo",
  thinking: "Pensando",
  working: "Trabajando",
  done: "¡Listo!",
};

/** Below this size droplets, halo, cheeks and props are dropped for legibility. */
const TINY_SIZE = 32;

const BODY = "M40 92c-4-26 14-48 40-48s46 18 42 44c-3 22-18 36-42 36S43 112 40 92z";

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

/** The lava blob. Animations use only transform, opacity and fill. */
export function Mascot({ color, state, size = 120, label, className }: MascotProps) {
  const gooId = `goo${useId().replace(/[^a-zA-Z0-9]/g, "")}`;
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
      aria-label={label ?? MASCOT_STATE_LABEL[state]}
      data-state={state}
      style={{ "--mascot-color": color } as CSSProperties}
    >
      <defs>
        <filter id={gooId} x="-30%" y="-30%" width="160%" height="160%">
          <feGaussianBlur in="SourceGraphic" stdDeviation="5" result="blur" />
          <feColorMatrix
            in="blur"
            mode="matrix"
            values="1 0 0 0 0  0 1 0 0 0  0 0 1 0 0  0 0 0 22 -10"
          />
        </filter>
      </defs>

      {!tiny && <circle className="m-halo" cx="80" cy="90" r="66" />}

      <g className="m-jump">
        <g className="m-squish">
          {/* Body and droplets share the goo filter so they melt together. */}
          <g className="m-goo" filter={`url(#${gooId})`}>
            <path d={BODY} />
            {!tiny && <Droplets state={state} />}
          </g>
          <ellipse className="m-shine" cx="64" cy="64" rx="12" ry="7" />
          {!tiny && (
            <g className="m-cheeks">
              <circle cx="58" cy="100" r="5" />
              <circle cx="102" cy="100" r="5" />
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
 * Droplets inside the goo filter: they melt into the body when they touch it.
 * The top one floats just above the halo, as in the visual identity.
 */
function Droplets({ state }: { state: MascotState }) {
  switch (state) {
    case "idle":
      return (
        <>
          <circle className="m-drop m-drop-top" cx="80" cy="22" r="8" />
          <circle className="m-drop m-drop-2" cx="118" cy="104" r="8" />
          <circle className="m-drop m-drop-3" cx="44" cy="108" r="7" />
        </>
      );
    case "sleeping":
      // A single slow drip on the side.
      return <circle className="m-drop m-drop-2 m-drop--slow" cx="116" cy="108" r="8" />;
    case "thinking":
      return <circle className="m-drop m-drop-3" cx="44" cy="108" r="7" />;
    case "working":
      return null;
    case "done":
      return <circle className="m-drop m-drop-3" cx="44" cy="108" r="7" />;
  }
}

function OpenEyes({ dx = 0, dy = 0, ry = 5.5 }: { dx?: number; dy?: number; ry?: number }) {
  return (
    <g className="m-eyes">
      <ellipse className="m-eye" cx={68 + dx} cy={90 + dy} rx="5.5" ry={ry} />
      <ellipse className="m-eye" cx={92 + dx} cy={90 + dy} rx="5.5" ry={ry} />
      <circle className="m-glint" cx={70 + dx} cy={88 + dy} r="2" />
      <circle className="m-glint" cx={94 + dx} cy={88 + dy} r="2" />
    </g>
  );
}

function Face({ state }: { state: MascotState }) {
  switch (state) {
    case "idle":
      return (
        <g className="m-face">
          <OpenEyes />
          <path className="m-line" d="M72 102q8 8 16 0" />
        </g>
      );
    case "sleeping":
      return (
        <g className="m-face">
          <path className="m-line" d="M62.5 90q5.5 4 11 0" />
          <path className="m-line" d="M86.5 90q5.5 4 11 0" />
          <ellipse className="m-mouth" cx="80" cy="104" rx="2.6" ry="2.2" />
        </g>
      );
    case "thinking":
      // Looks up and to the right, one eyebrow raised, mouth twisted.
      return (
        <g className="m-face">
          <OpenEyes dx={2} dy={-2} />
          <path className="m-line m-brow" d="M61 80l9-4" />
          <path className="m-line" d="M74 106q6-5 12-1" />
        </g>
      );
    case "working":
      return (
        <g className="m-face">
          <path className="m-line" d="M61 81l11 4" />
          <path className="m-line" d="M99 81l-11 4" />
          <OpenEyes dy={1} ry={4.5} />
          <path className="m-line" d="M72 101q8 6 16 0" />
          <ellipse className="m-tongue" cx="85" cy="106" rx="3.6" ry="4.2" />
        </g>
      );
    case "done":
      return (
        <g className="m-face">
          <path className="m-line" d="M62.5 92q5.5-7 11 0" />
          <path className="m-line" d="M86.5 92q5.5-7 11 0" />
          <path className="m-mouth" d="M71 100q9 14 18 0z" />
          <ellipse className="m-tongue" cx="80" cy="107" rx="4.5" ry="3" />
        </g>
      );
  }
}

const STAR = "M0-6l1.8 3.9 4.2.5-3.1 2.9.8 4.2L0 3.4l-3.7 2.1.8-4.2-3.1-2.9 4.2-.5z";

function Props({ state }: { state: MascotState }) {
  switch (state) {
    case "idle":
      return null;
    case "sleeping":
      return (
        <g className="m-zzz">
          <text className="m-z m-z1" x="110" y="58">z</text>
          <text className="m-z m-z2" x="119" y="46">z</text>
          <text className="m-z m-z3" x="128" y="33">z</text>
        </g>
      );
    case "thinking":
      return (
        <g className="m-cloud">
          <circle className="m-trail" cx="117" cy="62" r="4.5" />
          <circle className="m-trail" cx="124" cy="52" r="2.5" />
          <rect className="m-bubble" x="112" y="22" width="44" height="26" rx="13" />
          <circle className="m-dot m-dot1" cx="124" cy="35" r="2.8" />
          <circle className="m-dot m-dot2" cx="134" cy="35" r="2.8" />
          <circle className="m-dot m-dot3" cx="144" cy="35" r="2.8" />
        </g>
      );
    case "working":
      return (
        <g className="m-laptop">
          <rect className="m-screen" x="50" y="104" width="60" height="36" rx="5" />
          <rect className="m-code m-code1" x="57" y="112" width="26" height="3.5" rx="1.75" />
          <rect className="m-code m-code2" x="57" y="120" width="38" height="3.5" rx="1.75" />
          <rect className="m-code m-code3" x="57" y="128" width="20" height="3.5" rx="1.75" />
          <path className="m-base" d="M44 140h72l6 6H38z" />
          <circle className="m-hand m-hand-left" cx="60" cy="141" r="6.5" />
          <circle className="m-hand m-hand-right" cx="100" cy="141" r="6.5" />
        </g>
      );
    case "done":
      return (
        <g className="m-party">
          <path className="m-star m-star1" d={STAR} transform="translate(26 40)" />
          <path className="m-star m-star2" d={STAR} transform="translate(136 28)" />
          <path className="m-star m-star3" d={STAR} transform="translate(146 76)" />
          <rect className="m-confetti m-c1" x="20" y="70" width="5" height="3" rx="1" />
          <rect className="m-confetti m-c2" x="132" y="50" width="5" height="3" rx="1" />
          <rect className="m-confetti m-c3" x="52" y="22" width="5" height="3" rx="1" />
          <rect className="m-confetti m-c4" x="108" y="16" width="5" height="3" rx="1" />
        </g>
      );
  }
}
