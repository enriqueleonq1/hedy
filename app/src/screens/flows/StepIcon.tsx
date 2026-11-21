// A step's icon and its tint (a theme color, never as text): writers in the
// activity color, readers in the accent, checks in green, the user's OK in
// the warning color, the group at once in blue.

import type { FlowNode, RoleDef } from "../../api/types";

export function stepTint(node: FlowNode, roles: RoleDef[]): string {
  switch (node.kind) {
    case "role":
      return roles.find((r) => r.id === node.role)?.can.write ? "var(--activity)" : "var(--accent)";
    case "parallel":
      return "var(--term-blue)";
    case "condition":
      return "var(--success)";
    case "approval":
      return "var(--warning)";
    default:
      return "var(--muted)";
  }
}

export function StepIcon({ node, roles, size = 24 }: { node: FlowNode; roles: RoleDef[]; size?: number }) {
  const common = {
    "aria-hidden": true,
    width: size,
    height: size,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.8,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
  };
  switch (node.kind) {
    case "start":
      return (
        <svg {...common}>
          <path d="M13 2L4 14h7l-1 8 9-12h-7z" />
        </svg>
      );
    case "end":
      return (
        <svg {...common}>
          <path d="M5 21V4" />
          <path d="M5 4h11l-2 4 2 4H5" />
        </svg>
      );
    case "approval":
      return (
        <svg {...common}>
          <circle cx="12" cy="8" r="4" />
          <path d="M4 21c1.5-4 4.5-6 8-6s6.5 2 8 6" />
        </svg>
      );
    case "parallel":
      return (
        <svg {...common}>
          <path d="M4 6h10M4 12h16M4 18h10" />
          <path d="M17 4l3 2-3 2M17 16l3 2-3 2" />
        </svg>
      );
    case "condition":
      return node.check === "tests" ? (
        <svg {...common}>
          <path d="M9 3h6M10 3v6l-5 9a2 2 0 002 3h10a2 2 0 002-3l-5-9V3" />
          <path d="M7.5 15h9" />
        </svg>
      ) : node.check === "rules" ? (
        <svg {...common}>
          <path d="M6 3h9l3 3v15H6z" />
          <path d="M9 10h6M9 14h6M9 18h3" />
        </svg>
      ) : (
        <svg {...common}>
          <path d="M12 3l7 3v5c0 5-3.5 8.5-7 10-3.5-1.5-7-5-7-10V6z" />
          <path d="M9 12l2 2 4-4" />
        </svg>
      );
    case "role": {
      const role = roles.find((r) => r.id === node.role);
      if (role?.can.write) {
        return (
          <svg {...common}>
            <path d="M8 7l-5 5 5 5" />
            <path d="M16 7l5 5-5 5" />
          </svg>
        );
      }
      if (role?.delivers === "verdict") {
        return (
          <svg {...common}>
            <circle cx="11" cy="11" r="6" />
            <path d="M20 20l-4.5-4.5" />
          </svg>
        );
      }
      return (
        <svg {...common}>
          <path d="M12 3l8 4-8 4-8-4z" />
          <path d="M4 12l8 4 8-4" />
          <path d="M4 17l8 4 8-4" />
        </svg>
      );
    }
  }
}
