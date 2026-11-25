import type { AgentEvent, Task } from "../../api/types";
import { useAction } from "../../api/context";
import { Button, ConfirmButton } from "../../components/ui/Button";
import { useT } from "../../i18n";
import { ACTIVITY_KEY, listKeyFor, useSetQueue, useTaskDiff } from "../../tasks/hooks";
import { stoppedByLimit } from "../../tasks/state";

/** The last test run of the task: the agent's, or Hedy's check after its turn. */
function lastTests(events: AgentEvent[]): boolean | null {
  const run = events.findLast((e) => e.kind === "testsRun" || e.kind === "checkRan");
  return run?.kind === "testsRun" || run?.kind === "checkRan" ? run.passed : null;
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
 * that finished well asks for the review (and to fix failing tests); one that
 * failed offers to retry. Nothing for answers, pauses or closed tasks.
 */
export function EndCard({ task, events, onReview }: { task: Task; events: AgentEvent[]; onReview: () => void }) {
  const t = useT();
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
  const facts = [
    diff.isSuccess ? (files === 1 ? t("end.filesOne") : t("end.files", { n: files })) : null,
    tests === null ? null : tests ? t("end.testsPassed") : t("end.testsFailed"),
  ].filter(Boolean);
  return (
    <section className="end-card" aria-label={t("end.title")}>
      <h3>{t("end.title")}</h3>
      {facts.length > 0 && <p className="muted">{facts.join(" · ")}</p>}
      <div className="actions">
        {(files > 0 || diff.isError) && (
          <Button variant="primary" onClick={onReview}>
            {t("end.review")}
          </Button>
        )}
        {tests === false && (
          <Button disabled={send.isPending} onClick={() => send.mutate(t("end.fixTestsPrompt"))}>
            {t("end.fixTests")}
          </Button>
        )}
      </div>
    </section>
  );
}
