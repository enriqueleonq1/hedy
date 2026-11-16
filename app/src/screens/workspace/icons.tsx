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

/** A paperclip: attach images. */
export function PaperclipIcon() {
  return (
    <svg {...common} className="icon">
      <path d="m20.5 11.5-8.3 8.3a5 5 0 0 1-7.1-7.1l8.6-8.6a3.3 3.3 0 0 1 4.7 4.7l-8.6 8.6a1.7 1.7 0 0 1-2.4-2.4l7.9-7.9" />
    </svg>
  );
}

/** A picture: an image that is gone. */
export function ImageIcon() {
  return (
    <svg {...common} className="icon">
      <rect x="3.5" y="4.5" width="17" height="15" rx="2.5" />
      <circle cx="9" cy="10" r="1.6" />
      <path d="m20.5 16-5-5-8.5 8.5" />
    </svg>
  );
}

/** ↗ into a folder: carry a conversation into a project. */
export function CarryIcon() {
  return (
    <svg {...common} className="icon">
      <path d="M4 8V6a2 2 0 0 1 2-2h3l2 2h7a2 2 0 0 1 2 2v1" />
      <path d="M4 12v6a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-4" />
      <path d="M10 15l5-5M11 10h4v4" />
    </svg>
  );
}

/** A page with a fold: a memory note. */
export function NoteIcon() {
  return (
    <svg {...common} className="icon">
      <path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z" />
      <path d="M14 3v5h5M9 13h6M9 17h4" />
    </svg>
  );
}
