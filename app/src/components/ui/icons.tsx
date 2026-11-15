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

const small = { ...common, width: 14, height: 14 };

/** ⌄: a control that opens a list. */
export function ChevronIcon() {
  return (
    <svg {...small} width={12} height={12} strokeWidth={2.4} className="icon">
      <path d="m6 9 6 6 6-6" />
    </svg>
  );
}

/** ✓: the chosen item of a list. */
export function CheckIcon() {
  return (
    <svg {...small} strokeWidth={2.6} className="icon">
      <path d="M5 12.5l4.5 4.5L19 7.5" />
    </svg>
  );
}

/** ↑: send. */
export function SendIcon() {
  return (
    <svg {...common} strokeWidth={2.6} className="icon">
      <path d="M12 19V5M5 12l7-7 7 7" />
    </svg>
  );
}

/** </>: a task that changes the project's code. */
export function CodeIcon() {
  return (
    <svg {...small} className="icon">
      <path d="M8.5 7 3.5 12l5 5M15.5 7l5 5-5 5" />
    </svg>
  );
}

/** A speech bubble, small: a question that changes nothing. */
export function AskIcon() {
  return (
    <svg {...small} className="icon">
      <path d="M21 12a8 8 0 0 1-11.8 7L4 20l1.1-4.6A8 8 0 1 1 21 12z" />
    </svg>
  );
}

/** A magnifying glass: research, reading before answering. */
export function ResearchIcon() {
  return (
    <svg {...small} className="icon">
      <circle cx="11" cy="11" r="6.5" />
      <path d="m16 16 4.5 4.5" />
    </svg>
  );
}

/** A checklist: plan before changing anything. */
export function PlanIcon() {
  return (
    <svg {...small} className="icon">
      <path d="M10 6.5h10M10 12h10M10 17.5h10M3.5 6.5l1.3 1.3 2.2-2.4M3.5 12l1.3 1.3 2.2-2.4M3.5 17.5l1.3 1.3 2.2-2.4" />
    </svg>
  );
}

/** A stack of cards: prompt templates. */
export function TemplateIcon() {
  return (
    <svg {...common} width={16} height={16} className="icon">
      <rect x="4" y="7" width="13" height="13" rx="2" />
      <path d="M8 4h10a2 2 0 0 1 2 2v10" />
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
