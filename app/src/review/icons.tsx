// Line icons of the review (toolbar, file rows, comment actions). Decorative:
// the control carries the name.

const common = {
  width: 15,
  height: 15,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 2,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  "aria-hidden": true,
  focusable: false,
};

/** One column of lines: the unified diff. */
export function UnifiedIcon() {
  return (
    <svg {...common} className="icon">
      <rect x="4" y="4" width="16" height="16" rx="2.5" />
      <path d="M8 9h8M8 12h8M8 15h5" />
    </svg>
  );
}

/** Two columns: the side-by-side diff. */
export function SplitIcon() {
  return (
    <svg {...common} className="icon">
      <rect x="3.5" y="4" width="17" height="16" rx="2.5" />
      <path d="M12 4v16M6.5 9h3M14.5 9h3M6.5 13h3M14.5 13h3" />
    </svg>
  );
}

/** A box with an arrow out: open outside Hedy (VS Code). */
export function ExternalIcon() {
  return (
    <svg {...common} className="icon">
      <path d="M14 4h6v6M20 4l-8.5 8.5" />
      <path d="M18 14v4.5A1.5 1.5 0 0 1 16.5 20h-11A1.5 1.5 0 0 1 4 18.5v-11A1.5 1.5 0 0 1 5.5 6H10" />
    </svg>
  );
}

/** A page: open the file in Hedy's viewer. */
export function FileIcon() {
  return (
    <svg {...common} className="icon">
      <path d="M14 3.5H7A1.5 1.5 0 0 0 5.5 5v14A1.5 1.5 0 0 0 7 20.5h10a1.5 1.5 0 0 0 1.5-1.5V8z" />
      <path d="M14 3.5V8h4.5" />
    </svg>
  );
}

/** A speech bubble: comments. */
export function CommentIcon() {
  return (
    <svg {...common} width={13} height={13} className="icon">
      <path d="M5 5.5h14a1.5 1.5 0 0 1 1.5 1.5v8a1.5 1.5 0 0 1-1.5 1.5h-7L7.5 20v-3.5H5A1.5 1.5 0 0 1 3.5 15V7A1.5 1.5 0 0 1 5 5.5z" />
    </svg>
  );
}

export function PencilIcon() {
  return (
    <svg {...common} width={14} height={14} className="icon">
      <path d="M4 20h4L19 9a2.8 2.8 0 0 0-4-4L4 16z" />
    </svg>
  );
}

export function TrashIcon() {
  return (
    <svg {...common} width={14} height={14} className="icon">
      <path d="M4.5 7h15M9.5 7V4.5h5V7M6.5 7l1 12.5h9l1-12.5" />
    </svg>
  );
}

/** A paper plane: send to the AI. */
export function SendIcon() {
  return (
    <svg {...common} width={14} height={14} className="icon">
      <path d="M21 3 10.5 13.5M21 3l-6.5 18-4-7.5L3 9.5z" />
    </svg>
  );
}

/** ✓: done. */
export function CheckIcon() {
  return (
    <svg {...common} width={14} height={14} strokeWidth={2.4} className="icon">
      <path d="m5 12.5 4.5 4.5L19 7.5" />
    </svg>
  );
}

/** ⌄ / ⌃: a disclosure. */
export function DisclosureIcon({ open }: { open: boolean }) {
  return (
    <svg {...common} width={12} height={12} strokeWidth={2.4} className={`icon rv-disclosure${open ? " rv-disclosure--open" : ""}`}>
      <path d="m6 9 6 6 6-6" />
    </svg>
  );
}
