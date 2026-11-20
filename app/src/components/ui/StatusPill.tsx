// The one state pill of a task: a dot in the state's color and a word, the
// same in the task's header, Inicio and the project's list.

import type { BoardState, Task } from "../../api/types";
import { useLanguage, useT, type Language, type MessageKey, type Translate } from "../../i18n";

export type Tone = "needs" | "blocked" | "working" | "done" | "failed" | "closed";

export const PILL_TONE: Record<BoardState, Tone> = {
  plan: "needs",
  permission: "needs",
  limited: "blocked",
  working: "working",
  review: "done",
  answered: "done",
  failed: "failed",
  accepted: "closed",
  cancelled: "closed",
};

const PILL_LABEL: Record<Exclude<BoardState, "limited" | "accepted">, MessageKey> = {
  working: "pill.working",
  plan: "pill.plan",
  permission: "pill.permission",
  review: "pill.review",
  answered: "pill.answered",
  failed: "pill.failed",
  cancelled: "pill.cancelled",
};

/** "13:21": the clock time a paused task resumes at. */
function clock(seconds: number, language: Language): string {
  return new Date(seconds * 1000).toLocaleTimeString(language, { hour: "2-digit", minute: "2-digit" });
}

export function pillLabel(state: BoardState, task: Task, t: Translate, language: Language): string {
  if (state === "limited") {
    const wait = task.wait;
    if (wait?.ready) return t("pill.limitedReady");
    if (wait?.until) return t("pill.limitedUntil", { time: clock(wait.until, language) });
    return t("pill.limited");
  }
  if (state === "accepted") {
    if (task.ship?.kept) return t("pill.kept");
    const branch = task.ship?.branch ?? (task.merge ? task.workspace?.baseBranch : undefined);
    return branch ? t("pill.merged", { branch }) : t("pill.accepted");
  }
  return t(PILL_LABEL[state]);
}

export function StatusPill({ state, task }: { state: BoardState; task: Task }) {
  const t = useT();
  const language = useLanguage();
  return (
    <span className={`status-pill status-pill--${PILL_TONE[state]}`}>
      <span className="status-pill__dot" aria-hidden />
      {pillLabel(state, task, t, language)}
    </span>
  );
}
