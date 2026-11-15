import { useEffect, useId, useState, type KeyboardEvent, type ReactNode } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { errorMessage, useApi } from "../../../api/context";
import type { PullRequest, PullRequestDraft, ShipFailure, ShipState, Task } from "../../../api/types";
import { useProjectActions } from "../../../components/projects";
import { modKey } from "../../../components/palette/keys";
import { Button } from "../../../components/ui/Button";
import { Switch } from "../../../components/ui/controls";
import { Dialog } from "../../../components/ui/Dialog";
import { useT } from "../../../i18n";
import { tasksKey } from "../../../tasks/hooks";
import { useFolderGit } from "./FolderStatus";
import { ExternalIcon } from "./icons";
import { gitKeys, nextStep, shipKey } from "./useShip";
import "../../../styles/ship.css";

export type ShipStep = "publish" | "pr" | "done";

/** Where the flow starts: what is left to do. */
export function firstStep(state: ShipState): ShipStep {
  if (!state.pushed && state.remote) return "publish";
  if (state.canOpenPr && !state.pullRequest) return "pr";
  return "done";
}

const STEP_TITLE = {
  publish: "ship.publish.title",
  pr: "ship.pr.title",
  done: "ship.done.title",
} as const;

/** Ctrl/⌘ + Enter confirms the step, from any field. */
function submitOnModEnter(e: KeyboardEvent<HTMLFormElement>) {
  if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
    e.preventDefault();
    e.currentTarget.requestSubmit();
  }
}

/** The line that says where the accepted work is. */
export function StateLine({ state }: { state: ShipState }) {
  const t = useT();
  return (
    <p className="ship-state">
      <span className="ship-state__mark" aria-hidden>
        ✓
      </span>{" "}
      {state.kept ? t("ship.state.kept", { branch: state.branch }) : t("ship.state.merged", { branch: state.branch })}
    </p>
  );
}

/**
 * Publishing an accepted task, one optional step at a time: push (to the
 * upstream, or `-u` when there is none), then a pull request through `gh`,
 * then its link. Each step can be skipped ("Ahora no").
 */
export function ShipDialog({ task, initial, onClose }: { task: Task; initial?: ShipStep; onClose: () => void }) {
  const api = useApi();
  const t = useT();
  const client = useQueryClient();
  const state = useQuery({ queryKey: shipKey(task.id), queryFn: () => api.shipState(task.id) });
  const [chosen, setChosen] = useState<ShipStep | null>(initial ?? null);
  const [pr, setPr] = useState<PullRequest | null>(null);
  const step: ShipStep = chosen ?? (state.data ? firstStep(state.data) : "publish");

  const refresh = async () => {
    await Promise.all(
      [shipKey(task.id), tasksKey(task.project), ...gitKeys(task.project)].map((queryKey) =>
        client.invalidateQueries({ queryKey }),
      ),
    );
    return client.fetchQuery({ queryKey: shipKey(task.id), queryFn: () => api.shipState(task.id) });
  };

  const formId = useId();
  const busyLabel = { publish: "ship.publish.going", pr: "ship.pr.creating", done: "ship.done.close" } as const;
  const goLabel = { publish: "ship.publish.go", pr: "ship.pr.create", done: "ship.done.close" } as const;
  const [busy, setBusy] = useState(false);
  const loaded = !!state.data;
  // The step's own action has the focus (Enter confirms); a PR starts in its title.
  useEffect(() => {
    if (!loaded) return;
    const selector = step === "pr" ? `form[id="${formId}"] input` : `button[form="${formId}"]`;
    document.querySelector<HTMLElement>(selector)?.focus();
  }, [step, loaded, formId]);

  return (
    <Dialog
      title={t(STEP_TITLE[step])}
      onClose={onClose}
      footer={
        <>
          {step !== "done" && (
            <Button variant="ghost" onClick={onClose}>
              {t("ship.later")}
            </Button>
          )}
          <Button
            variant="primary"
            type="submit"
            form={formId}
            disabled={busy || (!state.data && step !== "done")}
            title={t("ship.confirmHint", { keys: `${modKey()}+Enter` })}
          >
            {busy ? t(busyLabel[step]) : t(goLabel[step])}
          </Button>
        </>
      }
    >
      {state.isError && (
        <p className="error-text" role="alert">
          {errorMessage(state.error, t)}
        </p>
      )}
      {state.data && step === "publish" && (
        <PublishStep
          formId={formId}
          task={task}
          state={state.data}
          onBusy={setBusy}
          onDone={async () => {
            const next = await refresh();
            setChosen(next.canOpenPr && !next.pullRequest ? "pr" : "done");
          }}
        />
      )}
      {state.data && step === "pr" && (
        <PullRequestStep
          formId={formId}
          task={task}
          state={state.data}
          onBusy={setBusy}
          onDone={async (created) => {
            setPr(created);
            await refresh();
            setChosen("done");
          }}
        />
      )}
      {step === "done" && (
        <DoneStep formId={formId} state={state.data ?? null} pr={pr ?? state.data?.pullRequest ?? null} onClose={onClose} />
      )}
    </Dialog>
  );
}

