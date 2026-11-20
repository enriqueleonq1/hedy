import { useEffect, useId, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";

import type { Account, BoardState, Task } from "../../api/types";
import { timeAgo } from "../../format";
import { ChevronIcon } from "../../components/ui/icons";
import { useT, type MessageKey } from "../../i18n";
import { keyLabels } from "../../platform";
import { useActivity } from "../../tasks/hooks";
import { DEFAULT_ACCOUNT_COLOR } from "../../theme/tokens";
import { StatusPill } from "../../components/ui/StatusPill";
import { taskState } from "../../tasks/state";
import { PlusIcon } from "./icons";

/** How many tasks have a Ctrl/⌘ + digit shortcut. */
export const JUMP_KEYS = 9;

/** The digit of a Ctrl/⌘ + 1…9 press (any layout), or null. */
export function jumpDigit(e: KeyboardEvent): number | null {
  if (!(e.ctrlKey || e.metaKey) || e.altKey || e.shiftKey) return null;
  const digit = /^[1-9]$/.test(e.key) ? e.key : /^Digit([1-9])$/.exec(e.code)?.[1];
  return digit ? Number(digit) : null;
}

/** The switcher's groups, in order: what needs the user first, finished ones folded. */
type Group = "needs" | "working" | "ready" | "done";

const GROUP: Record<BoardState, Group> = {
  plan: "needs",
  permission: "needs",
  limited: "needs",
  failed: "needs",
  working: "working",
  review: "ready",
  answered: "ready",
  accepted: "done",
  cancelled: "done",
};

const GROUP_LABEL: Record<Exclude<Group, "done">, MessageKey> = {
  needs: "switcher.needs",
  working: "switcher.working",
  ready: "switcher.ready",
};

/** From this many tasks on, a filter field heads the list. */
const FILTER_FROM = 9;

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
  const [query, setQuery] = useState("");
  const [showDone, setShowDone] = useState(false);

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

  const needle = query.trim().toLowerCase();
  const rows = tasks
    .map((task, index) => ({ task, index, state: taskState(task, byId.get(task.id)) }))
    .filter(({ task }) => !needle || task.title.toLowerCase().includes(needle));
  const groups = (["needs", "working", "ready", "done"] as Group[])
    .map((group) => ({ group, rows: rows.filter((r) => GROUP[r.state] === group) }))
    .filter((g) => g.rows.length > 0);

  const option = ({ task, index, state }: { task: Task; index: number; state: BoardState }) => {
    const item = byId.get(task.id);
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
            <StatusPill state={state} task={task} />
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
  };

  return (
    <div className="task-switcher" ref={root} onKeyDown={open ? move : undefined}>
      <button
        ref={toggle}
        type="button"
        className="tool-btn task-switcher__toggle"
        aria-expanded={open}
        aria-controls={open ? panelId : undefined}
        onClick={() => setOpen((o) => !o)}
      >
        <span className="tool-btn__label">
          {conversations ? t("conversation.count", { n: tasks.length }) : t("workspace.tasks", { n: tasks.length })}
        </span>
        <span className="tool-btn__short" aria-hidden>
          {tasks.length}
        </span>
        <span className={`task-switcher__chevron${open ? " task-switcher__chevron--open" : ""}`} aria-hidden>
          <ChevronIcon />
        </span>
      </button>
      {open && (
        <div className="task-switcher__panel" id={panelId} role="group" aria-label={conversations ? t("conversation.list") : t("workspace.taskList")}>
          <button type="button" className="btn btn--secondary btn--small task-switcher__new" onClick={() => choose(null)}>
            <PlusIcon />
            {conversations ? t("conversation.newLabel") : t("workspace.newTask")}
          </button>
          {tasks.length >= FILTER_FROM && (
            <input
              className="input task-switcher__filter"
              type="search"
              aria-label={t("switcher.filter")}
              placeholder={t("switcher.filter")}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
          )}
          {groups.map(({ group, rows }) =>
            group === "done" && !showDone && !needle && !rows.some((r) => r.task.id === current) ? (
              <button
                key={group}
                type="button"
                className="task-switcher__more"
                aria-expanded={false}
                onClick={() => {
                  setShowDone(true);
                  // The button goes away: the focus moves to the first finished task.
                  requestAnimationFrame(() =>
                    root.current?.querySelector<HTMLButtonElement>('[data-group="done"] .task-option')?.focus(),
                  );
                }}
              >
                {t("switcher.done", { n: rows.length })}
              </button>
            ) : (
              <section key={group} data-group={group} className="task-switcher__group" aria-label={group === "done" ? t("switcher.done", { n: rows.length }) : t(GROUP_LABEL[group])}>
                <h3 className="task-switcher__heading" aria-hidden>
                  {group === "done" ? t("switcher.done", { n: rows.length }) : t(GROUP_LABEL[group])}
                </h3>
                <ul className="task-switcher__list">{rows.map(option)}</ul>
              </section>
            ),
          )}
          {needle && groups.length === 0 && <p className="muted small task-switcher__empty">{t("switcher.noMatch")}</p>}
        </div>
      )}
    </div>
  );
}
