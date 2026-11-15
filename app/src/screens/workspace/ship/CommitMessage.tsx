import { useId, type KeyboardEvent } from "react";
import { useMutation } from "@tanstack/react-query";

import { errorMessage, useApi, useOverview } from "../../../api/context";
import type { AcceptMode, Task } from "../../../api/types";
import { Segmented } from "../../../components/ui/controls";
import { useT } from "../../../i18n";
import { SparkleIcon } from "./icons";
import "../../../styles/ship.css";

/** The suggestion replaces the subject line; a body the user kept stays. */
export function withSubject(message: string, subject: string): string {
  const [, ...rest] = message.split("\n");
  const body = rest.join("\n");
  return body.trim() ? `${subject}\n${body}` : subject;
}

/**
 * The commit message, with "Sugerir con IA": the task's own account and
 * model write a one-line Conventional Commit from the diff. It spends a few
 * tokens (said in the tooltip) and never uses another account.
 */
export function CommitMessage({
  task,
  value,
  onChange,
  hint,
}: {
  task: Task;
  value: string;
  onChange: (message: string) => void;
  hint: string;
}) {
  const api = useApi();
  const t = useT();
  const id = useId();
  const overview = useOverview();
  const account = overview.data?.accounts.find((a) => a.id === task.account)?.name ?? task.account;
  const suggest = useMutation({
    mutationFn: () => api.suggestCommitMessage(task.id),
    onSuccess: (subject) => onChange(withSubject(value, subject)),
  });

  // Ctrl/⌘ + Enter accepts from the message too.
  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      e.currentTarget.form?.requestSubmit();
    }
  };

  return (
    <div className="field">
      <div className="commit-message__head">
        <label className="field__label" htmlFor={id}>
          {t("review.message")}
        </label>
        <button
          type="button"
          className="tool-btn commit-message__suggest"
          title={t("ship.suggestHint", { account })}
          aria-describedby={`${id}-suggest`}
          disabled={suggest.isPending}
          aria-busy={suggest.isPending || undefined}
          onClick={() => suggest.mutate()}
        >
          <SparkleIcon />
          {suggest.isPending ? t("ship.suggesting") : t("ship.suggest")}
        </button>
        <span id={`${id}-suggest`} className="sr-only">
          {t("ship.suggestHint", { account })}
        </span>
      </div>
      <textarea
        id={id}
        className="mono accept-sheet__message"
        rows={4}
        value={value}
        aria-describedby={`${id}-hint`}
        onKeyDown={onKeyDown}
        onChange={(e) => onChange(e.target.value)}
      />
      {suggest.isError ? (
        <span id={`${id}-hint`} className="error-text small" role="alert">
          {errorMessage(suggest.error, t)}
        </span>
      ) : (
        <span id={`${id}-hint`} className="muted small">
          {hint}
        </span>
      )}
    </div>
  );
}

/** Merge into the folder's branch, or keep the work on the task's branch for a PR. */
export function AcceptModeChoice({
  mode,
  onChange,
  target,
  branch,
}: {
  mode: AcceptMode;
  onChange: (mode: AcceptMode) => void;
  target: string;
  branch: string;
}) {
  const t = useT();
  return (
    <div className="accept-mode">
      <Segmented
        label={t("ship.mode.label")}
        value={mode}
        onChange={onChange}
        options={[
          { value: "merge", label: t("ship.mode.merge", { branch: target }) },
          { value: "branch", label: t("ship.mode.branch") },
        ]}
      />
      {mode === "branch" && <p className="muted small">{t("ship.mode.branchHint", { branch })}</p>}
    </div>
  );
}
