// Small line icons for the sidebar rail. Decorative: the control carries the name.

const common = {
  width: 18,
  height: 18,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 2,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  "aria-hidden": true,
  focusable: false,
};

export function ActivityIcon() {
  return (
    <svg {...common} className="icon">
      <path d="M3 12h4l3-8 4 16 3-8h4" />
    </svg>
  );
}

export function AccountsIcon() {
  return (
    <svg {...common} className="icon">
      <circle cx="9" cy="8" r="3.5" />
      <path d="M2.5 20c.8-3.5 3.4-5.5 6.5-5.5s5.7 2 6.5 5.5" />
      <path d="M16 4.5a3.5 3.5 0 0 1 0 7M18 14.8c1.8.8 3 2.5 3.5 5.2" />
    </svg>
  );
}

export function SettingsIcon() {
  return (
    <svg {...common} className="icon">
      <circle cx="12" cy="12" r="3" />
      <path d="M12 2.5v3M12 18.5v3M2.5 12h3M18.5 12h3M5.3 5.3l2.1 2.1M16.6 16.6l2.1 2.1M5.3 18.7l2.1-2.1M16.6 7.4l2.1-2.1" />
    </svg>
  );
}

/** A speech bubble: a quick question outside any project. */
export function QuestionIcon() {
  return (
    <svg {...common} className="icon">
      <path d="M4 5.5h16v10H10l-4.5 3.5v-3.5H4z" />
      <path d="M10.2 8.6a1.9 1.9 0 1 1 2.6 1.8c-.5.2-.8.6-.8 1.1v.3M12 13.4h.01" />
    </svg>
  );
}

/** « when it collapses, » when it expands. */
export function CollapseIcon({ collapsed }: { collapsed: boolean }) {
  return (
    <svg {...common} className="icon">
      {collapsed ? <path d="M6 6l6 6-6 6M13 6l6 6-6 6" /> : <path d="M18 6l-6 6 6 6M11 6l-6 6 6 6" />}
    </svg>
  );
}