function Failure({ failure, task, kept, onPulled }: { failure: ShipFailure; task: Task; kept: boolean; onPulled: () => void }) {
  const t = useT();
  const actions = useProjectActions();
  const git = useFolderGit(task.project);
  const step = nextStep(failure);
  return (
    <div className="ship-failure" role="alert">
      <p>{failure.message}</p>
      <div className="ship-failure__actions">
        {/* Only the folder's own branch can be pulled from here. */}
        {step === "pull" && !kept && (
          <Button small disabled={git.busy} onClick={() => git.pull.mutate(undefined, { onSuccess: onPulled })}>
            {t("ship.folder.pull")}
          </Button>
        )}
        {step !== null && (
          <Button small onClick={() => actions.openEditor({ path: task.project })}>
            {t("common.openInVsCode")}
          </Button>
        )}
      </div>
    </div>
  );
}

function PublishStep({
  formId,
  task,
  state,
  onBusy,
  onDone,
}: {
  formId: string;
  task: Task;
  state: ShipState;
  onBusy: (busy: boolean) => void;
  onDone: () => Promise<void>;
}) {
  const api = useApi();
  const t = useT();
  const [failure, setFailure] = useState<ShipFailure | null>(null);
  const push = useMutation({
    mutationFn: () => api.pushTask(task.id),
    onMutate: () => {
      setFailure(null);
      onBusy(true);
    },
    onSettled: () => onBusy(false),
    onSuccess: async (out) => {
      if (out.failure) setFailure(out.failure);
      else await onDone();
    },
    onError: (e) => setFailure({ kind: "other", message: errorMessage(e, t) }),
  });

  return (
    <form
      id={formId}
      className="ship-step"
      onKeyDown={submitOnModEnter}
      onSubmit={(e) => {
        e.preventDefault();
        if (!push.isPending) push.mutate();
      }}
    >
      <StateLine state={state} />
      <p>
        {state.upstream
          ? t("ship.publish.to", { branch: state.branch, upstream: state.upstream })
          : t("ship.publish.newUpstream", { branch: state.branch, remote: state.remote ?? "origin" })}
      </p>
      {!state.kept && <p className="muted small">{t("ship.publish.noUndo")}</p>}
      <p className="muted small">{t("ship.publish.creds")}</p>
      {failure && <Failure failure={failure} task={task} kept={state.kept} onPulled={() => setFailure(null)} />}
    </form>
  );
}

