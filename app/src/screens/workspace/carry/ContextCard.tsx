import { useId, useState } from "react";

import { useAction } from "../../../api/context";
import type { Account, CarryInput, ContextTurn } from "../../../api/types";
import { Button } from "../../../components/ui/Button";
import { ChevronIcon } from "../../../components/ui/icons";
import { useLanguage, useT } from "../../../i18n";
import { CloseIcon } from "../icons";
import { useConversationContext } from "./queries";
import "./carry.css";

/** What of the conversation goes with the new task: the kept turns, or a summary. */
export interface CarryDraft {
  conversation: string;
  title: string;
  turns: ContextTurn[];
  removed: Set<number>;
  summary: string | null;
  loading: boolean;
  busy: boolean;
  longAfter: number;
  /** Nothing left to carry (every turn removed and no summary). */
  empty: boolean;
  /** What `startTask` takes. */
  input: CarryInput;
  toggle: (index: number) => void;
  setSummary: (summary: string | null) => void;
}

/**
 * The context a new task carries from conversation `conversation`: every
 * question with its final answer at first; the user removes some or
 * replaces them with a summary.
 */
export function useCarryDraft(conversation: string | undefined): CarryDraft | null {
  const context = useConversationContext(conversation);
  // Choices belong to one conversation: another one starts over.
  const [state, setState] = useState<{ for: string | undefined; removed: Set<number>; summary: string | null }>({
    for: conversation,
    removed: new Set(),
    summary: null,
  });
  if (state.for !== conversation) setState({ for: conversation, removed: new Set(), summary: null });
  if (!conversation) return null;
  const turns = context.data?.turns ?? [];
  const kept = turns.map((_, i) => i).filter((i) => !state.removed.has(i));
  const summary = state.summary;
  return {
    conversation,
    title: context.data?.title ?? "",
    turns,
    removed: state.removed,
    summary,
    loading: context.isPending,
    busy: context.data?.busy ?? false,
    longAfter: context.data?.longAfter ?? Infinity,
    empty: !context.isPending && kept.length === 0 && !summary?.trim(),
    input: { conversation, turns: kept, summary: summary?.trim() ? summary : null },
    toggle: (index) =>
      setState((s) => {
        const removed = new Set(s.removed);
        if (removed.has(index)) removed.delete(index);
        else removed.add(index);
        return { ...s, removed };
      }),
    setSummary: (next) => setState((s) => ({ ...s, summary: next })),
  };
}

/** Characters the task's context will take. */
function draftChars(draft: CarryDraft): number {
  if (draft.summary !== null) return draft.summary.length;
  return draft.turns.reduce((n, turn, i) => (draft.removed.has(i) ? n : n + turn.question.length + turn.answer.length), 0);
}

/**
 * "Contexto: conversación «…»" above the composer of a new task: what goes
 * before the instruction (never pasted into the text). It opens to preview
 * each question and answer, drop some, or swap them for an AI summary.
 */
