import type { AgentEvent, Task } from "../../api/types";
import { useAction } from "../../api/context";
import { Button } from "../../components/ui/Button";
import { useT } from "../../i18n";
import { listKeyFor, useSetQueue, useTaskDiff } from "../../tasks/hooks";
import { stoppedByLimit } from "../../tasks/state";

/** The last test run of the task, if the agent ran its tests. */
function lastTests(events: AgentEvent[]): boolean | null {
  const run = events.findLast((e) => e.kind === "testsRun");
  return run?.kind === "testsRun" ? run.passed : null;
}

/** Whether the task's history ends on the end of a turn (nothing came after it). */
function endsOnFinish(events: AgentEvent[]): boolean {
  const last = events.findLast((e) => e.kind !== "usage" && e.kind !== "rateLimit");
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
  const facts = [
    diff.isSuccess ? (files === 0 ? t("end.noFiles") : files === 1 ? t("end.filesOne") : t("end.files", { n: files })) : null,
    tests === null ? null : tests ? t("end.testsPassed") : t("end.testsFailed"),
  ].filter(Boolean);
  return (
    <section className="end-card" aria-label={t("end.title")}>
      <h3>{t("end.title")}</h3>
      {facts.length > 0 && <p className="muted">{facts.join(" · ")}</p>}
      <div className="actions">
        {files > 0 && (
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
