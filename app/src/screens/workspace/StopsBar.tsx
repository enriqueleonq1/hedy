// The stops of a task that works with one AI, over its conversation (3A,
// docs/decisions.md §60): done, the current one, failed, the ones to come.

import { useT, type MessageKey } from "../../i18n";
import type { Stop, StopKey, StopState } from "../../tasks/stops";

const LABEL: Record<StopKey, MessageKey> = {
  plan: "stops.plan",
  ok: "stops.ok",
  implement: "stops.implement",
  tests: "stops.tests",
  e2e: "stops.e2e",
  review: "stops.review",
  you: "stops.you",
};

const STATE: Record<StopState, MessageKey> = {
  done: "stops.state.done",
  current: "stops.state.current",
  pending: "stops.state.pending",
  failed: "stops.state.failed",
};

const MARK: Partial<Record<StopState, string>> = { done: "✓", failed: "✗" };

export function StopsBar({ stops }: { stops: Stop[] }) {
  const t = useT();
  return (
    <nav className="stops" aria-label={t("stops.label")}>
      <ol className="stops__list">
        {stops.map((stop, i) => (
          <li key={stop.key} className="stops__stop" data-state={stop.state} aria-current={stop.state === "current" ? "step" : undefined}>
            {i > 0 && <span className="stops__line" aria-hidden />}
            <span className="stops__name">
              {MARK[stop.state] && (
                <span className="stops__mark" aria-hidden>
                  {MARK[stop.state]}
                </span>
              )}
              {t(LABEL[stop.key])}
            </span>
            <span className="sr-only">{t(STATE[stop.state])}</span>
          </li>
        ))}
      </ol>
    </nav>
  );
}
