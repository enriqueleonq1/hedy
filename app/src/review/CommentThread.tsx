import { useEffect, useId, useRef, useState } from "react";

import { useAction } from "../api/context";
import { Markdown } from "../components/Markdown";
import { useToast } from "../components/ui/Toast";
import { modKey } from "../components/palette/keys";
import { useT } from "../i18n";
import { lineRef, useReviewComments, type ReviewComment } from "./comments";
import { CheckIcon, PencilIcon, RuleIcon, SendIcon, TrashIcon } from "./icons";

/**
 * Writes (or edits) a comment under the lines it is about: Markdown text,
 * Ctrl/⌘+Enter or the button adds it, Escape cancels.
 */
export function CommentComposer({
  where,
  initial = "",
  submitLabel,
  onSubmit,
  onCancel,
}: {
  /** `path:16-18`, for the field's name. */
  where: string;
  initial?: string;
  submitLabel?: string;
  onSubmit: (text: string) => void;
  onCancel: () => void;
}) {
  const t = useT();
  const [text, setText] = useState(initial);
  const field = useRef<HTMLTextAreaElement>(null);
  const hint = useId();
  // After a menu closes and gives its focus back (an effect, so it runs after that).
  useEffect(() => {
    const el = field.current;
    if (!el) return;
    el.focus();
    el.setSelectionRange(el.value.length, el.value.length);
  }, []);
  const ready = text.trim().length > 0;

  return (
    <form
      className="rv-composer"
      onSubmit={(e) => {
        e.preventDefault();
        if (ready) onSubmit(text);
      }}
    >
      <textarea
        ref={field}
        className="rv-composer__field"
        rows={3}
        value={text}
        aria-label={t("reviewer.composer.label", { where })}
        aria-describedby={hint}
        placeholder={t("reviewer.composer.placeholder")}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
            e.preventDefault();
            if (ready) onSubmit(text);
          } else if (e.key === "Escape") {
            e.preventDefault();
            e.stopPropagation();
            onCancel();
          }
        }}
      />
      <div className="rv-composer__actions">
        <span id={hint} className="rv-composer__hint">
          {t("reviewer.composer.hint", { mod: modKey() })}
        </span>
        <button type="button" className="btn btn--ghost btn--small" onClick={onCancel}>
          {t("common.cancel")}
        </button>
        <button type="submit" className="btn btn--primary btn--small" disabled={!ready}>
          {submitLabel ?? t("reviewer.composer.add")}
        </button>
      </div>
    </form>
  );
}

/** The comments anchored at one line: pending ones in full, sent ones dimmed and folded. */
export function CommentThread({ comments, showWhere = false }: { comments: ReviewComment[]; showWhere?: boolean }) {
  return (
    <ul className="rv-thread">
      {comments.map((c) => (
        <CommentItem key={c.id} comment={c} showWhere={showWhere} />
      ))}
    </ul>
  );
}

function CommentItem({ comment, showWhere }: { comment: ReviewComment; showWhere: boolean }) {
  const t = useT();
  const review = useReviewComments();
  const toast = useToast();
  const [editing, setEditing] = useState(false);
  const [ruling, setRuling] = useState(false);
  const [open, setOpen] = useState(false);
  // A comment the user would write again becomes a rule of the project's
  // memory: written by the user, so it needs no approval (rule 6).
  const saveRule = useAction((api, text: string) => api.addNote(review.project, text, "rule"), {
    onSuccess: () => {
      setRuling(false);
      toast.info(t("reviewer.comment.ruleSaved"));
    },
  });
  const sent = comment.sentAt !== null;
  const range = comment.endLine !== null;
  const where = showWhere || range ? lineRef(comment) : null;

  if (editing) {
    return (
      <li className="rv-comment rv-comment--editing">
        <CommentComposer
          where={lineRef(comment)}
          initial={comment.text}
          submitLabel={t("reviewer.composer.save")}
          onCancel={() => setEditing(false)}
          onSubmit={(text) => {
            review.update(comment.id, text);
            setEditing(false);
          }}
        />
      </li>
    );
  }

  if (ruling) {
    return (
      <li className="rv-comment rv-comment--editing">
        <CommentComposer
          where={lineRef(comment)}
          initial={comment.text}
          submitLabel={t("reviewer.comment.saveRule")}
          onCancel={() => setRuling(false)}
          onSubmit={(text) => {
            if (!saveRule.isPending) saveRule.mutate(text.trim());
          }}
        />
      </li>
    );
  }
  if (sent && !open) {
    return (
      <li className="rv-comment rv-comment--sent">
        <button type="button" className="rv-comment__fold" aria-expanded={false} onClick={() => setOpen(true)}>
          <span className="rv-comment__sent">
            <CheckIcon />
            {t("reviewer.comment.sent")}
          </span>
          <span className="rv-comment__summary">{comment.text.split("\n")[0]}</span>
        </button>
      </li>
    );
  }

  return (
    <li className={`rv-comment${sent ? " rv-comment--sent rv-comment--open" : ""}`}>
      <div className="rv-comment__head">
        <span className="rv-comment__author">{t("reviewer.comment.you")}</span>
        {where && <span className="rv-comment__where mono">{where}</span>}
        {sent && (
          <button type="button" className="rv-comment__sent rv-comment__sent--toggle" aria-expanded onClick={() => setOpen(false)}>
            <CheckIcon />
            {t("reviewer.comment.sent")}
          </button>
        )}
        <span className="rv-comment__actions">
          {!sent && review.canSend && (
            <button
              type="button"
              className="rv-icon-btn"
              aria-label={t("reviewer.comment.sendOne")}
              title={t("reviewer.comment.sendOne")}
              disabled={review.sending}
              onClick={() => void review.sendToAi([comment.id])}
            >
              <SendIcon />
            </button>
          )}
          {!sent && (
            <button
              type="button"
              className="rv-icon-btn"
              aria-label={t("reviewer.comment.edit")}
              title={t("reviewer.comment.edit")}
              onClick={() => setEditing(true)}
            >
              <PencilIcon />
            </button>
          )}
          {review.project && (
            <button
              type="button"
              className="rv-icon-btn"
              aria-label={t("reviewer.comment.asRule")}
              title={t("reviewer.comment.asRule")}
              onClick={() => setRuling(true)}
            >
              <RuleIcon />
            </button>
          )}
          <button
            type="button"
            className="rv-icon-btn rv-icon-btn--danger"
            aria-label={t("reviewer.comment.delete")}
            title={t("reviewer.comment.delete")}
            onClick={() => review.remove(comment.id)}
          >
            <TrashIcon />
          </button>
        </span>
      </div>
      <Markdown text={comment.text} className="rv-comment__body" />
    </li>
  );
}
