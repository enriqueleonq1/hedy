// A flow task, live (docs/decisions.md §47): where it is in its flow (a strip
// of its steps over the timeline, the board the roles share) and the card
// that asks for the user's approval at an approval step.

import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { useId, useMemo, useRef, useState, type KeyboardEvent } from "react";

import { useAction, useApi } from "../../api/context";
import type { BoardEntry, FlowNode, FlowRun, RoleDef, StepSpend, Task } from "../../api/types";
import { Button } from "../../components/ui/Button";
import { formatElapsed, formatTokens } from "../../format";
import { useT } from "../../i18n";
import { keyLabels } from "../../platform";
import { insertInComposer } from "../workspace/composerBridge";
import { isTextField, useClaimFocus } from "../workspace/Timeline";
import { edgeFrom, forward, runOrder } from "./flowModel";
import { StepIcon, stepTint } from "./StepIcon";
import { useStepText } from "./stepText";
import "../../styles/flows.css";

type StepState = "done" | "current" | "pending";

/** Each step's state: the ones before the current one ran, the rest wait; at the end, all ran. */
function states(run: FlowRun): Map<string, StepState> {
  const order = runOrder(run.flow);
  const here = order.findIndex((n) => n.id === run.position.node);
  const ended = order[here]?.kind === "end";
  return new Map(order.map((n, i): [string, StepState] => [n.id, ended || i < here ? "done" : i === here ? "current" : "pending"]));
}

const rolesOf = (run: FlowRun): RoleDef[] => Object.values(run.roles);

const tokensOf = (s: StepSpend) => s.inputTokens + s.outputTokens + s.cacheReadTokens + s.cacheWriteTokens;

/** The run's whole spend: tokens, seconds and the share of the input read from the cache. */
function totals(run: FlowRun): { tokens: number; secs: number; cache: number | null } {
  let tokens = 0;
  let secs = 0;
  let read = 0;
  let input = 0;
  for (const s of Object.values(run.spent ?? {})) {
    tokens += tokensOf(s);
    secs += s.secs;
    read += s.cacheReadTokens;
    input += s.inputTokens + s.cacheReadTokens + s.cacheWriteTokens;
  }
  return { tokens, secs, cache: input > 0 ? Math.round((read / input) * 100) : null };
}

/** The flow's steps in a row, the current one alive; the board under it. */
export function FlowProgress({ task, run }: { task: Task; run: FlowRun }) {
  const t = useT();
  const roles = useMemo(() => rolesOf(run), [run]);
  const text = useStepText(roles);
  const [board, setBoard] = useState(false);
  const boardId = useId();
  const order = useMemo(() => runOrder(run.flow), [run.flow]);
  const state = useMemo(() => states(run), [run]);
  const current = order.find((n) => state.get(n.id) === "current");
  const round = Math.max(0, ...Object.values(run.position.rounds ?? {})) + 1;
  // Waiting at an approval step, or a role at work asking for a permission.
  const waiting = task.status === "waitingApproval" && current?.kind === "approval";
  const asking = task.status === "waitingApproval" && !waiting;
  const working = task.live && task.status === "running";
  // Ended before its end (out of rounds, a failure, stopped): not alive anymore.
  const stopped = task.status !== "running" && task.status !== "waitingApproval";
  const lane = current?.kind === "parallel" ? current.roles[run.lane ?? 0] : undefined;
  const now = current ? (lane ? (roles.find((r) => r.id === lane.role)?.name ?? lane.role) : text(current).name) : "";
  const spent = totals(run);
  const stepSpend = (node: FlowNode) => {
    const s = run.spent?.[node.id];
    return s && tokensOf(s) > 0 ? t("flows.live.spent", { tokens: formatTokens(tokensOf(s)), time: formatElapsed(s.secs) }) : null;
  };
  return (
    <section className="flow-progress" aria-label={t("flows.live.label", { name: run.flow.name })}>
      <header className="flow-progress__head">
        <span className="flow-progress__name">{run.flow.name}</span>
        {current && current.kind !== "end" && (
          <span className="flow-progress__now" data-waiting={waiting || asking || undefined} data-stopped={stopped || undefined}>
            {t(
              waiting ? "flows.live.waiting" : asking ? "flows.live.asking" : stopped ? "flows.live.stopped" : working ? "flows.live.working" : "flows.live.at",
              { step: now },
            )}
            {round > 1 && <span className="flow-progress__round">{t("flows.live.round", { n: String(round) })}</span>}
          </span>
        )}
        <button type="button" className="flow-progress__toggle" aria-expanded={board} aria-controls={boardId} onClick={() => setBoard((b) => !b)}>
          {t("flows.live.board")}
        </button>
      </header>
      <ol className="flow-progress__steps">
        {order.map((node, i) => (
          <li key={node.id} className="flow-progress__step" data-state={state.get(node.id)} data-live={(state.get(node.id) === "current" && (working || waiting || asking)) || undefined} data-waiting={(state.get(node.id) === "current" && (waiting || asking)) || undefined}>
            {i > 0 && <span className="flow-progress__line" aria-hidden="true" data-lit={state.get(node.id) !== "pending" || undefined} />}
            <span
              className="flow-progress__tile"
              style={{ ["--tint" as string]: stepTint(node, roles) }}
              title={[text(node).name, stepSpend(node)].filter(Boolean).join(" · ")}
            >
              <StepIcon node={node} roles={roles} size={16} />
            </span>
            <span className="sr-only">
              {text(node).name}: {t(state.get(node.id) === "done" ? "flows.live.done" : state.get(node.id) === "current" ? "flows.live.current" : "flows.live.pending")}
              {stepSpend(node) && ` · ${stepSpend(node)}`}
            </span>
          </li>
        ))}
      </ol>
      {spent.tokens > 0 && (
        <ul className="flow-progress__costs" aria-label={t("flows.live.costs")}>
          <li>
            <span className="flow-progress__cost-name">{t("flows.live.total")}</span>{" "}
            {t("flows.live.spent", { tokens: formatTokens(spent.tokens), time: formatElapsed(spent.secs) })}
            {spent.cache !== null && ` · ${t("flows.live.cache", { percent: String(spent.cache) })}`}
          </li>
          {/* Step by step only when more than one spent: one would repeat the total. */}
          {order.filter((n) => stepSpend(n)).length > 1 &&
            order.map((node) => {
              const line = stepSpend(node);
              return line ? (
                <li key={node.id}>
                  <span className="flow-progress__cost-name">{text(node).name}</span> {line}
                </li>
              ) : null;
            })}
        </ul>
      )}
      <div id={boardId}>{board && <Board task={task} roles={roles} />}</div>
    </section>
  );
}

