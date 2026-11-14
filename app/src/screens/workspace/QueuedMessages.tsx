import { useState } from "react";

import { useAction } from "../../api/context";
import type { HoldReason, QueuedMessage, Task } from "../../api/types";
import { Button } from "../../components/ui/Button";
import { isBusy, useSetQueue } from "../../tasks/hooks";
import { useT, type MessageKey } from "../../i18n";

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
          <QueuedBubble key={message.id} message={message} />
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

function QueuedBubble({ message }: { message: QueuedMessage }) {
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

  return (
    <li className={`bubble bubble--queued${message.held ? " bubble--held" : ""}`}>
      <span className="bubble__tag">{message.held ? t("queue.badgeHeld") : t("queue.badge")}</span>
      {editing ? (
        <form
          className="bubble__edit"
          onSubmit={(e) => {
            e.preventDefault();
            if (draft.trim()) edit.mutate(draft);
          }}
        >
          <textarea
            aria-label={t("queue.editLabel")}
            value={draft}
            rows={2}
            autoFocus
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                e.preventDefault();
                setEditing(false);
                setDraft(message.text);
              }
              if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
                e.preventDefault();
                if (draft.trim()) edit.mutate(draft);
              }
            }}
          />
          <div className="actions">
            <Button small type="submit" variant="primary" disabled={!draft.trim() || edit.isPending}>
              {t("queue.save")}
            </Button>
            <Button
              small
              variant="ghost"
              onClick={() => {
                setEditing(false);
                setDraft(message.text);
              }}
            >
              {t("common.cancel")}
            </Button>
          </div>
        </form>
      ) : (
        <>
          <span className="bubble__text">{message.text}</span>
          <span className="bubble__actions">
            <Button
              small
              variant="ghost"
              aria-label={t("queue.editLabel")}
              onClick={() => {
                setDraft(message.text);
                setEditing(true);
              }}
            >
              {t("queue.edit")}
            </Button>
            <Button small variant="ghost" aria-label={t("queue.removeLabel")} disabled={remove.isPending} onClick={() => remove.mutate(undefined)}>
              {t("queue.remove")}
            </Button>
          </span>
        </>
      )}
    </li>
  );
}
