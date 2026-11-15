import { createContext, Fragment, useContext, useEffect, useId, useMemo, useRef, useState, type KeyboardEvent, type ReactNode, type RefObject } from "react";

import { useAction } from "../../api/context";
import type { Account, Decision, Task } from "../../api/types";
import { Mascot } from "../../components/Mascot/Mascot";
import { WorkingWord } from "../../components/Mascot/WorkingWord";
import { Button } from "../../components/ui/Button";
import { ChevronIcon } from "../../components/ui/icons";
import { relativeToCopy, taskHasCopy } from "../../code/copies";
import { useFileNavigator } from "../../files/navigator";
import { formatDuration } from "../../format";
import { Markdown } from "../../components/Markdown";
import { keyLabels } from "../../platform";
import { PHASE_VERB, type Resolution, type TimelineItem } from "../../tasks/timeline";
import type { TaskMascot } from "../../tasks/useMascot";
import { useT, type MessageKey, type Translate } from "../../i18n";
import { DEFAULT_ACCOUNT_COLOR } from "../../theme/tokens";
import { LimitCard } from "./LimitCard";
import { MessageImages } from "./MessageImages";
import { groupSteps, summarize, summaryFacts, type StepItem } from "./stepGroups";

interface TimelineProps {
  task: Task;
  items: TimelineItem[];
  agent: string;
  account: Account | undefined;
  accounts: Account[];
  /** The latest agent row shows it; while it works, a row of its own at the end. */
  mascot: TaskMascot;
  /** Shown after the history (the queued messages). */
  footer?: ReactNode;
  /** Scrolls to the end when it changes, besides new items. */
  scrollKey?: number;
}

export function Timeline({ task, items, agent, account, accounts, mascot, footer, scrollKey }: TimelineProps) {
  const t = useT();
  const end = useRef<HTMLDivElement>(null);
  useEffect(() => {
    end.current?.scrollIntoView?.({ block: "end" });
  }, [items.length, scrollKey]);

  const color = account?.color ?? DEFAULT_ACCOUNT_COLOR;
  const lastAgent = items.findLastIndex((i) => i.type !== "user");
  const working = mascot.state === "working";
  const entries = useMemo(() => groupSteps(items), [items]);

  /** An agent row spanning items `first..last`: the mascot opens each turn. */
  const agentRow = (key: string, first: number, last: number, body: ReactNode) => (
    <div key={key} className="agent-row">
      <span className="agent-row__avatar">
        {(first === 0 || items[first - 1]?.type === "user") && (
          <Mascot
            color={color}
            state={first <= lastAgent && lastAgent <= last && !working ? mascot.state : "idle"}
            size={30}
            label={agent}
          />
        )}
      </span>
      <div className="agent-row__body">{body}</div>
    </div>
  );

  // "editó facturas/impuestos.py" opens the file in the task's copy.
  const nav = useFileNavigator();
  const openFile = useMemo(() => {
    if (!taskHasCopy(task)) return null;
    return (reported: string) => {
      const path = relativeToCopy(reported, [task.workspace?.worktree, task.project]);
      if (path) nav.open({ copy: { kind: "task", task: task.id }, path });
    };
  }, [task, nav]);

  return (
    // Not a live region: the workspace announces only what needs the user or ends the task.
    <OpenFileContext.Provider value={openFile}>
    <div className="timeline">
      {entries.map((entry) => {
        if (entry.type === "group") {
          // The run still going on shows its latest step without opening it.
          const live = working && entry.last === items.length - 1;
          return agentRow(entry.key, entry.first, entry.last, <StepGroup steps={entry.steps} live={live} />);
        }
        const { item, index } = entry;
        if (item.type === "user") {
          return (
            <div key={item.key} className="bubble">
              <MessageImages taskId={task.id} images={item.images} />
              {item.text}
            </div>
          );
        }
        if (item.type === "phase") {
          return (
            <p key={item.key} className="phase-divider">
              <span>
                <strong>{item.agent}</strong>{" "}
                {t(task.mode === "project" ? PHASE_VERB[item.phase] : "workspace.phase.answer")} · {item.account}
              </span>
            </p>
          );
        }
        if (item.type === "limit") {
          // A message or a new phase after the card set the task going again.
          const active = !items.slice(index + 1).some((i) => i.type === "user" || i.type === "phase");
          return <LimitCard key={item.key} task={task} item={item} accounts={accounts} active={active} />;
        }
        return agentRow(item.key, index, index, <Item task={task} item={item} />);
      })}
      {working && (
        // What the agent is doing now; decorative: the activity lines say it too.
        <div className="agent-row agent-row--live" aria-hidden>
          <span className="agent-row__avatar">
            <Mascot color={color} state="working" activity={mascot.activity} size={44} label={agent} />
          </span>
          <WorkingWord activity={mascot.activity} className="working-word--chat" />
        </div>
      )}
      {footer}
      <div ref={end} />
    </div>
    </OpenFileContext.Provider>
  );
}

