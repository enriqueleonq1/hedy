// Line icons of the ship flow and the folder status. Decorative: the control
// carries the name.

const common = {
  width: 14,
  height: 14,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 2,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  "aria-hidden": true,
  focusable: false,
  className: "icon",
};

/** A git branch. */
export function BranchIcon() {
  return (
    <svg {...common}>
      <circle cx="6" cy="5" r="2" />
      <circle cx="6" cy="19" r="2" />
      <circle cx="18" cy="7" r="2" />
      <path d="M6 7v10M18 9c0 5-6 4-11 8.5" />
    </svg>
  );
}

/** ↑: push. */
export function UpIcon() {
  return (
    <svg {...common}>
      <path d="M12 19V5M6 11l6-6 6 6" />
    </svg>
  );
}

/** ↓: pull. */
export function DownIcon() {
  return (
    <svg {...common}>
      <path d="M12 5v14M6 13l6 6 6-6" />
    </svg>
  );
}

/** ✦: ask the AI. */
export function SparkleIcon() {
  return (
    <svg {...common}>
      <path d="M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8z" />
    </svg>
  );
}

/** ↗: opens outside Hedy. */
export function ExternalIcon() {
  return (
    <svg {...common} width={12} height={12}>
      <path d="M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5" />
    </svg>
  );
}

/** A pull request. */
export function PullRequestIcon() {
  return (
    <svg {...common}>
      <circle cx="6" cy="6" r="2" />
      <circle cx="6" cy="18" r="2" />
      <circle cx="18" cy="18" r="2" />
      <path d="M6 8v8M18 16V9a3 3 0 0 0-3-3h-4M13 4l-2 2 2 2" />
    </svg>
  );
}
