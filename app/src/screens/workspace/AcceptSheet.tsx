import { useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";

import { errorMessage, useAction, useApi } from "../../api/context";
import type { AcceptMode, AcceptPreview, CheckResult, ReviewWarning, Task } from "../../api/types";
import { modKey } from "../../components/palette/keys";
import { Button } from "../../components/ui/Button";
import { EmptyState } from "../../components/ui/EmptyState";
import { Switch } from "../../components/ui/controls";
import { Dialog } from "../../components/ui/Dialog";
import { useToast } from "../../components/ui/Toast";
import { useT, type Translate } from "../../i18n";
import { ACTIVITY_KEY, tasksKey, useSetQueue } from "../../tasks/hooks";
import { AcceptModeChoice, CommitMessage } from "./ship/CommitMessage";
import { folderKey, markJustAccepted, shipKey, useFolderStatus } from "./ship/useShip";
import "../../styles/review.css";

/** How long the "Deshacer" toast stays after accepting. */
export const UNDO_TOAST_MS = 12_000;

/** What accepting changes in the project: its branch, commits and files. */
const changedKeys = (task: Task) => [
  tasksKey(task.project),
  ["commits", task.project],
  ["status", task.project],
  folderKey(task.project),
  shipKey(task.id),
  ACTIVITY_KEY,
];

/**
 * The review before "Aceptar": target branch, files and lines, the commit
 * message (editable, or suggested by the task's AI) and the last test run.
 * Failed or missing tests are a warning, never a block: the user decides.
 * With a remote, the work can also stay on the task's branch for a PR; the
 * publish flow (push, PR) follows from the accepted view.
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
  const toast = useToast();
  const preview = useQuery({ queryKey: ["acceptPreview", task.id], queryFn: () => api.acceptPreview(task.id), gcTime: 0 });
  const folder = useFolderStatus(task.project);
  const remote = !!folder.data?.remote;
  // How the user accepts in this project, as last time ("Guardar en su rama" + PR: two clicks).
  const modeKey = `hedy.acceptMode.${task.project}`;
  const [mode, setModeState] = useState<AcceptMode>(() => {
    try {
      return localStorage.getItem(modeKey) === "branch" ? "branch" : "merge";
    } catch {
      return "merge";
    }
  });
  const setMode = (next: AcceptMode) => {
    setModeState(next);
    try {
      localStorage.setItem(modeKey, next);
    } catch {
      // Remembering the choice is only a convenience.
    }
  };
  // "Publicar y abrir un PR": publishing is chosen, never preset; once chosen
  // in this project, it stays chosen (accepting with a PR is then two clicks).
  const prKey = `hedy.acceptPr.${task.project}`;
  const [pr, setPrState] = useState<PullRequestChoice>(() => {
    let on = false;
    try {
      on = localStorage.getItem(prKey) === "on";
    } catch {
      // Remembering the choice is only a convenience.
    }
    return { on, title: "", body: "", draft: false };
  });
  const setPr = (next: PullRequestChoice) => {
    setPrState(next);
    try {
      localStorage.setItem(prKey, next.on ? "on" : "off");
    } catch {
      // Remembering the choice is only a convenience.
    }
  };
  const setQueue = useSetQueue();
  const fixTests = useAction((a) => a.queueMessage(task.id, t("end.fixTestsPrompt")), {
    invalidate: [tasksKey(task.project)],
    onSuccess: (left) => {
      setQueue(task.id, left);
      onClose();
    },
  });
  const accept = useAction(
    async (a, vars: { message: string; mode: AcceptMode; pr: PullRequestChoice | null }) => {
      await a.acceptTask(task.id, vars.message, vars.mode);
      if (!vars.pr) return null;
      // Publish and open the pull request in one go; a failure leaves the branch
      // committed, and the side panel offers each step again.
      try {
        // A rejected push is an answer, not an error: no PR from an unpublished branch.
        const pushed = await a.pushTask(task.id);
        if (pushed.failure) throw new Error(pushed.failure.message);
        return await a.createPullRequest(task.id, { title: vars.pr.title, body: vars.pr.body, base: "", draft: vars.pr.draft });
      } catch (error) {
        toast.error(errorMessage(error, t));
        return null;
      }
    },
    {
      invalidate: changedKeys(task),
      onError: () => {
        accepting.current = false;
      },
      onSuccess: (opened, vars) => {
        // With a remote, the accepted view opens the publish step next (unless it is done).
        if (remote && !vars.pr) markJustAccepted(task.id);
        onClose();
        const branch = preview.data?.branch ?? task.workspace?.branch ?? "";
        if (opened) {
          toast.info(t("ship.prOpened", { branch }), { action: { label: t("ship.prOpen"), onClick: () => void api.openLink(opened.url) } });
        } else if (vars.mode === "branch") {
          toast.info(t("ship.committed", { branch }));
        } else {
          onAccepted(preview.data?.targetBranch ?? task.workspace?.baseBranch ?? "");
        }
      },
    },
  );
  // Set on the first submit; a failure lets the user try again.
  const accepting = useRef(false);
  const branchMode = remote && mode === "branch";
  // Only with GitHub CLI logged in: otherwise the side panel says what is missing.
  const canPr = branchMode && folder.data?.gh === "ready";
  const withPr = canPr && pr.on;

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
            disabled={!preview.data || !!preview.data.branchOff || accept.isPending}
            title={t("ship.confirmHint", { keys: `${modKey()}+Enter` })}
          >
            {accept.isPending && withPr ? t("ship.prOpening") : withPr ? t("ship.confirmPr") : branchMode ? t("ship.confirmBranch") : t("review.confirm")}
          </Button>
        </>
      }
    >
      {preview.isPending && <p className="muted">{t("review.loading")}</p>}
      {preview.isError && (
        <EmptyState
          error
          title={t("review.loadError")}
          text={errorMessage(preview.error, t)}
          onRetry={() => void preview.refetch()}
        />
      )}
      {preview.data && (
        <PreviewForm
          key={preview.dataUpdatedAt}
          task={task}
          preview={preview.data}
          mode={remote ? mode : "merge"}
          onMode={remote ? setMode : null}
          pr={canPr ? pr : null}
          onPr={setPr}
          onFixTests={fixTests.isPending ? null : () => fixTests.mutate(undefined)}
          onSubmit={(message) => {
            // Ctrl/⌘ Enter works with the button disabled: never twice, even in
            // one tick, and never with the copy on another branch.
            if (accepting.current || preview.data?.branchOff) return;
            accepting.current = true;
            accept.mutate({ message, mode: remote ? mode : "merge", pr: withPr ? pr : null });
          }}
        />
      )}
    </Dialog>
  );
}

/** "Publicar y abrir un PR" in the sheet: empty title and description are written by Hedy. */
interface PullRequestChoice {
  on: boolean;
  title: string;
  body: string;
  draft: boolean;
}

function PreviewForm({
  task,
  preview,
  mode,
  onMode,
  pr,
  onPr,
  onFixTests,
  onSubmit,
}: {
  task: Task;
  preview: AcceptPreview;
  mode: AcceptMode;
  /** `null` without a remote: only merging makes sense. */
  onMode: ((mode: AcceptMode) => void) | null;
  /** `null` unless the work stays on its branch with a remote. */
  pr: PullRequestChoice | null;
  onPr: (pr: PullRequestChoice) => void;
  /** Sends the AI back to the failing tests (`null` while it is being sent). */
  onFixTests: (() => void) | null;
  onSubmit: (message: string) => void;
}) {
  const t = useT();
  const [message, setMessage] = useState(preview.message);
  const elsewhere = mode === "merge" && preview.checkoutBranch !== null && preview.checkoutBranch !== preview.targetBranch;
  const tests = preview.tests;
  // What Hedy checked after the AI's last turn says more than the AI's own tests.
  const checks = preview.checks;
  const reviewed = preview.review;
  const passed =
    checks.length > 0 || reviewed
      ? checks.every((c) => c.passed) && (reviewed?.length ?? 0) === 0
      : !preview.checksSkipped && !!tests?.passed;

  return (
    <form
      id="accept-sheet"
      className="accept-sheet"
      onSubmit={(e) => {
        e.preventDefault();
        onSubmit(message);
      }}
    >
      {onMode ? (
        <AcceptModeChoice mode={mode} onChange={onMode} target={preview.targetBranch} branch={preview.branch} />
      ) : (
        <p>{t("review.into", { target: preview.targetBranch, branch: preview.branch })}</p>
      )}
      {onMode && mode === "merge" && (
        <p className="muted small">{t("review.into", { target: preview.targetBranch, branch: preview.branch })}</p>
      )}
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
      {preview.branchOff && (
        <p className="accept-sheet__warn" role="alert">
          {t("review.branchOff", { branch: preview.branch, on: preview.branchOff })}
        </p>
      )}
      {elsewhere && (
        <p className="accept-sheet__warn" role="alert">
          {t("review.checkout", { current: preview.checkoutBranch ?? "", target: preview.targetBranch })}
        </p>
      )}
      <p className={`accept-sheet__tests ${passed ? "accept-sheet__tests--ok" : "accept-sheet__warn"}`}>
        <span aria-hidden>{passed ? "✓" : "!"}</span>{" "}
        {checks.length > 0 || reviewed
          ? t(passed ? "review.checks.passed" : "review.checks.failed", {
              list: [checkList(checks), reviewed && reviewList(reviewed, t)].filter(Boolean).join(" · "),
            })
          : preview.checksSkipped
            ? t("review.checks.skipped", { reason: preview.checksSkipped })
            : tests
              ? tests.passed
                ? t("review.tests.passed", { command: tests.command })
                : t("review.tests.failed", { command: tests.command })
              : t("review.tests.none")}
        {!passed && (checks.length > 0 || !!reviewed?.length || (tests && !preview.checksSkipped)) && (
          <Button small variant="ghost" className="accept-sheet__fix" disabled={!onFixTests} onClick={() => onFixTests?.()}>
            {t("end.fixTests")}
          </Button>
        )}
      </p>
      <CommitMessage task={task} value={message} onChange={setMessage} hint={t("review.messageHint")} />
      {pr && (
        <fieldset className="accept-pr">
          <label className="accept-pr__switch">
            <Switch checked={pr.on} onChange={(on) => onPr({ ...pr, on })} label={t("ship.withPr")} />
            <span aria-hidden>{t("ship.withPr")}</span>
          </label>
          {pr.on && (
            <>
              <label className="field">
                <span className="field__label">{t("ship.pr.fieldTitle")}</span>
                <input
                  className="input"
                  value={pr.title}
                  placeholder={message.split("\n")[0]}
                  // Enter alone never publishes: that takes the button, or Ctrl/⌘ Enter.
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && !(e.ctrlKey || e.metaKey)) e.preventDefault();
                  }}
                  onChange={(e) => onPr({ ...pr, title: e.target.value })}
                />
              </label>
              <label className="field">
                <span className="field__label">{t("ship.pr.body")}</span>
                <textarea
                  className="input"
                  rows={3}
                  value={pr.body}
                  placeholder={t("ship.withPrBodyHint")}
                  onChange={(e) => onPr({ ...pr, body: e.target.value })}
                />
              </label>
              <label className="accept-pr__switch">
                <Switch checked={pr.draft} onChange={(draft) => onPr({ ...pr, draft })} label={t("ship.pr.draft")} />
                <span aria-hidden>{t("ship.pr.draft")}</span>
              </label>
            </>
          )}
        </fieldset>
      )}
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

/** "tests ✓ · e2e ✗ (npm run e2e)": a failing check names its command. */
function checkList(checks: CheckResult[]): string {
  return checks.map((c) => (c.passed ? `${c.check} ✓` : `${c.check} ✗ (${c.command})`)).join(" · ");
}

/** "revisión ✓" or "revisión: 2 avisos". */
function reviewList(warnings: ReviewWarning[], t: Translate): string {
  return warnings.length ? t("review.checks.reviewWarnings", { n: String(warnings.length) }) : t("review.checks.reviewClean");
}
