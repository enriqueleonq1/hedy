// Small line icons of the workspace's toolbar, composer and queue. Decorative:
// the control carries the name.

const common = {
  width: 16,
  height: 16,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 2,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  "aria-hidden": true,
  focusable: false,
};

/** +: something new. */
export function PlusIcon() {
  return (
    <svg {...common} width={14} height={14} strokeWidth={2.4} className="icon">
      <path d="M12 5v14M5 12h14" />
    </svg>
  );
}

/** ■: stop the agent. */
export function StopIcon() {
  return (
    <svg {...common} className="icon">
      <rect x="6.5" y="6.5" width="11" height="11" rx="2" fill="currentColor" stroke="none" />
    </svg>
  );
}

/** A window with its right column: the side panel. */
export function PanelIcon() {
  return (
    <svg {...common} width={18} height={18} className="icon">
      <rect x="3" y="4.5" width="18" height="15" rx="2.5" />
      <path d="M14.5 4.5v15" />
    </svg>
  );
}

/** A pencil: edit. */
export function PencilIcon() {
  return (
    <svg {...common} width={14} height={14} className="icon">
      <path d="M4 20h4L19 9a2.8 2.8 0 0 0-4-4L4 16z" />
    </svg>
  );
}

/** ×: remove. */
export function CloseIcon() {
  return (
    <svg {...common} width={14} height={14} strokeWidth={2.4} className="icon">
      <path d="M6 6l12 12M18 6 6 18" />
    </svg>
  );
}
