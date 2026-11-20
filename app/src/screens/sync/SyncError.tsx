// A failed sync in plain words: no connection, no access to the repo, or a
// generic failure with git's own message behind "Detalles". The needles
// follow the core's (`PushProblem::from_git` in domain/ship.rs).

import { useId, useState } from "react";

import { Button } from "../../components/ui/Button";
import { useT, type MessageKey } from "../../i18n";

export type SyncErrorKind = "offline" | "access" | "other";

const OFFLINE = [
  "sin conexión",
  "could not resolve host",
  "could not resolve hostname",
  "temporary failure in name resolution",
  "failed to connect",
  "network is unreachable",
  "connection timed out",
  "operation timed out",
  "connection refused",
  "connection reset",
];

const ACCESS = [
  "authentication failed",
  "could not read username",
  "could not read password",
  "permission denied",
  "terminal prompts disabled",
  "invalid username or password",
  "host key verification failed",
  "repository not found",
  "could not read from remote repository",
  "the requested url returned error: 403",
  "the requested url returned error: 401",
];

/** What kind of failure git's message describes; the network is checked first. */
export function syncErrorKind(message: string): SyncErrorKind {
  const lower = message.toLowerCase();
  if (OFFLINE.some((needle) => lower.includes(needle))) return "offline";
  if (ACCESS.some((needle) => lower.includes(needle))) return "access";
  return "other";
}

const TEXT: Record<SyncErrorKind, { title: MessageKey; text: MessageKey }> = {
  offline: { title: "sync.error.offline", text: "sync.error.offlineText" },
  access: { title: "sync.error.access", text: "sync.error.accessText" },
  other: { title: "sync.failed", text: "sync.error.otherText" },
};

export function syncErrorTitle(message: string): MessageKey {
  return TEXT[syncErrorKind(message)].title;
}

/** The failure's sentence, "Reintentar" and git's message behind "Detalles". */
export function SyncErrorBox({
  message,
  retrying,
  onRetry,
}: {
  message: string;
  retrying: boolean;
  onRetry: () => void;
}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const id = useId();
  const { title, text } = TEXT[syncErrorKind(message)];
  return (
    <div className="sync-error" role="alert">
      <div className="sync-error__text">
        <strong>{t(title)}</strong>
        <span className="small">{t(text)}</span>
      </div>
      <Button small variant="primary" disabled={retrying} onClick={onRetry}>
        {retrying ? t("sync.syncing") : t("common.retry")}
      </Button>
      <button
        type="button"
        className="disclosure disclosure--quiet sync-error__toggle"
        aria-expanded={open}
        aria-controls={id}
        onClick={() => setOpen((o) => !o)}
      >
        <span className="disclosure__mark" aria-hidden>
          ›
        </span>
        {t("sync.error.details")}
      </button>
      <pre id={id} className="sync-error__detail mono small" hidden={!open}>
        {message}
      </pre>
    </div>
  );
}
