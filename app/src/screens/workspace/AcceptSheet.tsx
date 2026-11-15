import { useId, useState } from "react";
import { useQuery } from "@tanstack/react-query";

import { errorMessage, useAction, useApi } from "../../api/context";
import type { AcceptPreview, Task } from "../../api/types";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { useToast } from "../../components/ui/Toast";
import { useT } from "../../i18n";
import { ACTIVITY_KEY, tasksKey } from "../../tasks/hooks";
import "../../styles/review.css";

/** How long the "Deshacer" toast stays after accepting. */
export const UNDO_TOAST_MS = 12_000;

/** What accepting changes in the project: its branch, commits and files. */
const changedKeys = (task: Task) => [tasksKey(task.project), ["commits", task.project], ["status", task.project], ACTIVITY_KEY];

/**
 * The review before "Aceptar": target branch, files and lines, the commit
 * message (editable) and the last test run. Failed or missing tests are a
 * warning, never a block: the user decides.
 */
export function AcceptSheet({
  task,
  onClose,
  onAccepted,
}: {
  task: Task;
  onClose: () => void;
  /** After the merge, with the branch it went into (for the undo toast). */
  onAccepted: (targetBranch: string) => void;
}) {
  const api = useApi();
  const t = useT();
  const preview = useQuery({ queryKey: ["acceptPreview", task.id], queryFn: () => api.acceptPreview(task.id), gcTime: 0 });
  const accept = useAction((a, message: string) => a.acceptTask(task.id, message), {
    invalidate: changedKeys(task),
    onSuccess: () => {
      onClose();
      onAccepted(preview.data?.targetBranch ?? task.workspace?.baseBranch ?? "");
    },
  });

  return (
    <Dialog
      title={t("review.title")}
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button
            variant="primary"
            form="accept-sheet"
            type="submit"
            disabled={!preview.data || accept.isPending}
          >
            {t("review.confirm")}
          </Button>
        </>
      }
    >
      {preview.isPending && <p className="muted">{t("review.loading")}</p>}
      {preview.isError && (
        <p className="error-text" role="alert">
          {errorMessage(preview.error, t)}
        </p>
      )}
      {preview.data && (
        <PreviewForm key={preview.dataUpdatedAt} preview={preview.data} onSubmit={(message) => accept.mutate(message)} />
      )}
    </Dialog>
  );
}

function PreviewForm({ preview, onSubmit }: { preview: AcceptPreview; onSubmit: (message: string) => void }) {
  const t = useT();
  const id = useId();
  const [message, setMessage] = useState(preview.message);
  const elsewhere = preview.checkoutBranch !== null && preview.checkoutBranch !== preview.targetBranch;
  const tests = preview.tests;

  return (
    <form
      id="accept-sheet"
      className="accept-sheet"
      onSubmit={(e) => {
        e.preventDefault();
        onSubmit(message);
      }}
    >
      <p>
        {t("review.into", { target: preview.targetBranch, branch: preview.branch })}
      </p>
      <p className="accept-sheet__stats">
        <span>{t("review.files", { n: preview.files })}</span>
        <span aria-hidden>·</span>
        <span>
          <span className="plus" aria-hidden>
            +{preview.additions}
          </span>{" "}
          <span className="minus" aria-hidden>
            −{preview.deletions}
          </span>
          <span className="sr-only">{t("review.lines", { added: preview.additions, removed: preview.deletions })}</span>
        </span>
      </p>
      {elsewhere && (
        <p className="accept-sheet__warn" role="alert">
          {t("review.checkout", { current: preview.checkoutBranch ?? "", target: preview.targetBranch })}
        </p>
      )}
      <p className={`accept-sheet__tests ${tests?.passed ? "accept-sheet__tests--ok" : "accept-sheet__warn"}`}>
        <span aria-hidden>{tests?.passed ? "✓" : "!"}</span>{" "}
        {tests
          ? tests.passed
            ? t("review.tests.passed", { command: tests.command })
            : t("review.tests.failed", { command: tests.command })
          : t("review.tests.none")}
      </p>
      <div className="field">
        <label className="field__label" htmlFor={id}>
          {t("review.message")}
        </label>
        <textarea
          id={id}
          className="mono accept-sheet__message"
          rows={4}
          value={message}
          aria-describedby={`${id}-hint`}
          onChange={(e) => setMessage(e.target.value)}
        />
        <span id={`${id}-hint`} className="muted small">
          {t("review.messageHint")}
        </span>
      </div>
    </form>
  );
}

/**
 * "Deshacer": the backend checks it is safe, or says why not. Lives in the
 * changes panel, which stays mounted after the sheet closes.
 */
export function useUndoAccept(task: Task | null) {
  const t = useT();
  const toast = useToast();
  return useAction((api, id: string) => api.undoAccept(id), {
    invalidate: task ? [...changedKeys(task), ["diff", task.id]] : [],
    onSuccess: () => toast.info(t("review.undone")),
  });
}

/** After accepting: a toast with "Deshacer" for a few seconds. */
export function useAcceptedToast(task: Task | null) {
  const t = useT();
  const toast = useToast();
  const undo = useUndoAccept(task);
  return {
    undo,
    show: (taskId: string, branch: string) =>
      toast.info(t("review.merged", { branch }), {
        durationMs: UNDO_TOAST_MS,
        action: { label: t("review.undo"), onClick: () => undo.mutate(taskId) },
      }),
  };
}