export function ContextCard({
  draft,
  account,
  onRemove,
}: {
  draft: CarryDraft;
  /** The conversation's account, which writes the summary. */
  account: Account | undefined;
  /** Drops the whole context (a plain new task). */
  onRemove: () => void;
}) {
  const t = useT();
  const language = useLanguage();
  const bodyId = useId();
  const [open, setOpen] = useState(false);
  const chars = draftChars(draft);
  const long = draft.summary === null && chars > draft.longAfter;
  const number = (n: number) => n.toLocaleString(language === "es" ? "es-ES" : "en-US");
  const keptCount = draft.turns.length - draft.removed.size;

  const summarize = useAction((api, turns: number[]) => api.summarizeConversation(draft.conversation, turns), {
    invalidate: [],
    onSuccess: (text) => {
      draft.setSummary(text);
      setOpen(true);
    },
  });
  const summarizeButton = (
    <Button
      small
      variant="ghost"
      disabled={summarize.isPending || draft.busy || keptCount === 0}
      title={t("carry.summarizeHint", { account: account?.name ?? "" })}
      onClick={() => summarize.mutate(draft.input.turns)}
    >
      {summarize.isPending ? t("carry.summarizing") : t("carry.summarize")}
    </Button>
  );

  return (
    <section className="carry-card" aria-label={t("carry.contextTitle", { title: draft.title })}>
      <div className="carry-card__head">
        <button
          type="button"
          className="carry-card__toggle"
          aria-expanded={open}
          aria-controls={bodyId}
          title={open ? t("carry.collapse") : t("carry.expand")}
          onClick={() => setOpen((o) => !o)}
        >
          <span className={open ? "carry-card__chevron carry-card__chevron--open" : "carry-card__chevron"} aria-hidden>
            <ChevronIcon />
          </span>
          <span className="carry-card__title">
            {draft.loading ? t("carry.loading") : t("carry.contextTitle", { title: draft.title })}
          </span>{" "}
          {!draft.loading && (
            <span className="carry-card__meta">
              {draft.summary !== null
                ? t("carry.contextSummarized", { chars: number(chars) })
                : t("carry.contextCount", { kept: keptCount, total: draft.turns.length, chars: number(chars) })}
            </span>
          )}
        </button>
        <button
          type="button"
          className="carry-card__icon"
          aria-label={t("carry.remove")}
          title={t("carry.remove")}
          onClick={onRemove}
        >
          <CloseIcon />
        </button>
      </div>

      {draft.busy && <p className="carry-card__note carry-card__note--warn">{t("carry.busyCard")}</p>}
      {draft.empty && (
        <p className="carry-card__note carry-card__note--warn" role="status">
          {t("carry.empty")}
        </p>
      )}
      {long && !open && (
        <p className="carry-card__note">
          {t("carry.long")} {summarizeButton}
        </p>
      )}

      <div id={bodyId} className="carry-card__body" hidden={!open}>
        {draft.summary !== null ? (
          <div className="carry-card__summary">
            <label className="field__label" htmlFor={`${bodyId}-summary`}>
              {t("carry.summaryLabel")}
            </label>
            <textarea
              id={`${bodyId}-summary`}
              rows={6}
              value={draft.summary}
              onChange={(e) => draft.setSummary(e.target.value)}
            />
            <div className="carry-card__actions">
              <Button small variant="ghost" onClick={() => draft.setSummary(null)}>
                {t("carry.useTurns")}
              </Button>
            </div>
          </div>
        ) : (
          <>
            <ol className="carry-card__turns">
              {draft.turns.map((turn, index) => {
                const removed = draft.removed.has(index);
                const n = index + 1;
                return (
                  <li key={index} className={removed ? "carry-turn carry-turn--removed" : "carry-turn"}>
                    <div className="carry-turn__text">
                      <p className="carry-turn__label">{t("carry.question", { n })}</p>
                      <p className="carry-turn__question">{turn.question}</p>
                      {turn.images.length > 0 && (
                        <p className="carry-turn__images">
                          {t("carry.images", { names: turn.images.map((i) => i.name).join(", ") })}
                        </p>
                      )}
                      {!removed && (
                        <>
                          <p className="carry-turn__label">{t("carry.answer")}</p>
                          <p className="carry-turn__answer">{turn.answer}</p>
                        </>
                      )}
                    </div>
                    {removed ? (
                      <Button
                        small
                        variant="ghost"
                        aria-label={t("carry.restoreTurn", { n })}
                        onClick={() => draft.toggle(index)}
                      >
                        {t("carry.include")}
                      </Button>
                    ) : (
                      <button
                        type="button"
                        className="carry-card__icon"
                        aria-label={t("carry.removeTurn", { n })}
                        title={t("carry.removeTurn", { n })}
                        onClick={() => draft.toggle(index)}
                      >
                        <CloseIcon />
                      </button>
                    )}
                  </li>
                );
              })}
            </ol>
            <div className="carry-card__actions">
              {long && open && <span className="carry-card__hint">{t("carry.long")}</span>}
              {summarizeButton}
            </div>
          </>
        )}
      </div>
    </section>
  );
}
