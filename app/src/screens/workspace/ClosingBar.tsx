import type { Task } from "../../api/types";
import { Button } from "../../components/ui/Button";
import { useT } from "../../i18n";
import { useUndoAccept } from "./AcceptSheet";

/**
 * Where the composer was, once a task is closed: what became of its work and
 * the steps that follow from there. Writing here would start another task,
 * so it says so with buttons instead of a box that looks like "keep going".
 */
export function ClosingBar({
  task,
  onOpenPanel,
  onNewTask,
}: {
  task: Task;
  /** Shows the side panel, where publishing and the pull request live. */
  onOpenPanel: () => void;
  /** A new task in the project; `prompt` starts its composer with that request. */
  onNewTask: (prompt?: string) => void;
}) {
  const t = useT();
  const undo = useUndoAccept(task);
  const kept = !!task.ship?.kept;
  const branch = task.ship?.branch ?? task.workspace?.baseBranch ?? "";
  const text =
    task.status === "discarded"
      ? t("closed.discarded")
      : kept
        ? t("closed.kept", { branch: task.workspace?.branch ?? branch })
        : branch && (task.merge || task.ship)
          ? t("closed.merged", { branch })
          : t("closed.accepted");

  return (
    <div className="closing-bar" role="region" aria-label={t("closed.label")}>
      <p>{text}</p>
      <div className="actions">
        {task.status === "discarded" ? (
          <Button variant="primary" onClick={() => onNewTask(task.prompt)}>
            {t("closed.similar")}
          </Button>
        ) : (
          <>
            {task.merge && !kept && (
              <Button disabled={undo.isPending} onClick={() => undo.mutate(task.id)}>
                {t("closed.undo")}
              </Button>
            )}
            {task.ship && (
              <Button onClick={onOpenPanel}>{kept ? t("closed.openPr") : t("closed.publish")}</Button>
            )}
          </>
        )}
        <Button variant={task.status === "discarded" ? "secondary" : "primary"} onClick={() => onNewTask()}>
          {t("closed.newTask")}
        </Button>
      </div>
    </div>
  );
}
