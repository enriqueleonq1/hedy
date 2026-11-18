// A project's side of personal sync (docs/decisions.md §34): the owner
// question on its status screen, and where its memory lives, in its memory
// screen.

import { useQuery } from "@tanstack/react-query";

import { useAction, useApi } from "../../api/context";
import type { MemoryHome, MemoryHomeKind } from "../../api/types";
import { Button } from "../../components/ui/Button";
import { Select } from "../../components/ui/Select";
import { useT, type Translate } from "../../i18n";
import { SYNC_KEY, memoryHomeKey, useSyncStatus } from "../../sync/hooks";
import { TEAMS_KEY, useTeams } from "../../team/hooks";
import "../../styles/sync.css";

/** The short owner name for a sentence: `empresa-org` of `github.com/empresa-org`. */
export function ownerName(owner: string): string {
  return owner.split("/").slice(1).join("/") || owner;
}

export function useMemoryHome(project: string) {
  const api = useApi();
  return useQuery({ queryKey: memoryHomeKey(project), queryFn: () => api.memoryHome(project) });
}

/** "¿La memoria de empresa-org es solo tuya?": asked once per owner, while the sync is on. */
export function OwnerBanner({ project }: { project: string }) {
  const t = useT();
  const sync = useSyncStatus().data;
  const home = useMemoryHome(project).data;
  const answer = useAction((api, mine: boolean) => api.answerOwner(home?.ask ?? "", mine), {
    invalidate: [SYNC_KEY],
  });
  if (!sync || !home?.ask) return null;
  const owner = ownerName(home.ask);
  return (
    <div className="owner-banner" role="status">
      <span className="owner-banner__icon" aria-hidden>
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
          <circle cx="9" cy="8" r="3.2" />
          <path d="M3 20a6 6 0 0 1 12 0" />
          <circle cx="17" cy="9" r="2.6" />
          <path d="M15.5 14.2A5 5 0 0 1 21 19" />
        </svg>
      </span>
      <div className="owner-banner__text">
        <strong>{t("sync.owner.question", { owner })}</strong>
        <span className="small muted">{t("sync.owner.hint")}</span>
      </div>
      <div className="owner-banner__actions">
        <Button small disabled={answer.isPending} onClick={() => answer.mutate(true)}>
          {t("sync.owner.mine")}
        </Button>
        <Button small variant="primary" disabled={answer.isPending} onClick={() => answer.mutate(false)}>
          {t("sync.owner.shared")}
        </Button>
      </div>
    </div>
  );
}

/** A team's: `team:<id>`. */
type Choice = "follow" | "personal" | "shared" | "local" | `team:${string}`;

function homeLabel(home: MemoryHomeKind | null, t: Translate): string {
  if (!home) return t("sync.home.undecided");
  if (home.kind === "personal") return t("sync.home.personal");
  if (home.kind === "shared") return t("sync.home.sharedNamed", { group: ownerName(home.group) });
  if (home.kind === "team") return t("sync.home.team");
  return t("sync.home.local");
}

export function homeChoice(home: MemoryHome): Choice {
  if (!home.chosen || !home.home) return "follow";
  return home.home.kind === "team" ? `team:${home.home.team}` : home.home.kind;
}

/** "Memoria de este proyecto": personal, shared with a group, this PC only, or as its owner says. */
export function MemoryHomeCard({ project }: { project: string }) {
  const t = useT();
  const sync = useSyncStatus();
  const home = useMemoryHome(project).data;
  const set = useAction((api, next: MemoryHomeKind | null) => api.setMemoryHome(project, next), {
    invalidate: [SYNC_KEY],
  });
  // A team's project: its team notes go to the team's repo (docs/decisions.md §35).
  const teams = (useTeams().data ?? []).filter((team) => !team.waiting);
  const link = useAction((api, team: string) => api.teamLinkProject(team, project), {
    invalidate: [SYNC_KEY, TEAMS_KEY, ["memory"]],
  });
  if (!home) return null;
  const group = home.group;
  const choose = (choice: Choice) => {
    if (choice === "follow") set.mutate(null);
    else if (choice === "shared" && group) set.mutate({ kind: "shared", group });
    else if (choice === "personal" || choice === "local") set.mutate({ kind: choice });
    else if (choice.startsWith("team:")) link.mutate(choice.slice("team:".length));
  };
  const teamOptions = teams.map((team) => ({
    value: `team:${team.id}` as Choice,
    label: t("sync.home.teamNamed", { team: team.name }),
    detail: t("sync.home.teamDetail"),
  }));
  const value = homeChoice(home);
  return (
    <section className="card memory-home" aria-labelledby="memory-home-title">
      <h2 id="memory-home-title">{t("sync.home.title")}</h2>
      <p className="small muted">{t("sync.home.text")}</p>
      {group ? (
        <Select<Choice>
          label={t("sync.home.title")}
          value={value}
          options={[
            { value: "personal", label: t("sync.home.personal"), detail: t("sync.home.personalDetail") },
            {
              value: "shared",
              label: t("sync.home.sharedNamed", { group: ownerName(group) }),
              detail: t("sync.home.sharedDetail"),
            },
            // A project's notes are matched by its remote: a project without
            // one cannot be a team's either.
            ...teamOptions,
            { value: "local", label: t("sync.home.local"), detail: t("sync.home.localDetail") },
            {
              value: "follow",
              label: t("sync.home.follow"),
              // While it follows its owner, this option says what that gives now.
              detail: home.chosen ? undefined : t("sync.home.followDetail", { home: homeLabel(home.home, t) }),
            },
          ]}
          disabled={set.isPending || link.isPending}
          onChange={choose}
          renderValue={(option) => (value === "follow" ? capitalize(homeLabel(home.home, t)) : option?.label)}
        />
      ) : (
        <p className="small">{t("sync.home.noRemote")}</p>
      )}
      {home.home?.kind === "personal" && !home.syncs && group && <p className="small warning">{t("sync.home.noOrigin")}</p>}
      {sync.data === null && <p className="small muted">{t("sync.home.off")}</p>}
    </section>
  );
}

function capitalize(text: string): string {
  return text.charAt(0).toUpperCase() + text.slice(1);
}
