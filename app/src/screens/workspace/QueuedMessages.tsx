import { useState } from "react";

import { useAction } from "../../api/context";
import type { HoldReason, QueuedMessage, Task } from "../../api/types";
import { Button } from "../../components/ui/Button";
import { isBusy, useSetQueue } from "../../tasks/hooks";
import { useT, type MessageKey } from "../../i18n";
import { CloseIcon, PencilIcon } from "./icons";

const HELD: Record<HoldReason, MessageKey> = {
  failed: "queue.held.failed",
  stopped: "queue.held.stopped",
  limit: "queue.held.limit",
};

/**
 * Messages written while the agent works ("En cola"): editable and removable
 * until they go, all together, as the next user turn. On hold after a
 * failure, a stop or a limit: only the user sends them (spec rule 3).
 */
export function QueuedMessages({
  task,
  queue,
  agent,
  cardWaiting,
}: {
  task: Task;
  queue: QueuedMessage[];
  agent: string;
  /** A plan or permission card waits: its decision comes first. */
  cardWaiting: boolean;
}) {
  const t = useT();
  const setQueue = useSetQueue();
  const sendNow = useAction((api) => api.sendQueuedNow(task.id), {
    invalidate: [],
    onSuccess: (left) => setQueue(task.id, left),
  });
  if (queue.length === 0) return null;

  const busy = isBusy(task);
  const held = queue.find((m) => m.held !== null)?.held ?? null;
  const status = held
    ? t(HELD[held])
    : !busy
      ? t("queue.idle")
      : cardWaiting
        ? t("queue.waitingCard")
        : t("queue.waiting", { agent });

  return (
    <section className="queue" aria-label={t("queue.list")}>
      <ul className="queue__list">
        {queue.map((message) => (
          <QueuedPill key={message.id} message={message} />
        ))}
      </ul>
      <div className={`queue__status${held ? " queue__status--held" : ""}`}>
        <p>{status}</p>
        {(held || !busy) && (
          <Button small variant={held ? "secondary" : "primary"} disabled={sendNow.isPending} onClick={() => sendNow.mutate(undefined)}>
            {busy ? t("queue.sendAfterTurn") : t("queue.sendNow")}
          </Button>
        )}
      </div>
    </section>
  );
}

/**
 * One queued message: a single line above the composer (the whole text in
 * its tooltip), with edit and remove shown on hover or keyboard focus.
 */
function QueuedPill({ message }: { message: QueuedMessage }) {
  const t = useT();
  const setQueue = useSetQueue();
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(message.text);
  const edit = useAction((api, text: string) => api.editQueued(message.id, text), {
    invalidate: [],
    onSuccess: (left) => {
      setQueue(message.task, left);
      setEditing(false);
    },
  });
  const remove = useAction((api) => api.removeQueued(message.id), {
    invalidate: [],
    onSuccess: (left) => setQueue(message.task, left),
  });
  const save = () => {
    if (draft.trim() && !edit.isPending) edit.mutate(draft);
  };
  const cancel = () => {
    setEditing(false);
    setDraft(message.text);
  };

  if (editing) {
    return (
      <li className="queue-pill queue-pill--editing">
        <form
          className="queue-pill__edit"
          onSubmit={(e) => {
            e.preventDefault();
            save();
          }}
        >
          <textarea
            aria-label={t("queue.editLabel")}
            value={draft}
            rows={2}
            autoFocus
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.nativeEvent.isComposing) return;
              if (e.key === "Escape") {
                e.preventDefault();
                e.stopPropagation();
                cancel();
              }
              // Like the composer: Enter saves, Shift + Enter is a new line.
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                save();
              }
            }}
          />
          <div className="actions">
            <Button small type="submit" variant="primary" disabled={!draft.trim() || edit.isPending}>
              {t("queue.save")}
            </Button>
            <Button small variant="ghost" onClick={cancel}>
              {t("common.cancel")}
            </Button>
          </div>
        </form>
      </li>
    );
  }

  return (
    <li className={`queue-pill${message.held ? " queue-pill--held" : ""}`}>
      <span className={message.held ? "queue-pill__tag" : "sr-only"}>
        {message.held ? t("queue.badgeHeld") : t("queue.badge")}
      </span>
      <span className="queue-pill__text" title={message.text}>
        {message.text}
      </span>
      <span className="queue-pill__actions">
        <button
          type="button"
          className="queue-pill__action"
          aria-label={t("queue.editLabel")}
          title={t("queue.edit")}
          onClick={() => {
            setDraft(message.text);
            setEditing(true);
          }}
        >
          <PencilIcon />
        </button>
        <button
          type="button"
          className="queue-pill__action"
          aria-label={t("queue.removeLabel")}
          title={t("queue.remove")}
          disabled={remove.isPending}
          onClick={() => remove.mutate(undefined)}
        >
          <CloseIcon />
        </button>
      </span>
    </li>
  );
}
