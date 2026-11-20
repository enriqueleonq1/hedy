// "Pregunta a tu memoria" (docs/decisions.md §36), inside Memoria: a question
// answered only from the notes, each claim citing its own. A cited note opens
// right there, with the way to its project; an unanswered question offers to
// look in the code or to write the note.

import { useEffect, useMemo, useRef, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";

import { errorMessage, useApi } from "../../api/context";
import type { AskScope, CodeLookup, MemoryAnswer, MemorySource, Overview, SourceRef } from "../../api/types";
import { Button, ConfirmButton } from "../../components/ui/Button";
import { timeAgo } from "../../format";
import { useT, type Translate } from "../../i18n";
import { answerParts, revealHidden } from "./citations";
import "../../styles/ask.css";

export const MEMORY_GRAPH_KEY = ["memoryGraph"];

/** A question to ask; a new `seq` asks again (the same words too). */
export interface AskRequest {
  text: string;
  scope: AskScope;
  seq: number;
}

/** Where a source lives, as its line under it says. */
function place(source: MemorySource, t: Translate): string {
  const team = source.team ?? "";
  if (source.layer === "team") return t("ask.place.team", { team });
  if (source.layer === "project") {
    return source.project
      ? t("ask.place.teamProject", { team, project: source.project })
      : t("ask.place.otherProject", { team });
  }
  return source.project ? t("ask.place.mineProject", { project: source.project }) : t("ask.place.mine");
}

/** Longest note text shown in a source (the full note opens in its project). */
const SOURCE_CHARS = 600;

/** The answer's text with each citation the core read as a chip that goes to its note. */
function AnswerText({ text, cited, onCite }: { text: string; cited: number[]; onCite: (n: number) => void }) {
  const t = useT();
  return (
    <p className="ask-answer__text">
      {answerParts(text, cited).map((part, i) =>
        "text" in part
          ? part.text
          : part.cite.map((n) => (
              <button key={`${i}-${n}`} type="button" className="ask-cite" aria-label={t("ask.cite", { n })} onClick={() => onCite(n)}>
                {n}
              </button>
            )),
      )}
    </p>
  );
}

export function AskPanel({
  overview,
  request,
  onCited,
  onOpenProject,
  onWrite,
  onBusy,
  onClose,
}: {
  overview: Overview;
  request: AskRequest;
  /** The cited notes, for the map to light them. */
  onCited: (cited: { ref: SourceRef; n: number }[], asked: number) => void;
  /** "Abrir en su proyecto": that project's notes in Memoria. */
  onOpenProject: (project: string) => void;
  /** "Anotarlo": a new note in that project, with the question written. */
  onWrite: (project: string, question: string) => void;
  /** While a question is being answered (no other one meanwhile). */
  onBusy: (busy: boolean) => void;
  onClose: () => void;
}) {
  const api = useApi();
  const t = useT();
  const client = useQueryClient();
  const [answer, setAnswer] = useState<{ result: MemoryAnswer; asked: number; question: string; scope: AskScope } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [found, setFound] = useState<CodeLookup | null>(null);
  const [decided, setDecided] = useState<"approved" | "discarded" | null>(null);
  const [opened, setOpened] = useState<number | null>(null);
  /** The answer on screen, to drop a lookup that comes back after a newer question. */
  const askedRef = useRef(0);
  const sources = useRef(new Map<number, HTMLLIElement>());

  const ask = useMutation({
    mutationFn: ({ text, scope }: AskRequest) => api.askMemory(text, scope),
    onMutate: () => {
      // From here on, a code lookup for the earlier answer is out of date.
      askedRef.current += 1;
      setError(null);
      setFound(null);
      setDecided(null);
      setOpened(null);
    },
    onSuccess: (result, { text, scope }) => setAnswer({ result, asked: askedRef.current, question: text, scope }),
    onError: (e) => {
      setAnswer(null);
      setError(errorMessage(e, t));
    },
  });
  const { mutate, isPending } = ask;
  // Each request once, even when an effect runs twice (StrictMode in development).
  const sent = useRef<number | null>(null);
  useEffect(() => {
    if (sent.current === request.seq) return;
    sent.current = request.seq;
    mutate(request);
  }, [request, mutate]);
  useEffect(() => onBusy(isPending), [isPending, onBusy]);
  const [removed, setRemoved] = useState<Set<number>>(() => new Set());

  const look = useMutation({
    mutationFn: ({ text, project }: { text: string; project: string; asked: number }) => api.lookInCode(text, project),
    onMutate: () => setError(null),
    onSuccess: async (lookup, { project, asked }) => {
      // Its proposal is in the project's memory now.
      await client.invalidateQueries({ queryKey: ["memory", project] });
      // A lookup for an earlier question does not land under a newer one.
      if (asked !== askedRef.current) return;
      setFound(lookup);
      setDecided(null);
    },
    onError: (e) => setError(errorMessage(e, t)),
  });
  const decide = useMutation({
    mutationFn: async ({ id, keep }: { id: number; keep: boolean }) => {
      if (keep) await api.approveNote(id);
      else await api.forgetNote(id);
      return keep;
    },
    onSuccess: async (keep) => {
      setDecided(keep ? "approved" : "discarded");
      await client.invalidateQueries({ queryKey: MEMORY_GRAPH_KEY });
      if (scopeProject) await client.invalidateQueries({ queryKey: ["memory", scopeProject] });
    },
    onError: (e) => setError(errorMessage(e, t)),
  });
  const forget = useMutation({
    mutationFn: (id: number) => api.forgetNote(id),
    onSuccess: async (_, id) => {
      setRemoved((done) => new Set(done).add(id));
      await client.invalidateQueries({ queryKey: MEMORY_GRAPH_KEY });
      await client.invalidateQueries({ queryKey: ["memory"] });
    },
    onError: (e) => setError(errorMessage(e, t)),
  });

  const result = answer?.result;
  // The cited notes with their number, for the map's tags.
  const cited = useMemo(
    () => (result ? result.cited.flatMap((n) => (result.sources[n - 1] ? [{ ref: (result.sources[n - 1] as MemorySource).note, n }] : [])) : []),
    [result],
  );
  const asked = answer?.asked ?? 0;
  useEffect(() => onCited(cited, asked), [cited, asked, onCited]);

  const showSource = (n: number) => {
    setOpened(n);
    sources.current.get(n)?.scrollIntoView?.({ block: "nearest" });
  };
  // The project the answer was asked about (not what "Para" says now).
  const scopeProject = answer?.scope.kind === "project" ? answer.scope.project : null;
  // Looking in the code needs a Claude Code account (the core refuses others).
  const lookAccount =
    overview.projects.find((p) => p.path === scopeProject)?.account ??
    overview.defaultAccount ??
    (overview.accounts.length === 1 ? overview.accounts[0]?.id : null);
  const canLook = overview.accounts.find((a) => a.id === lookAccount)?.provider === "claude";
  // A source's project on this computer, by its folder name.
  const projectOf = (source: MemorySource) =>
    source.project ? overview.projects.find((p) => p.path.split("/").pop() === source.project)?.path : undefined;

  return (
    <section className="ask-panel" aria-label={t("ask.title")}>
      <div className="ask-panel__head">
        <span className="ask__label">{t("ask.title")}</span>
        <button type="button" className="ask-node__close" onClick={onClose}>
          {t("ask.node.close")}
        </button>
      </div>
      {error && (
        <p className="error-text" role="alert">
          {error}
        </p>
      )}
      <div className="ask__body" aria-live="polite">
        {ask.isPending && <p className="ask__thinking">{t("ask.thinking")}</p>}
        {!ask.isPending && result && result.text === null && (
          <div className="ask-answer ask-answer--none">
            <p className="ask-answer__text">{t("ask.notWritten")}</p>
            <p className="small muted">{t("ask.notWrittenHint")}</p>
            {scopeProject && !found && (
              <div className="row">
                {canLook && (
                  <Button
                    small
                    variant="primary"
                    disabled={look.isPending}
                    onClick={() => answer && look.mutate({ text: answer.question, project: scopeProject, asked: answer.asked })}
                  >
                    {t("ask.code.look")}
                  </Button>
                )}
                <Button small onClick={() => answer && onWrite(scopeProject, answer.question)}>
                  {t("ask.write")}
                </Button>
              </div>
            )}
            {look.isPending && <p className="ask__thinking">{t("ask.code.reading")}</p>}
          </div>
        )}
        {found && !ask.isPending && (
          <div className="ask-answer ask-code">
            <span className="ask__label">{t("ask.code.title")}</span>
            <p className="ask-answer__text">{found.text ?? t("ask.code.none")}</p>
            {found.proposal && (
              <div className="ask-code__proposal">
                <span className="small muted">{t("ask.code.proposal", { account: found.account })}</span>
                <blockquote>«{revealHidden(found.proposal.text)}»</blockquote>
                {decided === null ? (
                  <div className="row">
                    <Button
                      small
                      variant="primary"
                      disabled={decide.isPending}
                      onClick={() => found.proposal && decide.mutate({ id: found.proposal.id, keep: true })}
                    >
                      {t("ask.code.approve")}
                    </Button>
                    <Button
                      small
                      variant="ghost"
                      disabled={decide.isPending}
                      onClick={() => found.proposal && decide.mutate({ id: found.proposal.id, keep: false })}
                    >
                      {t("ask.code.discard")}
                    </Button>
                  </div>
                ) : (
                  <p className="small muted" role="status">
                    {t(decided === "approved" ? "ask.code.approved" : "ask.code.discarded")}
                  </p>
                )}
              </div>
            )}
          </div>
        )}
        {!ask.isPending && result?.text && (
          <div className="ask-answer">
            <AnswerText text={result.text} cited={result.cited} onCite={showSource} />
            {cited.length > 0 && (
              <>
                <h2 className="ask__label">{t("ask.sources")}</h2>
                <ol className="ask-sources">
                  {result.cited.map((n) => {
                    const source = result.sources[n - 1];
                    if (!source) return null;
                    const home = projectOf(source);
                    const local = source.note.kind === "local" && !removed.has(source.note.id) ? source.note.id : null;
                    const open = opened === n;
                    return (
                      <li
                        key={n}
                        ref={(el) => {
                          if (el) sources.current.set(n, el);
                          else sources.current.delete(n);
                        }}
                        className={`ask-source ask-source--${source.layer}${open ? " ask-source--open" : ""}`}
                      >
                        <button
                          type="button"
                          className="ask-source__n"
                          aria-label={t("ask.cite", { n })}
                          aria-expanded={open}
                          onClick={() => setOpened(open ? null : n)}
                        >
                          {n}
                        </button>
                        <span className="ask-source__body">
                          <span className="ask-source__text">
                            {revealHidden(!open && source.text.length > SOURCE_CHARS ? `${source.text.slice(0, SOURCE_CHARS)}…` : source.text)}
                          </span>
                          <span className="ask-source__meta">
                            {[place(source, t), source.author && t("ask.by", { author: source.author }), timeAgo(source.at, t)]
                              .filter(Boolean)
                              .join(" · ")}
                          </span>
                          {open && (home || local !== null) && (
                            <span className="ask-source__actions">
                              {home && (
                                <Button small onClick={() => onOpenProject(home)}>
                                  {t("ask.openProject")}
                                </Button>
                              )}
                              {local !== null && (
                                <ConfirmButton small confirmLabel={t("common.forgetConfirm")} onConfirm={() => forget.mutate(local)}>
                                  {t("common.forget")}
                                </ConfirmButton>
                              )}
                            </span>
                          )}
                        </span>
                      </li>
                    );
                  })}
                </ol>
              </>
            )}
          </div>
        )}
        {!ask.isPending && result?.account && (
          <p className="ask__reader">
            {result.sources.length === 1
              ? t("ask.readOne", { account: result.account })
              : t("ask.readMany", { account: result.account, n: result.sources.length })}
          </p>
        )}
      </div>
    </section>
  );
}