/** What the roles left on the board: the latest of each section, as plain text. */
function Board({ task, roles }: { task: Task; roles: RoleDef[] }) {
  const t = useT();
  const api = useApi();
  // Fresh on each step and while a role works (it can post mid-turn); the
  // old entries stay shown meanwhile, no blink.
  const entries = useQuery({
    queryKey: ["taskBoard", task.id, task.status, task.stage],
    queryFn: () => api.taskBoard(task.id),
    placeholderData: keepPreviousData,
    refetchInterval: task.status === "running" ? 5000 : false,
  });
  const sectionName = (section: string): string =>
    section === "frame"
      ? t("flows.live.section.frame")
      : section === "tests"
        ? t("flows.live.section.tests")
        : section === "rules"
          ? t("flows.live.section.rules")
          : (roles.find((r) => r.id === section)?.name ?? section);
  const data = entries.data;
  const latest = useMemo(() => {
    const by = new Map<string, BoardEntry>();
    for (const e of data ?? []) by.set(e.section, e);
    return [...by.values()];
  }, [data]);
  if (entries.isLoading) return null;
  if (latest.length === 0) {
    // A closed task's board is cleared on purpose (§43): say so.
    const closed = task.status === "accepted" || task.status === "discarded";
    return <p className="flow-board__empty">{t(closed ? "flows.live.boardCleared" : "flows.live.boardEmpty")}</p>;
  }
  return (
    <ul className="flow-board" aria-label={t("flows.live.board")}>
      {latest.map((e) => (
        <li key={e.section} className="flow-board__entry">
          <p className="flow-board__meta">
            <span className="flow-board__section">{e.byUser ? t("flows.live.feedback") : sectionName(e.section)}</span>
            {!e.byUser && e.role && <span>{e.role}</span>}
            {e.round > 1 && <span>{t("flows.live.round", { n: String(e.round) })}</span>}
          </p>
          <BoardText text={e.text} />
        </li>
      ))}
    </ul>
  );
}

/** Long entries fold; plain text only (the board is data, never markup). */
function BoardText({ text }: { text: string }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const long = text.length > 420 || text.split("\n").length > 8;
  return (
    <>
      <p className="flow-board__text" data-folded={(long && !open) || undefined}>
        {text}
      </p>
      {long && (
        <button type="button" className="link-button flow-board__more" onClick={() => setOpen((o) => !o)}>
          {t(open ? "flows.live.less" : "flows.live.more")}
        </button>
      )}
    </>
  );
}

