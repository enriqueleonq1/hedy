import type { AgentEvent, Overview, Phase, Task } from "../../api/types";
import { useApi } from "../../api/context";
import type { View } from "../../components/Layout";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { useToast } from "../../components/ui/Toast";
import { formatTokens, formatUsd } from "../../format";
import { useLanguage, useT, type MessageKey } from "../../i18n";
import { isOpen, modelLabel, MODE_LABEL } from "../../tasks/timeline";
import { OriginLink } from "./carry/links";

const PHASE: Record<Phase, MessageKey> = {
  plan: "activity.phase.plan",
  implement: "activity.phase.implement",
  tests: "activity.phase.tests",
  review: "activity.phase.review",
};

/** What the task spent, from its usage events: dollars (when known) and tokens. */
function spent(events: AgentEvent[]): { cost: number | null; tokens: number } {
  let cost: number | null = null;
  let tokens = 0;
  for (const e of events) {
    if (e.kind !== "usage") continue;
    tokens += e.inputTokens + e.outputTokens + e.cacheReadTokens + e.cacheWriteTokens;
    if (e.costUsd !== null) cost = (cost ?? 0) + e.costUsd;
  }
  return { cost, tokens };
}

/**
 * "ⓘ Detalles": what the header leaves out. Its copy and branch, where it
 * came from, who does each step and what it spent.
 */
export function TaskDetails({
  task,
  events,
  overview,
  onNavigate,
  onClose,
}: {
  task: Task;
  events: AgentEvent[];
  overview: Overview;
  onNavigate?: (view: View) => void;
  onClose: () => void;
}) {
  const t = useT();
  const language = useLanguage();
  const api = useApi();
  const toast = useToast();
  // Accepting or discarding deletes the copy; a branch kept for a PR stays.
  const open = isOpen(task);
  const copy = open ? task.workspace : null;
  const branch = open || task.ship?.kept ? task.workspace?.branch : undefined;
  const used = spent(events);
  const accountName = (id: string) => overview.accounts.find((a) => a.id === id)?.name ?? id;
  const copyBranch = (branch: string) =>
    void navigator.clipboard?.writeText(branch).then(
      () => toast.info(t("details.branchCopied", { branch })),
      () => toast.info(t("palette.copyFailed")),
    );

  return (
    <Dialog
      title={t("details.title")}
      onClose={onClose}
      footer={
        branch ? (
          <>
            <Button onClick={() => copyBranch(branch)}>{t("details.copyBranch")}</Button>
            {copy && <Button onClick={() => void api.openInEditor(copy.worktree)}>{t("details.openEditor")}</Button>}
          </>
        ) : undefined
      }
    >
      <dl className="details-list">
        <dt>{t("details.mode")}</dt>
        <dd>{t(MODE_LABEL[task.mode])}</dd>
        {branch && task.workspace && (
          <>
            <dt>{t("details.branch")}</dt>
            <dd className="mono">{t("details.branchFrom", { branch, base: task.workspace.baseBranch })}</dd>
          </>
        )}
        {copy ? (
          <>
            <dt>{t("details.copy")}</dt>
            <dd className="mono">{copy.worktree}</dd>
          </>
        ) : (
          open &&
          !task.conversation && (
            <>
              <dt>{t("details.copy")}</dt>
              <dd>{t("details.noCopy")}</dd>
            </>
          )
        )}
        {task.origin && onNavigate && (
          <>
            <dt>{t("details.origin")}</dt>
            <dd>
              <OriginLink origin={task.origin} onNavigate={onNavigate} />
            </dd>
          </>
        )}
        {task.stages.length > 0 && (
          <>
            <dt>{t("details.steps")}</dt>
            <dd>
              <ol className="details-list__steps">
                {task.stages.map((stage, i) => (
                  <li key={`${stage.phase}-${i}`} aria-current={i === task.stage ? "step" : undefined}>
                    {t(PHASE[stage.phase])}: {accountName(stage.account)}
                    {stage.model && ` · ${modelLabel(stage.model)}`}
                  </li>
                ))}
              </ol>
            </dd>
          </>
        )}
        {used.tokens > 0 && (
          <>
            <dt>{t("details.spent")}</dt>
            <dd>{t("details.spentValue", { cost: formatUsd(used.cost, language), tokens: formatTokens(used.tokens) })}</dd>
          </>
        )}
      </dl>
    </Dialog>
  );
}
