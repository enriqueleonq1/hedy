import { useQuery } from "@tanstack/react-query";

import { useAction, useApi } from "../../api/context";
import type { Account, Task } from "../../api/types";
import { Mascot } from "../../components/Mascot/Mascot";
import { Button } from "../../components/ui/Button";
import { useT, type Translate } from "../../i18n";
import { formatCountdown, useNowSeconds } from "../../tasks/countdown";
import { ACTIVITY_KEY, listKeyFor } from "../../tasks/hooks";
import { isOpen, type TimelineItem } from "../../tasks/timeline";
import { WARNING_MASCOT_COLOR } from "../../theme/tokens";
import "../../styles/review.css";

function resetsIn(resetsAt: number | null, now: number, t: Translate): string | null {
  if (!resetsAt) return null;
  const minutes = Math.round((resetsAt - now) / 60);
  if (minutes <= 0) return t("workspace.limit.resetPassed");
  return minutes < 90
    ? t("workspace.limit.resetsMinutes", { n: minutes })
    : t("workspace.limit.resetsHours", { n: Math.round(minutes / 60) });
}

/**
 * Hedy never switches accounts by itself (spec rule 3): the card offers to
 * wait, or to hand the task to another provider or an API key. Waiting is
 * stored with the task: a countdown to the reset, then Hedy resumes the task
 * on the same account (or, when the reset passed while it was closed, the
 * user presses "Continuar").
 */
export function LimitCard({
  task,
  item,
  accounts,
  active,
}: {
  task: Task;
  item: Extract<TimelineItem, { type: "limit" }>;
  accounts: Account[];
  /** Nothing set the task going since this card: its options still apply. */
  active: boolean;
}) {
  const api = useApi();
  const t = useT();
  const now = useNowSeconds(1000);
  const actionable = item.latest && active && isOpen(task);
  const wait = actionable ? (task.wait ?? null) : null;
  const options = useQuery({
    queryKey: ["handoff", task.id, item.key],
    queryFn: () => api.handoffOptions(task.id),
    enabled: actionable,
  });
  const changed = [listKeyFor(task), ACTIVITY_KEY];
  const handoff = useAction((a, account: string) => a.handoffTask(task.id, account, null), { invalidate: changed });
  const startWait = useAction((a) => a.waitForLimit(task.id), { invalidate: changed });
  const cancelWait = useAction((a) => a.cancelWait(task.id), { invalidate: changed });
  const resume = useAction((a) => a.resumeAfterLimit(task.id), { invalidate: changed });
  const busy = startWait.isPending || cancelWait.isPending || resume.isPending || handoff.isPending;

  const account = accounts.find((a) => a.id === item.account);
  const name = account?.name ?? item.account;
  const title =
    item.level === "reached"
      ? t("workspace.limit.reached", { name })
      : item.percent != null
        ? t("workspace.limit.percent", { name, percent: Math.round(item.percent) })
        : t("workspace.limit.near", { name });
  const candidates = (options.data ?? [])
    .map((id) => accounts.find((a) => a.id === id))
    .filter((a): a is Account => !!a);
  const waitName = accounts.find((a) => a.id === wait?.account)?.name ?? wait?.account ?? name;
  const left = wait?.until ? formatCountdown(wait.until - now, t) : null;
  // The reset passed while Hedy was closed, or its time is unknown: the user resumes.
  const canContinue = !!wait && (wait.ready || wait.until === null);

  return (
    <section className="limit-card" aria-label={title}>
      <Mascot color={account?.color ?? WARNING_MASCOT_COLOR} state="sleeping" size={36} label="" />
      <div className="limit-card__body">
        <h3>{title}</h3>
        {!wait && (
          <p>
            {resetsIn(item.resetsAt, now, t) ?? ""} {candidates.length > 0 && t("workspace.limit.canHandOff")}
          </p>
        )}
        {wait && (
          <div className="limit-wait">
            {wait.ready ? (
              <p role="status">{t("wait.ready", { account: waitName })}</p>
            ) : wait.until === null ? (
              <p>{t("wait.unknown", { account: waitName })}</p>
            ) : left ? (
              // Ticks every second: kept out of the timeline's live region.
              <p aria-live="off">
                <span className="sr-only">{t("wait.countingLabel")}: </span>
                <span className="limit-wait__time">{t("wait.counting", { time: left, account: waitName })}</span>
              </p>
            ) : (
              <p role="status">{t("wait.resuming", { account: waitName })}</p>
            )}
            <p className="small">{t("wait.sameAccount")}</p>
          </div>
        )}
        {actionable && (
          <div className="actions">
            {!wait && (
              <Button variant="primary" disabled={busy} onClick={() => startWait.mutate(undefined)}>
                {t("workspace.limit.wait")}
              </Button>
            )}
            {canContinue && (
              <Button variant="primary" disabled={busy} onClick={() => resume.mutate(undefined)}>
                {t("wait.continue")}
              </Button>
            )}
            {wait && (
              <Button variant="ghost" disabled={busy} onClick={() => cancelWait.mutate(undefined)}>
                {t("wait.cancel")}
              </Button>
            )}
            {candidates.map((c) => (
              <Button key={c.id} disabled={busy} onClick={() => handoff.mutate(c.id)}>
                {t("workspace.limit.handOff", { name: c.name })}
              </Button>
            ))}
          </div>
        )}
        {actionable && options.isSuccess && candidates.length === 0 && (
          <p className="small">{t("workspace.limit.noOther")}</p>
        )}
      </div>
    </section>
  );
}
