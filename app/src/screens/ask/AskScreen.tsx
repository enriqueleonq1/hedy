// "Pregunta a tu memoria" (docs/decisions.md §36): a question answered only
// from the notes, each claim citing its own, beside the memory's graph,
// which lights the notes that answered.

import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { errorMessage, useApi } from "../../api/context";
import type { AskScope, CodeLookup, GraphNode, MemoryAnswer, MemorySource, Overview } from "../../api/types";
import { Button } from "../../components/ui/Button";
import { timeAgo } from "../../format";
import { useT, type Translate } from "../../i18n";
import { useTeams } from "../../team/hooks";
import { answerParts, revealHidden } from "./citations";
import { MemoryGraphView } from "./MemoryGraphView";
import "../../styles/ask.css";

export const MEMORY_GRAPH_KEY = ["memoryGraph"];

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

/** Longest note text shown in a source (the full note opens in its memory). */
const SOURCE_CHARS = 600;

/** The answer's text with each citation the core read as a chip that shows its note. */
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

export function AskScreen({
  overview,
  project,
  onOpenMemory,
}: {
  overview: Overview;
  /** Asked from a project: its memory by default. */
  project?: string;
  onOpenMemory: (project: string) => void;
}) {
  const api = useApi();
  const t = useT();
  const teamList = useTeams().data;
  const teams = useMemo(() => teamList ?? [], [teamList]);
  const graph = useQuery({ queryKey: MEMORY_GRAPH_KEY, queryFn: () => api.memoryGraph() });
  const [chosen, setChosen] = useState<string>(project ? `project:${project}` : "mine");
  const [question, setQuestion] = useState("");
  /** The answer, with the question and scope it was asked with. */
  const [answer, setAnswer] = useState<{ result: MemoryAnswer; asked: number; question: string; scope: string } | null>(null);
  const [selected, setSelected] = useState<GraphNode | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [found, setFound] = useState<CodeLookup | null>(null);
  const [decided, setDecided] = useState<"approved" | "discarded" | null>(null);
  const client = useQueryClient();

  const scopes = useMemo(
    () => [
      { value: "mine", label: t("ask.scope.mine") },
      ...overview.projects.map((p) => ({ value: `project:${p.path}`, label: t("ask.scope.project", { name: p.name }) })),
      ...teams.filter((team) => !team.waiting).map((team) => ({ value: `team:${team.id}`, label: t("ask.scope.team", { name: team.name }) })),
    ],
    [overview.projects, teams, t],
  );
  // A project not on the list (removed, or an old link): all of mine, as shown.
  const scope = scopes.some((s) => s.value === chosen) ? chosen : "mine";
  const toScope = (value: string): AskScope =>
    value.startsWith("project:")
      ? { kind: "project", project: value.slice(8) }
      : value.startsWith("team:")
        ? { kind: "team", team: value.slice(5) }
        : { kind: "mine" };

  const ask = useMutation({
    mutationFn: ({ text, where }: { text: string; where: string }) => api.askMemory(text, toScope(where)),
    onMutate: () => {
      setError(null);
      setFound(null);
      setDecided(null);
    },
    onSuccess: (result, { text, where }) => {
      setSelected(null);
      setAnswer((previous) => ({ result, asked: (previous?.asked ?? 0) + 1, question: text, scope: where }));
    },
    onError: (e) => {
      setAnswer(null);
      setError(errorMessage(e, t));
    },
  });

  const look = useMutation({
    mutationFn: ({ text, project }: { text: string; project: string }) => api.lookInCode(text, project),
    onMutate: () => setError(null),
    onSuccess: (lookup) => {
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

  const result = answer?.result;
  // The cited notes with their number, for the graph's tags.
  const cited = useMemo(
    () => (result ? result.cited.flatMap((n) => (result.sources[n - 1] ? [{ ref: (result.sources[n - 1] as MemorySource).note, n }] : [])) : []),
    [result],
  );
  const showSource = (n: number) => {
    const source = result?.sources[n - 1];
    const node = source && graph.data?.nodes.find((g) => g.source && JSON.stringify(g.source) === JSON.stringify(source.note));
    if (node) setSelected(node);
  };
  // The project the answer was asked about (not what the select says now).
  const scopeProject = answer?.scope.startsWith("project:") ? answer.scope.slice(8) : null;

  return (
    <div className="ask">
      <section className="ask__panel" aria-labelledby="ask-title">
        <header className="ask__head">
          <span className="ask__eyebrow">{t("ask.eyebrow")}</span>
          <h1 id="ask-title">{t("ask.title")}</h1>
          <p className="ask__subtitle">{t("ask.subtitle")}</p>
        </header>

        <form
          className="ask-form"
          onSubmit={(e) => {
            e.preventDefault();
            if (question.trim() && !ask.isPending) ask.mutate({ text: question.trim(), where: scope });
          }}
        >
          <label className="ask-form__scope">
            <span>{t("ask.scope")}</span>
            <select value={scope} onChange={(e) => setChosen(e.target.value)}>
              {scopes.map((s) => (
                <option key={s.value} value={s.value}>
                  {s.label}
                </option>
              ))}
            </select>
          </label>
          <div className="ask-form__row">
            <label htmlFor="ask-question" className="sr-only">
              {t("ask.question")}
            </label>
            <input
              id="ask-question"
              value={question}
              placeholder={t("ask.placeholder")}
              maxLength={1000}
              onChange={(e) => setQuestion(e.target.value)}
            />
            <Button variant="primary" type="submit" disabled={!question.trim() || ask.isPending}>
              {t("ask.submit")}
            </Button>
          </div>
        </form>

        {error && (
          <p className="error-text" role="alert">
            {error}
          </p>
        )}
        <div className="ask__body" aria-live="polite">
          {ask.isPending && <p className="ask__thinking">{t("ask.thinking")}</p>}
          {!ask.isPending && !result && !error && <p className="ask__empty">{t("ask.empty")}</p>}
          {!ask.isPending && result && result.text === null && (
            <div className="ask-answer ask-answer--none">
              <p className="ask-answer__text">{t("ask.notWritten")}</p>
              <p className="small muted">{t("ask.notWrittenHint")}</p>
              {scopeProject && !found && (
                <div className="row">
                  <Button
                    small
                    variant="primary"
                    disabled={look.isPending}
                    onClick={() => answer && look.mutate({ text: answer.question, project: scopeProject })}
                  >
                    {t("ask.code.look")}
                  </Button>
                  <Button small onClick={() => onOpenMemory(scopeProject)}>
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
                      return (
                        <li key={n} className={`ask-source ask-source--${source.layer}`}>
                          <button type="button" className="ask-source__n" aria-label={t("ask.cite", { n })} onClick={() => showSource(n)}>
                            {n}
                          </button>
                          <span className="ask-source__body">
                            <span className="ask-source__text">
                              {revealHidden(source.text.length > SOURCE_CHARS ? `${source.text.slice(0, SOURCE_CHARS)}…` : source.text)}
                            </span>
                            <span className="ask-source__meta">
                              {[place(source, t), source.author && t("ask.by", { author: source.author }), timeAgo(source.at, t)]
                                .filter(Boolean)
                                .join(" · ")}
                            </span>
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
          {selected && (
            <aside className="ask-node" aria-label={t(`ask.node.${selected.kind}`)}>
              <div className="ask-node__head">
                <span className="ask__label">{t(`ask.node.${selected.kind}`)}</span>
                <button type="button" className="ask-node__close" onClick={() => setSelected(null)}>
                  {t("ask.node.close")}
                </button>
              </div>
              <p className="ask-node__text">{revealHidden(selected.label)}</p>
            </aside>
          )}
        </div>
      </section>

      {graph.data && graph.data.nodes.some((n) => n.kind === "note") ? (
        <MemoryGraphView graph={graph.data} cited={cited} answer={answer?.asked ?? 0} onSelect={setSelected} />
      ) : (
        <div className="ask-graph ask-graph--empty">
          <p role={graph.isError ? "alert" : undefined}>
            {graph.isLoading ? t("ask.thinking") : graph.isError ? errorMessage(graph.error, t) : t("ask.graph.empty")}
          </p>
        </div>
      )}
    </div>
  );
}
