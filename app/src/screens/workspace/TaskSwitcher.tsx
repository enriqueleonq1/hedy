import { useEffect, useId, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";

import type { Account, ActivityItem, BoardState, Task } from "../../api/types";
import { timeAgo } from "../../format";
import { useT } from "../../i18n";
import { keyLabels } from "../../platform";
import { useActivity } from "../../tasks/hooks";
import { DEFAULT_ACCOUNT_COLOR } from "../../theme/tokens";
import { MARK, STATE_LABEL, TONE } from "../activity/ActivityScreen";

/** How many tasks have a Ctrl/⌘ + digit shortcut. */
export const JUMP_KEYS = 9;

/** The digit of a Ctrl/⌘ + 1…9 press (any layout), or null. */
export function jumpDigit(e: KeyboardEvent): number | null {
  if (!(e.ctrlKey || e.metaKey) || e.altKey || e.shiftKey) return null;
  const digit = /^[1-9]$/.test(e.key) ? e.key : /^Digit([1-9])$/.exec(e.code)?.[1];
  return digit ? Number(digit) : null;
}

/** The board's state for a task; derived from its status when the board does not list it. */
function stateOf(task: Task, item: ActivityItem | undefined): BoardState {
  if (item) return item.state;
  switch (task.status) {
    case "running":
      return "working";
    case "waitingApproval":
      return "plan";
    case "review":
      return task.mode === "project" ? "review" : "answered";
    case "accepted":
      return "accepted";
    case "discarded":
      return "cancelled";
    case "failed":
      return "failed";
  }
}

/**
 * The project's tasks, a click away: each one's full title, state, account
 * and time, the current one marked, and a new task. Ctrl/⌘ + 1…9 jump to
 * the first nine (handled by the workspace, shown here).
 */
export function TaskSwitcher({
  tasks,
  current,
  accounts,
  onSelect,
  conversations = false,
}: {
  tasks: Task[];
  current: string | null;
  accounts: Account[];
  onSelect: (task: string | null) => void;
  /** The list holds conversations outside any project, not a project's tasks. */
  conversations?: boolean;
}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const panelId = useId();
  const root = useRef<HTMLDivElement>(null);
  const toggle = useRef<HTMLButtonElement>(null);
  const board = useActivity();
  const byId = new Map((board.data ?? []).map((item) => [item.task.id, item]));
  const keys = keyLabels();

  // Clicking anywhere else closes it.
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  // Opening it puts the focus on the current task (or the first one).
  useEffect(() => {
    if (!open) return;
    const panel = root.current?.querySelector(".task-switcher__panel");
    const target =
      panel?.querySelector<HTMLButtonElement>('.task-option[aria-current="true"]') ??
      panel?.querySelector<HTMLButtonElement>(".task-option");
    target?.focus();
  }, [open]);

  function choose(task: string | null) {
    setOpen(false);
    toggle.current?.focus();
    onSelect(task);
  }

  function move(e: ReactKeyboardEvent<HTMLDivElement>) {
    if (e.key === "Escape") {
      e.preventDefault();
      setOpen(false);
      toggle.current?.focus();
      return;
    }
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    const items = [...(root.current?.querySelectorAll<HTMLButtonElement>(".task-switcher__panel button") ?? [])];
    const index = items.indexOf(document.activeElement as HTMLButtonElement);
    const next = items[(index + (e.key === "ArrowDown" ? 1 : -1) + items.length) % items.length];
    if (next) {
      e.preventDefault();
      next.focus();
    }
  }

  return (
    <div className="task-switcher" ref={root} onKeyDown={open ? move : undefined}>
      <button
        ref={toggle}
        type="button"
        className="btn btn--secondary btn--small task-switcher__toggle"
        aria-expanded={open}
        aria-controls={open ? panelId : undefined}
        onClick={() => setOpen((o) => !o)}
      >
        {conversations ? t("conversation.count", { n: tasks.length }) : t("workspace.tasks", { n: tasks.length })}
        <span className={`task-switcher__chevron${open ? " task-switcher__chevron--open" : ""}`} aria-hidden>
          ▾
        </span>
      </button>
      {open && (
        <div className="task-switcher__panel" id={panelId} role="group" aria-label={conversations ? t("conversation.list") : t("workspace.taskList")}>
          <button type="button" className="btn btn--primary btn--small task-switcher__new" onClick={() => choose(null)}>
            {conversations ? t("conversation.newLabel") : t("workspace.newTask")}
          </button>
          <ul className="task-switcher__list">
            {tasks.map((task, index) => {
              const item = byId.get(task.id);
              const state = stateOf(task, item);
              const account = accounts.find((a) => a.id === task.account);
              const jump = index < JUMP_KEYS ? keys.jump(index + 1) : null;
              return (
                <li key={task.id}>
                  <button
                    type="button"
                    className="task-option"
                    aria-current={task.id === current ? "true" : undefined}
                    aria-keyshortcuts={jump ? `Control+${index + 1} Meta+${index + 1}` : undefined}
                    onClick={() => choose(task.id)}
                  >
                    <span className="task-option__title">{task.title}</span>
                    {jump && (
                      <kbd className="kbd task-option__key" aria-hidden>
                        {jump}
                      </kbd>
                    )}
                    <span className="task-option__meta">
                      <span className={`board-state board-state--${TONE[state]}`}>
                        <span aria-hidden>{MARK[state]}</span>
                        {t(STATE_LABEL[state])}
                      </span>
                      <span aria-hidden>·</span>
                      <span className="task-option__who">
                        <span className="dot" style={{ background: account?.color ?? DEFAULT_ACCOUNT_COLOR }} aria-hidden />
                        {account?.name ?? task.account}
                      </span>
                      <span aria-hidden>·</span>
                      <span>{timeAgo(item?.updatedAt ?? task.createdAt, t)}</span>
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>
        </div>
      )}
    </div>
  );
}
