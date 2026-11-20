// Line icons of the code views (16 px, 1.6 stroke): tabs, toolbars, row actions.
// Decorative: the control around them carries the name.

import type { ReactNode } from "react";

function Icon({ children, size = 16 }: { children: ReactNode; size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.5}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
      focusable={false}
      className="icon"
    >
      {children}
    </svg>
  );
}

export const FilesIcon = () => (
  <Icon>
    <path d="M5.5 2.5h4l3 3v6a1 1 0 0 1-1 1h-6a1 1 0 0 1-1-1v-8a1 1 0 0 1 1-1Z" />
    <path d="M9.5 2.5v3h3M2.5 5v8a1 1 0 0 0 1 1h6" />
  </Icon>
);

/** Two files compared: the review. */
export const ReviewIcon = () => (
  <Icon>
    <circle cx="4.5" cy="4" r="1.75" />
    <circle cx="11.5" cy="12" r="1.75" />
    <path d="M4.5 5.75V14M11.5 10.25V6.5a2 2 0 0 0-2-2H7.5m0 0L9 3m-1.5 1.5L9 6" />
  </Icon>
);

export const TerminalIcon = () => (
  <Icon>
    <rect x="1.75" y="2.75" width="12.5" height="10.5" rx="1.5" />
    <path d="m4.5 6.5 2 1.75-2 1.75M8.5 10.5h3" />
  </Icon>
);

export const PreviewIcon = () => (
  <Icon>
    <path d="M1.75 8s2.25-4.25 6.25-4.25S14.25 8 14.25 8 12 12.25 8 12.25 1.75 8 1.75 8Z" />
    <circle cx="8" cy="8" r="1.9" />
  </Icon>
);

export const DiffIcon = () => (
  <Icon>
    <path d="M8 2.5v5M5.5 5h5M5.5 11h5M3.5 13.5h9" />
  </Icon>
);

export const HistoryIcon = () => (
  <Icon>
    <path d="M2.5 8a5.5 5.5 0 1 0 1.6-3.9L2.5 5.7" />
    <path d="M2.5 2.75V5.7h2.95M8 5v3.25l2 1.25" />
  </Icon>
);

/** Who wrote each line. */
export const BlameIcon = () => (
  <Icon>
    <circle cx="4.25" cy="4.5" r="1.75" />
    <path d="M1.75 10c.4-1.6 1.3-2.5 2.5-2.5s2.1.9 2.5 2.5M9 3.5h5.25M9 6.5h5.25M9 9.5h5.25M1.75 12.5h12.5" />
  </Icon>
);

/** Back in time: before the AI. */
export const RewindIcon = () => (
  <Icon>
    <path d="M7.5 4 3 8l4.5 4V4ZM13 4 8.5 8l4.5 4V4Z" />
  </Icon>
);

export const ExternalIcon = () => (
  <Icon>
    <path d="M9.5 2.5h4v4M13.5 2.5 8 8M11.5 9.5v3a1 1 0 0 1-1 1h-7a1 1 0 0 1-1-1v-7a1 1 0 0 1 1-1h3" />
  </Icon>
);

export const CopyIcon = () => (
  <Icon>
    <rect x="5.25" y="5.25" width="8.5" height="8.5" rx="1.5" />
    <path d="M10.75 5.25V3.5a1.25 1.25 0 0 0-1.25-1.25H3.5A1.25 1.25 0 0 0 2.25 3.5v6c0 .7.55 1.25 1.25 1.25h1.75" />
  </Icon>
);

export const MoreIcon = () => (
  <Icon>
    <circle cx="3.5" cy="8" r=".6" fill="currentColor" />
    <circle cx="8" cy="8" r=".6" fill="currentColor" />
    <circle cx="12.5" cy="8" r=".6" fill="currentColor" />
  </Icon>
);

export const CollapseAllIcon = () => (
  <Icon>
    <rect x="2.25" y="2.25" width="11.5" height="11.5" rx="2" />
    <path d="M5.5 8h5" />
  </Icon>
);

export const RefreshIcon = () => (
  <Icon>
    <path d="M13.25 8A5.25 5.25 0 1 1 11.7 4.3l1.55 1.45" />
    <path d="M13.25 2.75v3h-3" />
  </Icon>
);

/** Find the open file in the tree. */
export const TargetIcon = () => (
  <Icon>
    <circle cx="8" cy="8" r="4.5" />
    <circle cx="8" cy="8" r="1.25" fill="currentColor" />
    <path d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2" />
  </Icon>
);

/** Only changed files. */
export const ChangedIcon = () => (
  <Icon>
    <circle cx="8" cy="8" r="2.5" fill="currentColor" stroke="none" />
    <circle cx="8" cy="8" r="5.5" />
  </Icon>
);

