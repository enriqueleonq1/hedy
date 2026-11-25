import type { AgentEvent, Task } from "../../api/types";
import { useAction } from "../../api/context";
import { Button, ConfirmButton } from "../../components/ui/Button";
import { useToast } from "../../components/ui/Toast";
import { useT } from "../../i18n";
import { ACTIVITY_KEY, listKeyFor, useSetQueue, useTaskDiff } from "../../tasks/hooks";
import { stoppedByLimit } from "../../tasks/state";

/** The last test run of the task: the agent's, or Hedy's check after its turn. */
function lastTests(events: AgentEvent[]): { passed: boolean; byHedy: boolean } | null {
  const run = events.findLast((e) => e.kind === "testsRun" || e.kind === "checkRan");
  if (run?.kind === "checkRan") return { passed: run.passed, byHedy: true };
  if (run?.kind === "testsRun") return { passed: run.passed, byHedy: false };
  return null;
}

/** The agent's latest answer, for the footer's copy button. */
function lastAnswer(events: AgentEvent[]): string | null {
  const message = events.findLast((e) => e.kind === "message");
  return message?.kind === "message" && message.text.trim() ? message.text : null;
}

/** What comes after a turn's end without being more work: usage, limits and Hedy's checks. */
const AFTER_TURN = new Set<AgentEvent["kind"]>([
  "usage",
  "rateLimit",
  "limitWarning",
  "checkRan",
  "checksSkipped",
  "reviewRan",
]);

/** Whether the task's history ends on the end of a turn (nothing came after it). */
function endsOnFinish(events: AgentEvent[]): boolean {
  const last = events.findLast((e) => !AFTER_TURN.has(e.kind));
  return last?.kind === "finished";
}

/**
 * What comes next when the agent's turn ends, under the chat: a project task
 * that finished well closes the turn with what changed, the review and a next
 * step or two (and asks to fix failing tests); one that failed offers to
 * retry. Nothing for answers, pauses or closed tasks.
 */
export function EndCard({ task, events, onReview }: { task: Task; events: AgentEvent[]; onReview: () => void }) {
  const t = useT();
  const toast = useToast();
  const setQueue = useSetQueue();
  const ready = task.status === "review" && task.mode === "project" && endsOnFinish(events);
  const failed = task.status === "failed" && !task.live && !stoppedByLimit(events);
  const diff = useTaskDiff(ready ? task.id : null, ready);
  const send = useAction((api, text: string) => api.queueMessage(task.id, text), {
    invalidate: [listKeyFor(task)],
    onSuccess: (left) => setQueue(task.id, left),
  });
  const discard = useAction((api) => api.discardTask(task.id), { invalidate: [listKeyFor(task), ACTIVITY_KEY] });
  if (!ready && !failed) return null;

  if (failed) {
    return (
      <section className="end-card end-card--failed" aria-label={t("end.failedTitle")}>
        <h3>{t("end.failedTitle")}</h3>
        <p className="muted">{t("end.failedHint")}</p>
        <div className="actions">
          <Button variant="primary" disabled={send.isPending} onClick={() => send.mutate(t("end.retryPrompt"))}>
            {t("end.retry")}
          </Button>
        </div>
      </section>
    );
  }

  const files = diff.data?.length ?? 0;
  const tests = lastTests(events);
  // Nothing changed: what follows is telling it what to do, or closing the task.
  if (diff.isSuccess && files === 0) {
    return (
      <section className="end-card" aria-label={t("end.noChangesTitle")}>
        <h3>{t("end.noChangesTitle")}</h3>
        <p className="muted">{t("end.noChangesHint")}</p>
        <div className="actions">
          <ConfirmButton small confirmLabel={t("workspace.changes.discardConfirm")} onConfirm={() => discard.mutate(undefined)}>
            {t("end.discard")}
          </ConfirmButton>
        </div>
      </section>
    );
  }
  const answer = lastAnswer(events);
  const copy = (text: string) =>
    void navigator.clipboard?.writeText(text).then(
      () => toast.info(t("end.copied")),
      () => toast.info(t("palette.copyFailed")),
    );
  const testsText = tests && (tests.byHedy
    ? t(tests.passed ? "end.hedyTestsPass" : "end.hedyTestsFail")
    : t(tests.passed ? "end.testsPass" : "end.testsFail"));
  return (
    <section className="end-card turn-foot" aria-label={t("end.title")}>
      <div className="turn-foot__bar">
        {diff.isSuccess && <span className="turn-chip">{files === 1 ? t("end.filesOne") : t("end.files", { n: files })}</span>}
        {tests && (
          <span className={`turn-chip turn-chip--${tests.passed ? "passed" : "failed"}`}>
            <span className="turn-chip__mark" aria-hidden>
              {tests.passed ? "✓" : "✗"}
            </span>
            {testsText}
          </span>
        )}
        <span className="turn-foot__actions">
          {answer && (
            <Button small variant="ghost" aria-label={t("end.copyLabel")} onClick={() => copy(answer)}>
              {t("end.copy")}
            </Button>
          )}
          {tests?.passed === false && (
            <Button small disabled={send.isPending} onClick={() => send.mutate(t("end.fixTestsPrompt"))}>
              {t("end.fixTests")}
            </Button>
          )}
          {(files > 0 || diff.isError) && (
            <Button small variant="primary" onClick={onReview}>
              {t("end.review")}
            </Button>
          )}
        </span>
      </div>
      {/* The next step or two, sent as a message: only once the turn went well. */}
      {tests?.passed !== false && (
        <div className="turn-foot__suggestions">
          <button type="button" className="suggestion" disabled={send.isPending} onClick={() => send.mutate(t("end.explainPrompt"))}>
            {t("end.explain")}
          </button>
          <button type="button" className="suggestion" disabled={send.isPending} onClick={() => send.mutate(t("end.moreTestsPrompt"))}>
            {t("end.moreTests")}
          </button>
        </div>
      )}
    </section>
  );
}
