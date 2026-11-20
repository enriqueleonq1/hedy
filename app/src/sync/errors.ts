// A failed sync in plain words: no connection, no access to the repo, or a
// generic failure. The needles follow the core's (`PushProblem::from_git` in
// domain/ship.rs). Shared by the sync and team hooks and their screens.

import type { MessageKey } from "../i18n";

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

export const SYNC_ERROR_TEXT: Record<SyncErrorKind, { title: MessageKey; text: MessageKey }> = {
  offline: { title: "sync.error.offline", text: "sync.error.offlineText" },
  access: { title: "sync.error.access", text: "sync.error.accessText" },
  other: { title: "sync.failed", text: "sync.error.otherText" },
};

export function syncErrorTitle(message: string): MessageKey {
  return SYNC_ERROR_TEXT[syncErrorKind(message)].title;
}
