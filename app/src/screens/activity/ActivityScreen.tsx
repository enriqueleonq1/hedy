// The pieces of Inicio's board (`HomeScreen`): a row per task or proposal,
// what can be answered right there, and the board's keyboard.

import { useEffect, useId, useState, type ReactNode, type RefObject } from "react";
import { useQuery } from "@tanstack/react-query";

import { useAction, useApi } from "../../api/context";
import type { ActivityItem, AgentEvent, BoardState, Decision, Overview, Phase, Task } from "../../api/types";
import { WorkingWord } from "../../components/Mascot/WorkingWord";
import { Button } from "../../components/ui/Button";
import { formatElapsed, timeAgo } from "../../format";
import { useLanguage, useT, type MessageKey } from "../../i18n";
import { pillLabel } from "../../components/ui/StatusPill";
import { formatCountdown } from "../../tasks/countdown";
import { ACTIVITY_KEY, listKeyFor, useTaskEvents } from "../../tasks/hooks";
import { openPermission, type InboxItem } from "../../tasks/inbox";
import { useProjectTeam } from "../team/TeamMemory";
import { useFilesNotice } from "../memory/skills";
import { useMascot } from "../../tasks/useMascot";
import { DEFAULT_ACCOUNT_COLOR } from "../../theme/tokens";

/** Finished tasks stay on the board this long (the backend's `RECENT_HOURS`). */
export const RECENT_HOURS = 24;

const PHASE_LABEL: Record<Phase, MessageKey> = {
  plan: "activity.phase.plan",
  implement: "activity.phase.implement",
  tests: "activity.phase.tests",
  review: "activity.phase.review",
};

/**
 * A state's color family. Waiting for a decision (plan, permission) is amber,
 * blocked by a plan limit is red, proposals (`ProposalRow`) are violet.
 */
export type Tone = "needs" | "blocked" | "proposal" | "working" | "done" | "failed" | "closed";

export const TONE: Record<BoardState, Tone> = {
  plan: "needs",
  permission: "needs",
  limited: "blocked",
  working: "working",
  review: "done",
  answered: "done",
  failed: "failed",
  accepted: "closed",
  cancelled: "closed",
};

export const MARK: Record<BoardState, string> = {
  plan: "◆",
  permission: "◆",
  limited: "■",
  working: "▶",
  review: "✔",
  answered: "✔",
  failed: "✗",
  accepted: "·",
  cancelled: "·",
};

/** The current time in seconds, refreshed every `everyMs`. */
export function useNow(everyMs: number): number {
  const [now, setNow] = useState(() => Date.now() / 1000);
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now() / 1000), everyMs);
    return () => clearInterval(timer);
  }, [everyMs]);
  return now;
}

/** Where typing happens: the board's letter keys stay out of it. */
function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return (
    target.isContentEditable ||
    !!target.closest("input, textarea, select, [role=combobox], [role=menu], [role=listbox], .xterm")
  );
}

const ENTRY = "[data-entry]";

/**
 * The board's keyboard: j/k (or ↓/↑) move the selection through every row,
 * Enter opens the selected one, `a` approves or allows once and `r` rejects or
 * denies, where the row has those buttons. Keys are ignored while typing or
 * with a dialog open. The selection is the focused row: focus moves with it
 * (so screen readers follow) and Tab or a click selects too. When the
 * selected row leaves (answered, approved), the one now in its place takes
 * over. `keys`: every row's key, in order. Returns the selected row's key.
 */
