// The mascot of a task: its state from the task's status, and, while it
// works, the activity of the latest tool, with the clock (silence, long
// turns) and a minimum hold so it does not flicker.

import { useEffect, useRef, useState } from "react";

import type { AgentEvent, Task } from "../api/types";
import type { MascotState } from "../components/Mascot/Mascot";
import { ACTIVITY_TIMING, activityOfEvents, withClock, type WorkingActivity } from "../components/Mascot/activity";
import { mascotState } from "./timeline";

export interface TaskMascot {
  state: MascotState;
  /** Set only while `working`. */
  activity: WorkingActivity | undefined;
}

/**
 * `value`, but each value stays at least `minMs`: a change that comes sooner
 * waits for the rest of that time, and only the latest one is shown then.
 */
export function useHeld<T>(value: T, minMs: number): T {
  const [shown, setShown] = useState(value);
  const since = useRef<number | null>(null);
  useEffect(() => {
    since.current ??= Date.now();
    if (Object.is(value, shown)) return;
    const wait = Math.max(0, since.current + minMs - Date.now());
    const timer = setTimeout(() => {
      since.current = Date.now();
      setShown(value);
    }, wait);
    return () => clearTimeout(timer);
  }, [value, shown, minMs]);
  return shown;
}

/** True once `ms` have passed since `key` last changed (and while it stays the same). */
function useElapsed(key: string, ms: number | null): boolean {
  const [reached, setReached] = useState<string | null>(null);
  useEffect(() => {
    if (ms === null) return;
    const timer = setTimeout(() => setReached(key), Math.max(0, ms));
    return () => clearTimeout(timer);
  }, [key, ms]);
  return reached === key;
}

/** When the current turn started, in ms: the task's creation on its first turn, else now. */
function useTurnDelay(task: Task | null | undefined, turn: number, active: boolean): number | null {
  const [delay, setDelay] = useState<number | null>(null);
  const createdAt = task?.createdAt ?? 0;
  useEffect(() => {
    if (!active) return;
    // Events carry no time: a follow-up turn counts from when it is first seen.
    const start = turn <= 1 ? createdAt * 1000 : Date.now();
    const timer = setTimeout(() => setDelay(start + ACTIVITY_TIMING.longAfterMs - Date.now()), 0);
    return () => clearTimeout(timer);
  }, [createdAt, turn, active]);
  return active ? delay : null;
}

export function useMascot(task: Task | null | undefined, events: AgentEvent[]): TaskMascot {
  const state = mascotState(task, events);
  const working = state === "working";
  const id = task?.id ?? "";
  const turn = events.filter((e) => e.kind === "userMessage").length;

  const silent = useElapsed(`${id}:${events.length}`, working ? ACTIVITY_TIMING.waitingAfterMs : null);
  const longTurn = useElapsed(`${id}:${turn}`, useTurnDelay(task, turn, working));
  const activity = useHeld(withClock(activityOfEvents(events), { silent, longTurn }), ACTIVITY_TIMING.minHoldMs);
  return { state, activity: working ? activity : undefined };
}
