import { useEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";

import { useAction, useApi } from "../../api/context";
import type { Account, Decision, Task } from "../../api/types";
import { Mascot, type MascotState } from "../../components/Mascot/Mascot";
import { Button } from "../../components/ui/Button";
import { formatDuration } from "../../format";
import { tasksKey } from "../../tasks/hooks";
import { isOpen, PHASE_VERB, type Resolution, type TimelineItem } from "../../tasks/timeline";
import { useT, type MessageKey, type Translate } from "../../i18n";
import { DEFAULT_ACCOUNT_COLOR, WARNING_MASCOT_COLOR } from "../../theme/tokens";

interface TimelineProps {
  task: Task;
  items: TimelineItem[];
  agent: string;
  account: Account | undefined;
  accounts: Account[];
  mascot: MascotState;
}

export function Timeline({ task, items, agent, account, accounts, mascot }: TimelineProps) {
  const t = useT();
  const end = useRef<HTMLDivElement>(null);
  useEffect(() => {
    end.current?.scrollIntoView?.({ block: "end" });
  }, [items.length]);

  const color = account?.color ?? DEFAULT_ACCOUNT_COLOR;
  const lastAgent = items.findLastIndex((i) => i.type !== "user");

  return (
    <div className="timeline" aria-live="polite">
      {items.map((item, index) => {
        if (item.type === "user") {
          return (
            <div key={item.key} className="bubble">
              {item.text}
            </div>
          );
        }
        if (item.type === "phase") {
          return (
            <p key={item.key} className="phase-divider">
              <span>
                <strong>{item.agent}</strong> {t(PHASE_VERB[item.phase])} · {item.account}
              </span>
            </p>
          );
        }
        if (item.type === "limit") {
          return <LimitCard key={item.key} task={task} item={item} accounts={accounts} />;
        }
        return (
          <div key={item.key} className="agent-row">
            <span className="agent-row__avatar">
              {(index === 0 || items[index - 1]?.type === "user") && (
                <Mascot color={color} state={index === lastAgent ? mascot : "idle"} size={30} label={agent} />
              )}
            </span>
            <div className="agent-row__body">
              <Item task={task} item={item} />
            </div>
          </div>
        );
      })}
      <div ref={end} />
    </div>
  );
}

function Item({ task, item }: { task: Task; item: TimelineItem }) {
  const t = useT();
  switch (item.type) {
    case "agent":
      return <p className="agent-text">{item.text}</p>;
    case "plan":
      return <PlanCard task={task} item={item} agent={item.agent} />;
    case "steps":
      return (
        <section className="card plan-card" aria-label={t("workspace.steps")}>
          <h3>{t("workspace.steps")}</h3>
          <ul className="steps">
            {item.steps.map((s, i) => (
              <li key={i} className={`step step--${s.status}`}>
                <span className="step__mark" aria-hidden>
                  {s.status === "done" ? "✓" : s.status === "inProgress" ? "●" : "○"}
                </span>
                <span>{s.text}</span>
                <span className="sr-only">
                  {t(
                    s.status === "done"
                      ? "workspace.step.done"
                      : s.status === "inProgress"
                        ? "workspace.step.inProgress"
                        : "workspace.step.pending",
                  )}
                </span>
              </li>
            ))}
          </ul>
        </section>
      );
    case "permission":
      return <PermissionCard task={task} item={item} />;
    case "activity":
      return (
        <p className={`activity activity--${item.tone}`}>
          <span className="activity__mark" aria-hidden>
            {item.tone === "done" ? "✓" : item.tone === "failed" ? "✗" : "●"}
          </span>
          {item.text}
        </p>
      );
    case "finished":
      return (
        <p className={`activity activity--${item.ok ? "done" : "failed"}`}>
          <span className="activity__mark" aria-hidden>
            {item.ok ? "✓" : "✗"}
          </span>
          {item.ok ? t("workspace.finished") : t("workspace.finishedWithError")}
          {item.durationMs !== null && ` · ${formatDuration(item.durationMs)}`}
        </p>
      );
    case "error":
      return (
        <p className="activity activity--failed" role="alert">
          <span className="activity__mark" aria-hidden>
            ✗
          </span>
          {item.message}
        </p>
      );
    default:
      return null;
  }
}

const RESOLUTION: Record<Decision, MessageKey> = {
  allow: "workspace.resolution.allow",
  allowForTask: "workspace.resolution.allowForTask",
  deny: "workspace.resolution.deny",
};

function resolutionText(r: Resolution, t: Translate): string {
  if (r.automatic) {
    return r.reason ?? (r.decision === "deny" ? t("workspace.resolution.hedyDenied") : t("workspace.resolution.rule"));
  }
  return t(RESOLUTION[r.decision]);
}

const TOOL_TITLE: Record<string, MessageKey> = {
  Bash: "workspace.permission.Bash",
  Edit: "workspace.permission.Edit",
  MultiEdit: "workspace.permission.Edit",
  Write: "workspace.permission.Write",
  NotebookEdit: "workspace.permission.NotebookEdit",
  WebFetch: "workspace.permission.WebFetch",
  WebSearch: "workspace.permission.WebSearch",
};

function PermissionCard({ task, item }: { task: Task; item: Extract<TimelineItem, { type: "permission" }> }) {
  const t = useT();
  const answer = useAction((api, decision: Decision) => api.answerPermission(task.id, item.requestId, decision));
  const name = item.agent.split(" ")[0] ?? item.agent;
  const titleKey = TOOL_TITLE[item.tool];
  const title = titleKey
    ? t(titleKey, { agent: name })
    : t("workspace.permission.other", { agent: name, tool: item.tool });

  if (item.resolution) {
    const denied = item.resolution.decision === "deny";
    return (
      <p className={`activity activity--${denied ? "failed" : "done"}`}>
        <span className="activity__mark" aria-hidden>
          {denied ? "✗" : "✓"}
        </span>
        {resolutionText(item.resolution, t)} · <code>{item.summary}</code>
      </p>
    );
  }
  const waiting = task.live;
  return (
    <section className="permission-card" aria-label={title}>
      <h3>{title}</h3>
      <code className="permission-card__what">{item.summary}</code>
      <div className="actions">
        <Button variant="primary" disabled={!waiting || answer.isPending} onClick={() => answer.mutate("allow")}>
          {t("workspace.permission.allow")}
        </Button>
        <Button className="btn--warn" disabled={!waiting || answer.isPending} onClick={() => answer.mutate("allowForTask")}>
          {t("workspace.permission.allowForTask")}
        </Button>
        <Button className="btn--warn" disabled={!waiting || answer.isPending} onClick={() => answer.mutate("deny")}>
          {t("workspace.permission.deny")}
        </Button>
      </div>
      {!waiting && <p className="small">{t("workspace.permission.stopped")}</p>}
    </section>
  );
}

function PlanCard({ task, item, agent }: { task: Task; item: Extract<TimelineItem, { type: "plan" }>; agent: string }) {
  const t = useT();
  const [changing, setChanging] = useState(false);
  const [feedback, setFeedback] = useState("");
  const answer = useAction((api, vars: { approve: boolean; feedback: string | null }) =>
    api.answerPlan(task.id, item.requestId ?? "", vars.approve, vars.feedback),
  );
  const pending = item.requestId !== null && item.resolution === null;

  return (
    <section className={`card plan-card${pending ? " plan-card--pending" : ""}`} aria-label={t("workspace.plan.of", { agent })}>
      <h3>
        Plan · {agent}
        {item.resolution && (
          <span className={`plan-card__state plan-card__state--${item.resolution.decision}`}>
            {item.resolution.decision === "deny" ? t("workspace.plan.changesRequested") : t("workspace.plan.approved")}
          </span>
        )}
      </h3>
      <div className="plan-card__text">{item.text}</div>
      {pending && !changing && (
        <div className="actions">
          <Button variant="primary" disabled={!task.live || answer.isPending} onClick={() => answer.mutate({ approve: true, feedback: null })}>
            {t("workspace.plan.approve")}
          </Button>
          <Button disabled={!task.live} onClick={() => setChanging(true)}>
            {t("workspace.plan.requestChanges")}
          </Button>
        </div>
      )}
      {pending && changing && (
        <form
          className="plan-card__feedback"
          onSubmit={(e) => {
            e.preventDefault();
            answer.mutate({ approve: false, feedback });
          }}
        >
          <label className="field">
            <span className="field__label">{t("workspace.plan.whatToChange")}</span>
            <textarea value={feedback} onChange={(e) => setFeedback(e.target.value)} rows={2} />
          </label>
          <div className="actions">
            <Button type="submit" variant="primary" disabled={!feedback.trim() || answer.isPending}>
              {t("workspace.plan.sendChanges")}
            </Button>
            <Button variant="ghost" onClick={() => setChanging(false)}>
              {t("common.cancel")}
            </Button>
          </div>
        </form>
      )}
    </section>
  );
}

function resetsIn(resetsAt: number | null, t: Translate): string | null {
  if (!resetsAt) return null;
  const minutes = Math.round((resetsAt * 1000 - Date.now()) / 60_000);
  if (minutes <= 0) return t("workspace.limit.resetPassed");
  return minutes < 90
    ? t("workspace.limit.resetsMinutes", { n: minutes })
    : t("workspace.limit.resetsHours", { n: Math.round(minutes / 60) });
}

/**
 * Hedy never switches accounts by itself (spec rule 3): the card offers to
 * wait, or to hand the task to another provider or an API key.
 */
function LimitCard({
  task,
  item,
  accounts,
}: {
  task: Task;
  item: Extract<TimelineItem, { type: "limit" }>;
  accounts: Account[];
}) {
  const api = useApi();
  const t = useT();
  const [waiting, setWaiting] = useState(false);
  const actionable = item.latest && isOpen(task) && !waiting;
  const options = useQuery({
    queryKey: ["handoff", task.id, item.key],
    queryFn: () => api.handoffOptions(task.id),
    enabled: actionable,
  });
  const handoff = useAction((a, account: string) => a.handoffTask(task.id, account, null), {
    invalidate: [tasksKey(task.project)],
  });
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

  return (
    <section className="limit-card" aria-label={title}>
      <Mascot color={account?.color ?? WARNING_MASCOT_COLOR} state="sleeping" size={36} label="" />
      <div className="limit-card__body">
        <h3>{title}</h3>
        <p>
          {resetsIn(item.resetsAt, t) ?? ""} {candidates.length > 0 && t("workspace.limit.canHandOff")}
        </p>
        {actionable && (
          <div className="actions">
            <Button variant="primary" onClick={() => setWaiting(true)}>
              {t("workspace.limit.wait")}
            </Button>
            {candidates.map((c) => (
              <Button key={c.id} className="btn--warn" disabled={handoff.isPending} onClick={() => handoff.mutate(c.id)}>
                {t("workspace.limit.handOff", { name: c.name })}
              </Button>
            ))}
          </div>
        )}
        {actionable && options.isSuccess && candidates.length === 0 && (
          <p className="small">{t("workspace.limit.noOther")}</p>
        )}
        {waiting && <p className="small">{t("workspace.limit.waiting")}</p>}
      </div>
    </section>
  );
}
