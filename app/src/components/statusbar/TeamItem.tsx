// "Equipo · N para revisar" in the status bar (docs/decisions.md §35): only
// while a team has suggestions this person may take; it opens Ajustes →
// Equipos.

import { useQueries } from "@tanstack/react-query";

import { useApi } from "../../api/context";
import { useT } from "../../i18n";
import { teamSuggestionsKey, useTeams } from "../../team/hooks";

export function TeamItem({ onOpen }: { onOpen: () => void }) {
  const api = useApi();
  const t = useT();
  const teams = (useTeams().data ?? []).filter((team) => !team.waiting);
  const suggestions = useQueries({
    queries: teams.map((team) => ({
      queryKey: teamSuggestionsKey(team.id),
      queryFn: () => api.teamSuggestions(team.id),
    })),
  });
  const n = suggestions.reduce((sum, q) => sum + (q.data ?? []).filter((s) => s.canReview).length, 0);
  if (n === 0) return null;
  const text = t("team.bar.review", { n });
  return (
    <button type="button" className="statusbar__item sync-item__button sync-item__button--review" onClick={onOpen} aria-label={text}>
      <span className="statusbar__dot sync-item__dot" aria-hidden />
      <span className="statusbar__text" aria-hidden>
        {text}
      </span>
    </button>
  );
}