function Item({ task, item }: { task: Task; item: TimelineItem }) {
  const t = useT();
  switch (item.type) {
    case "agent":
      return <Markdown className="agent-text" text={item.text} />;
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
      return <StepRow step={item} />;
    case "finished":
      // The quiet end of a turn, not a step of it.
      return (
        <p className={`turn-end${item.ok ? "" : " turn-end--failed"}`}>
          {!item.ok && (
            <span className="turn-end__mark" aria-hidden>
              ✗
            </span>
          )}
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

/** Opens a file an agent edited in the viewer; `null` when the task has no files of its own. */
const OpenFileContext = createContext<((path: string) => void) | null>(null);

/** One tool step as a light line: a mark and what it did (an edited file links to it). */
function StepRow({ step, live = false }: { step: StepItem; live?: boolean }) {
  const t = useT();
  const openFile = useContext(OpenFileContext);
  const at = step.step === "edit" && openFile && step.detail ? step.text.lastIndexOf(step.detail) : -1;
  return (
    <p className={`step-row step-row--${step.tone}${live ? " step-row--live" : ""}`}>
      <span className="step-row__mark" aria-hidden>
        {step.tone === "done" ? "✓" : step.tone === "failed" ? "✗" : "●"}
      </span>
      {at >= 0 && openFile ? (
        <span>
          {step.text.slice(0, at)}
          <button
            type="button"
            className="step-row__file"
            title={t("code.timeline.open", { path: step.detail })}
            onClick={() => openFile(step.detail)}
          >
            {step.detail}
          </button>
          {step.text.slice(at + step.detail.length)}
        </span>
      ) : (
        <span>{step.text}</span>
      )}
    </p>
  );
}

/**
 * A run of tool steps: one line that sums it up (failures always in sight),
 * opened on demand into the steps themselves. While the run goes on it also
 * shows its latest step.
 */
function StepGroup({ steps, live }: { steps: StepItem[]; live: boolean }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const listId = useId();
  const latest = steps.at(-1);
  if (steps.length === 1 && latest) return <StepRow step={latest} live={live} />;

  const facts = summaryFacts(summarize(steps), t);
  return (
    <div className={`step-group${open ? " step-group--open" : ""}`}>
      <button
        type="button"
        className="step-group__summary"
        aria-expanded={open}
        aria-controls={open ? listId : undefined}
        onClick={() => setOpen((o) => !o)}
      >
        <span className="step-group__chevron">
          <ChevronIcon />
        </span>
        <span className="step-group__facts">
          {facts.map((fact, i) => (
            <Fragment key={i}>
              {i > 0 && " · "}
              {fact.failed ? (
                <span className="step-group__fact--failed">
                  <span className="step-group__mark" aria-hidden>
                    ✗
                  </span>
                  {fact.text}
                </span>
              ) : (
                fact.text
              )}
            </Fragment>
          ))}
        </span>
      </button>
      {open ? (
        <ul id={listId} className="step-group__list">
          {steps.map((step) => (
            <li key={step.key}>
              <StepRow step={step} live={live && step === latest} />
            </li>
          ))}
        </ul>
      ) : (
        live && latest && <StepRow step={latest} live />
      )}
    </div>
  );
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

/** Typing in a field: card shortcuts stay out of the way. */
function isTextField(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLElement &&
    (target.isContentEditable || target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.tagName === "SELECT")
  );
}

/**
 * A card that needs an answer takes the focus when it appears, unless the
 * user is somewhere else: only from the page itself or the chat, and never
 * from a field they are typing in.
 */
function useClaimFocus(ref: RefObject<HTMLElement | null>, active: boolean) {
  useEffect(() => {
    if (!active) return;
    const current = document.activeElement;
    const free = !current || current === document.body || (!!current.closest(".timeline") && !isTextField(current));
    if (free) ref.current?.focus();
  }, [ref, active]);
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
  const name = item.agent.split(" ")[0] ?? item.agent;
  const titleKey = TOOL_TITLE[item.tool];
  const title = titleKey
    ? t(titleKey, { agent: name })
    : t("workspace.permission.other", { agent: name, tool: item.tool });

  if (item.resolution) {
    const denied = item.resolution.decision === "deny";
    return (
      <p className={`step-row step-row--${denied ? "failed" : "done"}`}>
        <span className="step-row__mark" aria-hidden>
          {denied ? "✗" : "✓"}
        </span>
        <span>
          {resolutionText(item.resolution, t)} · <code>{item.summary}</code>
        </span>
      </p>
    );
  }
  return <PendingPermission task={task} item={item} title={title} />;
}

function PendingPermission({
  task,
  item,
  title,
}: {
  task: Task;
  item: Extract<TimelineItem, { type: "permission" }>;
  title: string;
}) {
  const t = useT();
  const card = useRef<HTMLElement>(null);
  const answer = useAction((api, decision: Decision) => api.answerPermission(task.id, item.requestId, decision));
  const waiting = task.live;
  const keys = keyLabels();
  useClaimFocus(card, waiting);

  return (
    <section
      ref={card}
      className="permission-card"
      aria-label={title}
      tabIndex={-1}
      onKeyDown={(e) => {
        // Enter on the card itself (a focused button answers on its own).
        if (e.key !== "Enter" || e.target !== e.currentTarget || e.ctrlKey || e.metaKey || e.altKey) return;
        e.preventDefault();
        if (waiting && !answer.isPending) answer.mutate("allow");
      }}
    >
      <h3>{title}</h3>
      <code className="permission-card__what">{item.summary}</code>
      <div className="actions">
        <Button
          variant="primary"
          aria-keyshortcuts="Enter"
          disabled={!waiting || answer.isPending}
          onClick={() => answer.mutate("allow")}
        >
          {t("workspace.permission.allow")}
          <kbd className="kbd" aria-hidden>
            {keys.enter}
          </kbd>
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
  const card = useRef<HTMLElement>(null);
  const keys = keyLabels();
  const approve = () => {
    if (task.live && !answer.isPending) answer.mutate({ approve: true, feedback: null });
  };
  useClaimFocus(card, pending && task.live);

  function shortcut(e: KeyboardEvent<HTMLElement>) {
    if (!pending || changing || e.ctrlKey || e.metaKey || e.altKey || isTextField(e.target)) return;
    if (e.key === "Enter" && e.target === e.currentTarget) {
      e.preventDefault();
      approve();
    } else if ((e.key === "e" || e.key === "E") && task.live) {
      e.preventDefault();
      setChanging(true);
    }
  }

  return (
    <section
      ref={card}
      className={`card plan-card${pending ? " plan-card--pending" : ""}`}
      aria-label={t("workspace.plan.of", { agent })}
      tabIndex={pending ? -1 : undefined}
      onKeyDown={shortcut}
    >
      <h3>
        Plan · {agent}
        {item.resolution && (
          <span className={`plan-card__state plan-card__state--${item.resolution.decision}`}>
            {item.resolution.decision === "deny" ? t("workspace.plan.changesRequested") : t("workspace.plan.approved")}
          </span>
        )}
      </h3>
      <Markdown className="plan-card__text" text={item.text} />
      {pending && !changing && (
        <div className="actions">
          <Button variant="primary" aria-keyshortcuts="Enter" disabled={!task.live || answer.isPending} onClick={approve}>
            {t("workspace.plan.approve")}
            <kbd className="kbd" aria-hidden>
              {keys.enter}
            </kbd>
          </Button>
          <Button aria-keyshortcuts="E" disabled={!task.live} onClick={() => setChanging(true)}>
            {t("workspace.plan.requestChanges")}
            <kbd className="kbd" aria-hidden>
              E
            </kbd>
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
            <textarea autoFocus value={feedback} onChange={(e) => setFeedback(e.target.value)} rows={2} />
          </label>
          <div className="actions">
            <Button type="submit" variant="primary" disabled={!feedback.trim() || answer.isPending}>
              {t("workspace.plan.sendChanges")}
            </Button>
            <Button
              variant="ghost"
              onClick={() => {
                setChanging(false);
                card.current?.focus();
              }}
            >
              {t("common.cancel")}
            </Button>
          </div>
        </form>
      )}
    </section>
  );
}