export function useBoardKeys(root: RefObject<HTMLDivElement | null>, keys: string[]): string | null {
  const [chosen, setChosen] = useState<{ key: string; index: number } | null>(null);
  const selected =
    chosen === null
      ? null
      : keys.includes(chosen.key)
        ? chosen.key
        : (keys[Math.min(chosen.index, keys.length - 1)] ?? null);

  // Tab or a click into a row selects it.
  useEffect(() => {
    const el = root.current;
    if (!el) return;
    const onFocus = (e: FocusEvent) => {
      const entry = (e.target as HTMLElement).closest<HTMLElement>(ENTRY);
      const list = entriesOf(el);
      if (entry?.dataset.entry) setChosen({ key: entry.dataset.entry, index: list.indexOf(entry) });
    };
    el.addEventListener("focusin", onFocus);
    return () => el.removeEventListener("focusin", onFocus);
  }, [root]);

  // The selected row went away and took the focus with it: the next one gets it.
  useEffect(() => {
    if (selected === null || selected === chosen?.key) return;
    if (document.activeElement && document.activeElement !== document.body) return;
    focusRow(entriesOf(root.current).find((el) => el.dataset.entry === selected));
  }, [root, selected, chosen]);

  useEffect(() => {
    const select = (list: HTMLElement[], index: number) => {
      const el = list[index];
      if (!el?.dataset.entry) return;
      setChosen({ key: el.dataset.entry, index });
      const reduced = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
      el.scrollIntoView?.({ block: "nearest", behavior: reduced ? "auto" : "smooth" });
      focusRow(el);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.ctrlKey || e.metaKey || e.altKey) return;
      if (isTypingTarget(e.target) || document.querySelector("[aria-modal=true]")) return;
      const list = entriesOf(root.current);
      if (list.length === 0) return;
      const at = list.findIndex((el) => el.dataset.entry === selected);
      const focus = document.activeElement;
      // Elsewhere (the sidebar, the filters…) keys keep their own meaning.
      const onBoard =
        !focus ||
        focus === document.body ||
        (!!root.current?.contains(focus) && !(focus instanceof Element && focus.closest("[data-own-keys]")));
      if (!onBoard) return;
      const press = (action: string) => {
        const button = list[at]?.querySelector<HTMLButtonElement>(`[data-key-action="${action}"]:not(:disabled)`);
        if (!button) return;
        e.preventDefault();
        button.click();
      };
      switch (e.key) {
        case "ArrowDown":
        case "ArrowUp":
          e.preventDefault();
          select(list, at < 0 ? 0 : Math.max(0, Math.min(at + (e.key === "ArrowDown" ? 1 : -1), list.length - 1)));
          break;
        case "j":
        case "k":
          e.preventDefault();
          select(list, at < 0 ? 0 : Math.max(0, Math.min(at + (e.key === "j" ? 1 : -1), list.length - 1)));
          break;
        case "Enter":
          // On a button, Enter already presses it.
          if (focus instanceof HTMLButtonElement || focus instanceof HTMLAnchorElement) return;
          press("open");
          break;
        case "a":
          press("approve");
          break;
        case "r":
          press("reject");
          break;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [root, selected]);

  return selected;
}

function entriesOf(root: HTMLElement | null): HTMLElement[] {
  return [...(root?.querySelectorAll<HTMLElement>(ENTRY) ?? [])];
}

function focusRow(entry: HTMLElement | undefined) {
  entry?.querySelector<HTMLElement>(".board-row")?.focus({ preventScroll: true });
}

/** "j/k para moverte · Enter abrir · a aprobar · r rechazar": the board's keys, said once and quietly. */
export function KeysHint() {
  const t = useT();
  return (
    <p className="board__keys" aria-hidden>
      <span>
        <kbd className="kbd">j</kbd>
        <kbd className="kbd">k</kbd> {t("activity.keys.move")}
      </span>
      <span>
        <kbd className="kbd">Enter</kbd> {t("activity.keys.open")}
      </span>
      <span>
        <kbd className="kbd">a</kbd> {t("activity.keys.approve")}
      </span>
      <span>
        <kbd className="kbd">r</kbd> {t("activity.keys.reject")}
      </span>
    </p>
  );
}

export function BoardGroup<T>({
  title,
  items,
  note,
  children,
}: {
  title: string;
  items: T[];
  note?: string;
  children: (item: T) => ReactNode;
}) {
  const id = useId();
  return (
    <section className="board__group" aria-labelledby={id}>
      <h2 id={id} className="board__group-title">
        {title} <span className="muted">({items.length})</span>
      </h2>
      <ul className="board__list">{items.map(children)}</ul>
      {note && <p className="hint">{note}</p>}
    </section>
  );
}

export function BoardRow({
  entryKey,
  selected,
  item,
  overview,
  now,
  onOpen,
}: {
  entryKey: string;
  selected: boolean;
  item: ActivityItem;
  overview: Overview;
  now: number;
  onOpen: (project: string, taskId: string) => void;
}) {
  const t = useT();
  const language = useLanguage();
  const { task, state } = item;
  const account = overview.accounts.find((a) => a.id === task.account);
  // While it works, the state reads as what it is doing ("Leyendo…").
  const events = useTaskEvents(state === "working" ? task.id : null);
  const { activity } = useMascot(state === "working" ? task : null, events.data ?? []);
  const tone = TONE[state];
  const active = tone === "needs" || tone === "blocked" || state === "working";
  const time = active
    ? t("activity.runningFor", { time: formatElapsed(now - task.createdAt) })
    : timeAgo(item.updatedAt, t, now);
  const live = state === "permission" && task.live;
  // The same words as the task's pill everywhere else.
  const label = pillLabel(state, task, t, language);
  const row = (
    <button
      className={`board-row board-row--${tone}`}
      data-key-action="open"
      onClick={() => onOpen(task.project, task.id)}
    >
      <span className="board-row__title">{task.title}</span>
      <span className="board-row__meta">
        <span className={`board-state board-state--${tone}`}>
          <span className="board-state__mark" aria-hidden>
            {MARK[state]}
          </span>
          {activity ? (
            <>
              <span className="sr-only">{label}</span>
              <WorkingWord activity={activity} />
            </>
          ) : (
            label
          )}
        </span>
        <span aria-hidden>·</span>
        <span className="board-row__project">{item.projectName}</span>
        <span aria-hidden>·</span>
        <span className="board-row__who">
          <span className="dot" style={{ background: account?.color ?? DEFAULT_ACCOUNT_COLOR }} aria-hidden />
          {account?.name ?? task.account}
        </span>
        <span aria-hidden>·</span>
        <span>{item.agent}</span>
        {task.mode === "project" && (
          <>
            <span aria-hidden>·</span>
            <span>{t(PHASE_LABEL[item.phase])}</span>
          </>
        )}
        <span aria-hidden>·</span>
        <span>{time}</span>
        {item.queued > 0 && (
          <>
            <span aria-hidden>·</span>
            <span className="board-row__queued">{t("queue.count", { n: item.queued })}</span>
          </>
        )}
        <WaitCountdown task={task} now={now} />
      </span>
    </button>
  );
  const entryClass = `board-entry${selected ? " board-entry--selected" : ""}`;
  // What is answered right here: a permission, a plan, a limit; a finished
  // task offers its next step. Anything else opens the task.
  const actions =
    live ? (
      <PermissionActions task={task} />
    ) : state === "plan" && task.live ? (
      <PlanActions task={task} onOpen={() => onOpen(task.project, task.id)} selected={selected} />
    ) : state === "limited" ? (
      <LimitActions task={task} overview={overview} />
    ) : state === "review" || state === "answered" || state === "failed" ? (
      <NextStep
        label={t(state === "review" ? "home.reviewChanges" : state === "answered" ? "home.seeAnswer" : "home.seeError")}
        onClick={() => onOpen(task.project, task.id)}
      />
    ) : null;
  if (!actions) {
    return (
      <li className={entryClass} data-entry={entryKey}>
        {row}
      </li>
    );
  }
  return (
    <li className={entryClass} data-entry={entryKey}>
      <div className={`board-item board-item--${tone}`}>
        {row}
        {actions}
      </div>
    </li>
  );
}

/** The plan still waiting for an answer, if any (its request and text). */
function openPlan(events: AgentEvent[]): { requestId: string; text: string } | null {
  let open: { requestId: string; text: string } | null = null;
  for (const e of events) {
    if (e.kind === "plan" && e.requestId) open = { requestId: e.requestId, text: e.text };
    if (e.kind === "permissionResolved" && open?.requestId === e.requestId) open = null;
  }
  return open;
}

/** Approve the plan right here, or open the task to ask for changes; the selected row shows the plan. */
function PlanActions({ task, onOpen, selected }: { task: Task; onOpen: () => void; selected: boolean }) {
  const t = useT();
  const events = useTaskEvents(task.id);
  const plan = openPlan(events.data ?? []);
  const answer = useAction((api) => api.answerPlan(task.id, plan?.requestId ?? "", true, null), {
    invalidate: [ACTIVITY_KEY, listKeyFor(task)],
  });
  if (!plan) return null;
  return (
    <>
      <div className="board-item__actions" role="group" aria-label={t("home.planOf", { title: task.title })}>
        <Button small variant="ghost" onClick={onOpen}>
          {t("home.askChanges")}
        </Button>
        <Button small variant="primary" data-key-action="approve" disabled={answer.isPending} onClick={() => answer.mutate(undefined)}>
          {t("home.approvePlan")}
        </Button>
      </div>
      {selected && <pre className="board-item__plan">{plan.text}</pre>}
    </>
  );
}

/**
 * A task paused by its account's limit: wait for the reset, or hand it to
 * another provider or an API key. Hedy never switches by itself (rule 3).
 */
function LimitActions({ task, overview }: { task: Task; overview: Overview }) {
  const t = useT();
  const api = useApi();
  const options = useQuery({ queryKey: ["handoff", task.id, "board"], queryFn: () => api.handoffOptions(task.id) });
  const changed = [ACTIVITY_KEY, listKeyFor(task)];
  const wait = useAction((a) => a.waitForLimit(task.id), { invalidate: changed });
  const cancel = useAction((a) => a.cancelWait(task.id), { invalidate: changed });
  const resume = useAction((a) => a.resumeAfterLimit(task.id), { invalidate: changed });
  const handoff = useAction((a, account: string) => a.handoffTask(task.id, account, null), { invalidate: changed });
  const other = options.data?.[0];
  const otherName = overview.accounts.find((a) => a.id === other)?.name ?? other;
  const busy = wait.isPending || cancel.isPending || resume.isPending || handoff.isPending;
  // The reset passed while Hedy was closed, or its time is unknown: the user resumes.
  const canContinue = !!task.wait && (task.wait.ready || task.wait.until === null);
  return (
    <div className="board-item__actions" role="group" aria-label={t("home.limitOf", { title: task.title })}>
      {other && (
        <Button small variant="ghost" disabled={busy} onClick={() => handoff.mutate(other)}>
          {t("home.handoff", { name: otherName ?? "" })}
        </Button>
      )}
      {task.wait && (
        <Button small variant="ghost" disabled={busy} onClick={() => cancel.mutate(undefined)}>
          {t("wait.cancel")}
        </Button>
      )}
      {!task.wait && (
        <Button small data-key-action="approve" disabled={busy} onClick={() => wait.mutate(undefined)}>
          {t("home.wait")}
        </Button>
      )}
      {canContinue && (
        <Button small variant="primary" data-key-action="approve" disabled={busy} onClick={() => resume.mutate(undefined)}>
          {t("wait.continue")}
        </Button>
      )}
    </div>
  );
}

/** A finished task's next step: review its changes, or read its answer. */
function NextStep({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <div className="board-item__actions">
      <Button small data-key-action="approve" onClick={onClick}>
        {label}
      </Button>
    </div>
  );
}

/** Allow once or deny the waiting request; "allow in this chat" stays on the task's own card. */
function PermissionActions({ task }: { task: Task }) {
  const t = useT();
  const events = useTaskEvents(task.id);
  const request = openPermission(events.data ?? []);
  const answer = useAction(
    (api, decision: Decision) => api.answerPermission(task.id, request?.requestId ?? "", decision),
    { invalidate: [ACTIVITY_KEY, listKeyFor(task)] },
  );
  if (!request) return null;
  return (
    <div className="board-item__actions" role="group" aria-label={t("activity.inbox.permissionOf", { title: task.title })}>
      <code className="board-item__what" title={`${request.tool}: ${request.summary}`}>
        {request.tool}: {request.summary}
      </code>
      <Button
        small
        variant="primary"
        data-key-action="approve"
        disabled={answer.isPending}
        onClick={() => answer.mutate("allow")}
      >
        {t("activity.inbox.allowOnce")}
      </Button>
      <Button
        small
        variant="ghost"
        data-key-action="reject"
        disabled={answer.isPending}
        onClick={() => answer.mutate("deny")}
      >
        {t("activity.inbox.deny")}
      </Button>
    </div>
  );
}

/**
 * A note or skill an AI proposed, approved or rejected right here by the user
 * (rule 6); the row opens the project's memory to read it in full first.
 */
export function ProposalRow({
  entryKey,
  selected,
  entry,
  overview,
  now,
  onOpen,
}: {
  entryKey: string;
  selected: boolean;
  entry: Extract<InboxItem, { kind: "note" | "skill" }>;
  overview: Overview;
  now: number;
  onOpen: (project: string, tab: "notes" | "skills") => void;
}) {
  const t = useT();
  const notice = useFilesNotice();
  const isNote = entry.kind === "note";
  const scope = entry.kind === "note" ? entry.proposal.note.project : entry.proposal.skill.project;
  const author = entry.kind === "note" ? entry.proposal.note.author : entry.proposal.skill.author;
  const createdAt = entry.kind === "note" ? entry.proposal.note.createdAt : entry.proposal.skill.createdAt;
  const change = !!entry.proposal.replaces;
  const title =
    entry.kind === "note" ? entry.proposal.note.text : `${entry.proposal.skill.name} — ${entry.proposal.skill.description}`;
  const state = isNote
    ? t(change ? "activity.inbox.noteChange" : "activity.inbox.note")
    : t(change ? "activity.inbox.skillChange" : "activity.inbox.skill");
  const where =
    scope === null
      ? t("activity.inbox.global")
      : (overview.projects.find((p) => p.path === scope)?.name ?? scope.split("/").pop() ?? scope);
  // A global note shows in every project's memory: refresh them all.
  const invalidate = isNote ? [["memory"], ["status"]] : [["skills"]];
  const approve = useAction(
    async (api) => {
      if (entry.kind === "note") return api.approveNote(entry.proposal.note.id);
      const result = await api.approveSkill(entry.proposal.skill.id);
      notice("saved", result.skill.name, result.files, t);
    },
    { invalidate },
  );
  const reject = useAction(
    (api) => (entry.kind === "note" ? api.forgetNote(entry.proposal.note.id) : api.rejectSkill(entry.proposal.skill.id)),
    { invalidate },
  );
  const busy = approve.isPending || reject.isPending;
  const short = title.length > 60 ? `${title.slice(0, 57)}…` : title;
  // Decided where its context is: a note like an approved one, or a team's
  // project (who it is for is asked there, never assumed here).
  const team = useProjectTeam(entry.project);
  const needsContext =
    entry.kind === "note" && ((!entry.proposal.replaces && entry.proposal.similar.length > 0) || team !== null);

  return (
    <li className={`board-entry${selected ? " board-entry--selected" : ""}`} data-entry={entryKey}>
      <div className="board-item board-item--proposal">
        <button
          className="board-row board-row--proposal"
          data-key-action="open"
          onClick={() => onOpen(entry.project, isNote ? "notes" : "skills")}
        >
          <span className="board-row__title">{title}</span>
          <span className="board-row__meta">
            <span className="board-state board-state--proposal">
              <span aria-hidden>◇</span>
              {state}
            </span>
            <span aria-hidden>·</span>
            <span className="board-row__project">{where}</span>
            <span aria-hidden>·</span>
            <span>{t("activity.inbox.proposedBy", { author: author === "user" ? t("memory.note.you") : author })}</span>
            <span aria-hidden>·</span>
            <span>{timeAgo(createdAt, t, now)}</span>
            {needsContext && entry.kind === "note" && entry.proposal.similar.length > 0 && (
              <>
                <span aria-hidden>·</span>
                <span className="board-row__warn">{t("home.looksLike")}</span>
              </>
            )}
          </span>
        </button>
        <div className="board-item__actions" role="group" aria-label={t("activity.inbox.decide", { title: short })}>
          {needsContext ? (
            <Button small data-key-action="approve" onClick={() => onOpen(entry.project, "notes")}>
              {t("home.seeAndDecide")}
            </Button>
          ) : (
            <>
              <Button small variant="ghost" data-key-action="reject" disabled={busy} onClick={() => reject.mutate(undefined)}>
                {t("activity.inbox.reject")}
              </Button>
              <Button small variant="primary" data-key-action="approve" disabled={busy} onClick={() => approve.mutate(undefined)}>
                {t(change ? "activity.inbox.approveChange" : "activity.inbox.approve")}
              </Button>
            </>
          )}
        </div>
      </div>
    </li>
  );
}

/** A task waiting for its limit: when it resumes, or that it can now. */
function WaitCountdown({ task, now }: { task: ActivityItem["task"]; now: number }) {
  const t = useT();
  const wait = task.wait;
  if (!wait || task.status === "accepted" || task.status === "discarded") return null;
  const left = wait.until ? formatCountdown(wait.until - now, t) : null;
  const text = wait.ready ? t("wait.boardReady") : left ? t("wait.board", { time: left }) : null;
  if (!text) return null;
  return (
    <>
      <span aria-hidden>·</span>
      <span className="board-row__wait">{text}</span>
    </>
  );
}