/**
 * A way back ran out of rounds: the run ended there and the changes wait for
 * the user, with what was last asked. Another round is a message to the
 * task; the button writes it in the composer.
 */
export function FlowOutOfRounds({ task, rounds, asked }: { task: Task; rounds: number; asked: string }) {
  const t = useT();
  const open = task.status === "review";
  return (
    <section className="card flow-rounds" aria-label={t("flows.rounds.title")}>
      <h3>{t("flows.rounds.title")}</h3>
      <p className="flow-approval__text">{rounds === 1 ? t("flows.rounds.textOne") : t("flows.rounds.text", { n: String(rounds) })}</p>
      {asked && <BoardText text={asked} />}
      {open && (
        <>
          <p className="flow-approval__text">{t("flows.rounds.decide")}</p>
          <div className="actions">
            <Button onClick={() => insertInComposer(t("flows.rounds.againText"))}>{t("flows.rounds.again")}</Button>
          </div>
        </>
      )}
    </section>
  );
}

/** The flow waits for the user: approve, or say what to change (back to the step before). */
export function FlowApprovalCard({
  task,
  node,
  answer,
  latest,
}: {
  task: Task;
  node: string;
  answer: "approved" | "changes" | null;
  latest: boolean;
}) {
  const t = useT();
  const run = task.flow;
  const roles = useMemo(() => (run ? rolesOf(run) : []), [run]);
  const text = useStepText(roles);
  const [changing, setChanging] = useState(false);
  const [feedback, setFeedback] = useState("");
  const card = useRef<HTMLElement>(null);
  const keys = keyLabels();
  const reply = useAction((api, vars: { approve: boolean; feedback: string | null }) =>
    api.answerFlowApproval(task.id, vars.approve, vars.feedback),
  );
  // Waiting is the task's state, not the history's: a message written while
  // it waits leaves the buttons; an answer sent hides them at once.
  const pending = latest && answer === null && !reply.isSuccess && task.status === "waitingApproval" && run?.position.node === node;
  useClaimFocus(card, pending);
  const next: FlowNode | undefined = (() => {
    const here = run?.flow.nodes.find((n) => n.id === node);
    const to = run && here ? edgeFrom(run.flow, node, forward(here))?.to : undefined;
    return to ? run?.flow.nodes.find((n) => n.id === to) : undefined;
  })();
  const approve = () => {
    if (pending && !reply.isPending) reply.mutate({ approve: true, feedback: null });
  };
  const shortcut = (e: KeyboardEvent<HTMLElement>) => {
    if (!pending || changing || e.ctrlKey || e.metaKey || e.altKey) return;
    if (isTextField(e.target)) return;
    if (e.key === "Enter" && e.target === e.currentTarget) {
      e.preventDefault();
      approve();
    } else if (e.key === "e" || e.key === "E") {
      e.preventDefault();
      setChanging(true);
    }
  };
  return (
    <section
      ref={card}
      className={`card plan-card flow-approval${pending ? " plan-card--pending" : ""}`}
      aria-label={t("flows.approval.title")}
      tabIndex={pending ? -1 : undefined}
      onKeyDown={shortcut}
    >
      <h3>
        {t("flows.approval.title")}
        {answer && (
          <span className={`plan-card__state plan-card__state--${answer === "approved" ? "allow" : "deny"}`}>
            {t(answer === "approved" ? "workspace.plan.approved" : "workspace.plan.changesRequested")}
          </span>
        )}
      </h3>
      <p className="flow-approval__text">
        {next && next.kind !== "end" ? t("flows.approval.next", { step: text(next).name }) : t("flows.approval.last")}
      </p>
      {pending && !changing && (
        <div className="actions">
          <Button variant="primary" aria-keyshortcuts="Enter" disabled={reply.isPending} onClick={approve}>
            {t("flows.approval.approve")}
            <kbd className="kbd" aria-hidden>
              {keys.enter}
            </kbd>
          </Button>
          <Button aria-keyshortcuts="E" onClick={() => setChanging(true)}>
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
            reply.mutate({ approve: false, feedback: feedback.trim() || null });
          }}
        >
          <label className="field">
            <span className="field__label">{t("flows.approval.whatToChange")}</span>
            <textarea autoFocus value={feedback} onChange={(e) => setFeedback(e.target.value)} rows={2} />
          </label>
          <div className="actions">
            <Button type="submit" variant="primary" disabled={!feedback.trim() || reply.isPending}>
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