/** Show ignored files. */
export const IgnoredIcon = () => (
  <Icon>
    <path d="M1.75 8s2.25-4.25 6.25-4.25S14.25 8 14.25 8 12 12.25 8 12.25 1.75 8 1.75 8Z" />
    <path d="m2.5 13.5 11-11" />
  </Icon>
);

export const LockIcon = ({ size = 16 }: { size?: number }) => (
  <Icon size={size}>
    <rect x="3.25" y="7" width="9.5" height="6.75" rx="1.5" />
    <path d="M5.25 7V5a2.75 2.75 0 0 1 5.5 0v2" />
  </Icon>
);

export const BackIcon = () => (
  <Icon>
    <path d="M9.5 3.5 5 8l4.5 4.5" />
  </Icon>
);

export const CloseIcon = () => (
  <Icon>
    <path d="m4 4 8 8M12 4l-8 8" />
  </Icon>
);

export const FolderOpenIcon = () => (
  <Icon>
    <path d="M2 12.5V4a1 1 0 0 1 1-1h3l1.5 1.5H12a1 1 0 0 1 1 1V7" />
    <path d="M2 12.5 3.6 7.7a1 1 0 0 1 .95-.7h9.2a.6.6 0 0 1 .57.79l-1.4 4.2a.75.75 0 0 1-.7.51H2Z" />
  </Icon>
);

export const SparkIcon = () => (
  <Icon>
    <path d="M8 1.75c.4 2.9 1.35 3.85 4.25 4.25C9.35 6.4 8.4 7.35 8 10.25 7.6 7.35 6.65 6.4 3.75 6c2.9-.4 3.85-1.35 4.25-4.25ZM12.25 10.5c.2 1.3.6 1.75 1.75 1.75-1.15.2-1.55.6-1.75 1.75-.2-1.15-.6-1.55-1.75-1.75 1.15 0 1.55-.45 1.75-1.75Z" />
  </Icon>
);

export const CommentIcon = () => (
  <Icon>
    <path d="M2.75 3.75a1 1 0 0 1 1-1h8.5a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1H7l-2.75 2.5v-2.5h-.5a1 1 0 0 1-1-1v-6Z" />
  </Icon>
);

export const LineIcon = () => (
  <Icon>
    <path d="M2.5 4h2M2.5 8h2M2.5 12h2M7 4h6.5M7 8h6.5M7 12h4" />
  </Icon>
);

/** Editing: a pencil. */
export const PencilIcon = () => (
  <Icon>
    <path d="M10.6 3.1a1.5 1.5 0 0 1 2.1 0l.2.2a1.5 1.5 0 0 1 0 2.1L6 12.3l-3 .7.7-3Z" />
    <path d="m9.5 4.2 2.3 2.3" />
  </Icon>
);

export const UndoIcon = () => (
  <Icon>
    <path d="M5.5 4 3 6.5 5.5 9" />
    <path d="M3 6.5h6.5a3.5 3.5 0 0 1 0 7H7" />
  </Icon>
);

export const RedoIcon = () => (
  <Icon>
    <path d="M10.5 4 13 6.5 10.5 9" />
    <path d="M13 6.5H6.5a3.5 3.5 0 0 0 0 7H9" />
  </Icon>
);

export const NewFileIcon = () => (
  <Icon>
    <path d="M9 2.5H4.5a1 1 0 0 0-1 1v9a1 1 0 0 0 1 1h7a1 1 0 0 0 1-1V6Z" />
    <path d="M9 2.5V6h3.5M8 8.5v3M6.5 10h3" />
  </Icon>
);

export const NewFolderIcon = () => (
  <Icon>
    <path d="M2.5 4.5a1 1 0 0 1 1-1h3l1.5 1.5h4.5a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1h-9a1 1 0 0 1-1-1Z" />
    <path d="M8 7.5v3.5M6.25 9.25h3.5" />
  </Icon>
);

export const TrashIcon = () => (
  <Icon>
    <path d="M3 4.5h10M6.5 4.5v-1a1 1 0 0 1 1-1h1a1 1 0 0 1 1 1v1" />
    <path d="M4.5 4.5l.6 8.1a1 1 0 0 0 1 .9h3.8a1 1 0 0 0 1-.9l.6-8.1" />
  </Icon>
);

export const RenameIcon = () => (
  <Icon>
    <path d="M2.5 5.5h7v5h-7Z" />
    <path d="M12 3.5v9M10.75 3.5h2.5M10.75 12.5h2.5" />
  </Icon>
);

export const DuplicateIcon = () => (
  <Icon>
    <rect x="5.5" y="5.5" width="7.5" height="7.5" rx="1" />
    <path d="M3.5 10.5h-.25a.75.75 0 0 1-.75-.75v-6.5a.75.75 0 0 1 .75-.75h6.5a.75.75 0 0 1 .75.75v.25" />
  </Icon>
);