function PullRequestStep({
  formId,
  task,
  state,
  onBusy,
  onDone,
}: {
  formId: string;
  task: Task;
  state: ShipState;
  onBusy: (busy: boolean) => void;
  onDone: (pr: PullRequest) => Promise<void>;
}) {
  const api = useApi();
  const t = useT();
  const draft = useQuery({ queryKey: ["prDraft", task.id], queryFn: () => api.pullRequestDraft(task.id), gcTime: 0 });
  if (draft.isError) {
    return (
      <p className="error-text" role="alert">
        {errorMessage(draft.error, t)}
      </p>
    );
  }
  if (!draft.data) return <p className="muted">{t("review.loading")}</p>;
  return <PullRequestForm key={draft.dataUpdatedAt} formId={formId} task={task} state={state} draft={draft.data} onBusy={onBusy} onDone={onDone} />;
}

function PullRequestForm({
  formId,
  task,
  state,
  draft,
  onBusy,
  onDone,
}: {
  formId: string;
  task: Task;
  state: ShipState;
  draft: PullRequestDraft;
  onBusy: (busy: boolean) => void;
  onDone: (pr: PullRequest) => Promise<void>;
}) {
  const api = useApi();
  const t = useT();
  const id = useId();
  const [title, setTitle] = useState(draft.title);
  const [base, setBase] = useState(draft.base);
  const [body, setBody] = useState(draft.body);
  const [asDraft, setAsDraft] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const create = useMutation({
    mutationFn: () => api.createPullRequest(task.id, { title, body, base, draft: asDraft }),
    onMutate: () => {
      setError(null);
      onBusy(true);
    },
    onSettled: () => onBusy(false),
    onSuccess: (pr) => onDone(pr),
    onError: (e) => setError(errorMessage(e, t)),
  });

  return (
    <form
      id={formId}
      className="ship-step"
      onKeyDown={submitOnModEnter}
      onSubmit={(e) => {
        e.preventDefault();
        if (!create.isPending) create.mutate();
      }}
    >
      <StateLine state={state} />
      <div className="field">
        <label className="field__label" htmlFor={`${id}-title`}>
          {t("ship.pr.fieldTitle")}
        </label>
        <input id={`${id}-title`} value={title} onChange={(e) => setTitle(e.target.value)} />
      </div>
      <div className="ship-pr__branches">
        <div className="field">
          <label className="field__label" htmlFor={`${id}-base`}>
            {t("ship.pr.base")}
          </label>
          <input id={`${id}-base`} className="mono" value={base} onChange={(e) => setBase(e.target.value)} />
        </div>
        <p className="muted small ship-pr__head">{t("ship.pr.from", { head: draft.head })}</p>
      </div>
      <div className="field">
        <label className="field__label" htmlFor={`${id}-body`}>
          {t("ship.pr.body")}
        </label>
        <textarea
          id={`${id}-body`}
          className="mono ship-pr__body"
          rows={9}
          value={body}
          aria-describedby={`${id}-hint`}
          onChange={(e) => setBody(e.target.value)}
        />
        <span id={`${id}-hint`} className="muted small">
          {t("ship.pr.bodyHint")}
        </span>
      </div>
      <label className="ship-pr__draft">
        <Switch checked={asDraft} onChange={setAsDraft} label={t("ship.pr.draft")} />
        <span aria-hidden>{t("ship.pr.draft")}</span>
      </label>
      {error && (
        <p className="error-text" role="alert">
          {error}
        </p>
      )}
    </form>
  );
}

function DoneStep({
  formId,
  state,
  pr,
  onClose,
}: {
  formId: string;
  state: ShipState | null;
  pr: PullRequest | null;
  onClose: () => void;
}) {
  const api = useApi();
  const t = useT();
  let link: ReactNode = null;
  if (pr) {
    link = (
      <Button onClick={() => void api.openLink(pr.url)}>
        {pr.number !== null ? t("ship.done.open", { n: pr.number }) : t("ship.done.openAny")} <ExternalIcon />
      </Button>
    );
  }
  return (
    <form
      id={formId}
      className="ship-step"
      onSubmit={(e) => {
        e.preventDefault();
        onClose();
      }}
    >
      {state && <StateLine state={state} />}
      {state?.pushed && state.upstream && <p>{t("ship.done.pushed", { upstream: state.upstream })}</p>}
      {link && <div>{link}</div>}
    </form>
  );
}
