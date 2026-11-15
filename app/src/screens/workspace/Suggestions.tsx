// Three quiet suggestions under the mascot of an empty task. Picking one
// fills the composer (it never sends): in a project they come from what Hedy
// already knows locally (the last test run, uncommitted changes); in a quick
// question they are starters with the caret where the user types.

import { useQuery } from "@tanstack/react-query";

import { useApi } from "../../api/context";
import type { ProjectStatus, ProviderId, TaskMode } from "../../api/types";
import { useT, type Translate } from "../../i18n";

/** What a suggestion puts in the composer. */
export interface Suggestion {
  label: string;
  text: string;
  /** Where the caret goes in `text`. */
  caret: number;
  /** The mode it needs (a review reads the main folder: it only asks). */
  mode?: TaskMode;
}

const CARET = "{caret}";

/** A starter text: the caret goes where `{caret}` was (else at the end). */
export function starter(label: string, text: string, mode?: TaskMode): Suggestion {
  const at = text.indexOf(CARET);
  const clean = at < 0 ? text : text.slice(0, at) + text.slice(at + CARET.length);
  return { label, text: clean, caret: at < 0 ? clean.length : at, ...(mode ? { mode } : {}) };
}

/**
 * The three most relevant for a project: failing tests first, then
 * uncommitted changes (only for agents that work in the main folder when
 * they only ask: the others work in a worktree, where those changes are not),
 * then always the tour of the repo, then tests where there are none and a
 * look for dead code.
 */
export function projectSuggestions(status: ProjectStatus | undefined, provider: ProviderId | undefined, t: Translate): Suggestion[] {
  const all: Suggestion[] = [];
  if (status?.lastTests && !status.lastTests.passed) {
    all.push(
      starter(
        t("composer.suggest.fixTests"),
        t("composer.suggest.fixTestsText", { command: status.lastTests.command }),
        "project",
      ),
    );
  }
  if ((status?.uncommitted ?? 0) > 0 && provider === "claude") {
    all.push(starter(t("composer.suggest.review"), t("composer.suggest.reviewText"), "ask"));
  }
  all.push(starter(t("composer.suggest.explain"), t("composer.suggest.explainText"), "ask"));
  all.push(starter(t("composer.suggest.tests"), t("composer.suggest.testsText"), "project"));
  all.push(starter(t("composer.suggest.cleanup"), t("composer.suggest.cleanupText"), "ask"));
  return all.slice(0, 3);
}

export function questionSuggestions(t: Translate): Suggestion[] {
  return [
    starter(t("composer.suggest.concept"), t("composer.suggest.conceptText")),
    starter(t("composer.suggest.compare"), t("composer.suggest.compareText")),
    starter(t("composer.suggest.draft"), t("composer.suggest.draftText")),
  ];
}

/** The suggestions of a new task in `project` (`null`: a quick question). */
export function useSuggestions(project: string | null, provider: ProviderId | undefined): Suggestion[] {
  const t = useT();
  const api = useApi();
  const status = useQuery({
    queryKey: ["status", project],
    queryFn: () => api.projectStatus(project ?? ""),
    enabled: project !== null,
    staleTime: 30_000,
  });
  if (project === null) return questionSuggestions(t);
  // Until the status arrives, nothing: the chips would change under the pointer.
  if (status.isPending) return [];
  return projectSuggestions(status.data, provider, t);
}

export function Suggestions({ items, onPick }: { items: Suggestion[]; onPick: (suggestion: Suggestion) => void }) {
  const t = useT();
  if (items.length === 0) return null;
  return (
    <ul className="suggestions" aria-label={t("composer.suggest.label")}>
      {items.map((item) => (
        <li key={item.label}>
          <button type="button" className="suggestion" onClick={() => onPick(item)}>
            {item.label}
          </button>
        </li>
      ))}
    </ul>
  );
}
