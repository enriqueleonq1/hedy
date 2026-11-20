// Memoria: one place for what the AIs know. A bar that searches the notes and,
// with Enter, asks them; "Para" picks whose memory; tabs for the notes, the
// skills, what waits for approval and the map of everything.

import { useCallback, useState, type FormEvent } from "react";
import { useQuery } from "@tanstack/react-query";

import { errorMessage, useApi } from "../../api/context";
import type { AskScope, GraphNode, Overview, SourceRef } from "../../api/types";
import type { MemoryTab, View } from "../../components/Layout";
import { Button } from "../../components/ui/Button";
import { useT, type MessageKey } from "../../i18n";
import { useInbox, type InboxItem } from "../../tasks/inbox";
import { useTeams } from "../../team/hooks";
import { DEFAULT_ACCOUNT_COLOR } from "../../theme/tokens";
import { AskPanel, MEMORY_GRAPH_KEY, type AskRequest } from "../ask/AskPanel";
import { revealHidden } from "../ask/citations";
import { MemoryGraphView } from "../ask/MemoryGraphView";
import { SkillsScreen } from "../skills/SkillsScreen";
import { NewNoteDialog } from "./NewNoteDialog";
import { NoteCard } from "./NoteCard";
import { NotesPanel } from "./NotesPanel";
import { SkillCard } from "./SkillCard";
import { memoryKey } from "./notes";
import { skillsKey } from "./skills";
import { NewSkillDialog, SkillsPanel } from "./SkillsPanel";

const TABS: { id: MemoryTab; label: MessageKey }[] = [
  { id: "notes", label: "skills.tabNotes" },
  { id: "skills", label: "skills.tabSkills" },
  { id: "pending", label: "memory.tab.pending" },
  { id: "map", label: "memory.tab.map" },
];

/** "Para": everything, one project, or one team (asking only). */
type For = { kind: "all" } | { kind: "project"; path: string } | { kind: "team"; id: string };

const forValue = (f: For) => (f.kind === "all" ? "all" : f.kind === "project" ? `project:${f.path}` : `team:${f.id}`);
const forOf = (value: string): For =>
  value.startsWith("project:")
    ? { kind: "project", path: value.slice(8) }
    : value.startsWith("team:")
      ? { kind: "team", id: value.slice(5) }
      : { kind: "all" };
const askScope = (f: For): AskScope =>
  f.kind === "project" ? { kind: "project", project: f.path } : f.kind === "team" ? { kind: "team", team: f.id } : { kind: "mine" };

type Proposed = Extract<InboxItem, { kind: "note" | "skill" }>;

export function MemoryHub({
  overview,
  project,
  tab = "notes",
  onNavigate,
}: {
  overview: Overview;
  /** "Para" this project; unset, everything. */
  project?: string;
  tab?: MemoryTab;
  onNavigate: (view: View) => void;
}) {
  const t = useT();
  const api = useApi();
  const teams = (useTeams().data ?? []).filter((team) => !team.waiting);
  const inbox = useInbox();
  // A team is chosen for asking only, while the route stays where it was: any
  // navigation (a project picked, the sidebar, the palette) drops it.
  const [teamPick, setTeamPick] = useState<{ team: string; at: string | undefined; tab: MemoryTab } | null>(null);
  const team = teamPick && teamPick.at === project && teamPick.tab === tab ? teamPick.team : null;
  const setTeam = (id: string | null) => setTeamPick(id ? { team: id, at: project, tab } : null);
  const shown: For = team ? { kind: "team", id: team } : project ? { kind: "project", path: project } : { kind: "all" };
  const [asking, setAsking] = useState(false);
  const [query, setQuery] = useState("");
  const [request, setRequest] = useState<AskRequest | null>(null);
  const [cited, setCited] = useState<{ cited: { ref: SourceRef; n: number }[]; asked: number }>({ cited: [], asked: 0 });
  const [writing, setWriting] = useState<{ project: string; text: string } | null>(null);
  const [addingSkill, setAddingSkill] = useState(false);
  const [node, setNode] = useState<GraphNode | null>(null);
  const graph = useQuery({ queryKey: MEMORY_GRAPH_KEY, queryFn: () => api.memoryGraph(), enabled: tab === "map" });
  const projectPath = shown.kind === "project" ? shown.path : null;

  // A project's own lists (a global proposal shows in every project that sees it); else the inbox's, once each.
  const memory = useQuery({
    queryKey: memoryKey(projectPath ?? ""),
    queryFn: () => api.memory(projectPath ?? ""),
    enabled: !!projectPath,
  });
  const projectSkills = useQuery({
    queryKey: skillsKey(projectPath ?? ""),
    queryFn: () => api.skills(projectPath ?? ""),
    enabled: !!projectPath,
  });
  const proposals: Proposed[] = projectPath
    ? [
        ...(memory.data?.proposed ?? []).map((proposal) => ({ kind: "note" as const, key: `note:${proposal.note.id}`, project: projectPath, proposal })),
        ...(projectSkills.data?.proposed ?? []).map((proposal) => ({ kind: "skill" as const, key: `skill:${proposal.skill.id}`, project: projectPath, proposal })),
      ]
    : inbox.items.filter((entry): entry is Proposed => entry.kind === "note" || entry.kind === "skill");
  const info = overview.projects.find((p) => p.path === projectPath);
  const color = overview.accounts.find((a) => a.id === info?.account)?.color ?? DEFAULT_ACCOUNT_COLOR;

  const go = (next: { project?: string; tab?: MemoryTab }) => {
    const target = "project" in next ? next.project : project;
    // Another project's notes start unfiltered; an answer on screen stays.
    if (target !== project) setQuery("");
    onNavigate({ screen: "memory", project: target, tab: next.tab ?? tab });
  };
  const choose = (value: string) => {
    const next = forOf(value);
    setTeam(next.kind === "team" ? next.id : null);
    if (next.kind !== "team") go({ project: next.kind === "project" ? next.path : undefined });
  };
  const ask = (e: FormEvent) => {
    e.preventDefault();
    const text = query.trim();
    // One question at a time: each one is a model call.
    if (text && !asking) setRequest((r) => ({ text, scope: askScope(shown), seq: (r?.seq ?? 0) + 1 }));
  };
  const onCited = useCallback((list: { ref: SourceRef; n: number }[], asked: number) => setCited({ cited: list, asked }), []);
  // Typing searches the project's notes as it goes; Enter asks.
  const searching = tab === "notes" && !!projectPath ? query : "";

  return (
    <div className="screen memory-hub">
      <header className="page-head">
        <div>
          <h1>{t("memory.hub.title")}</h1>
          <p className="muted">{t("memory.hub.subtitle")}</p>
        </div>
        <label className="memory-hub__for">
          <span>{t("memory.hub.for")}</span>
          <select value={forValue(shown)} onChange={(e) => choose(e.target.value)}>
            <option value="all">{t("memory.hub.forAll")}</option>
            {overview.projects.map((p) => (
              <option key={p.path} value={`project:${p.path}`}>
                {p.name}
              </option>
            ))}
            {teams.map((tm) => (
              <option key={tm.id} value={`team:${tm.id}`}>
                {t("ask.scope.team", { name: tm.name })}
              </option>
            ))}
          </select>
        </label>
      </header>

      <form className="memory-hub__bar" role="search" onSubmit={ask}>
        <input
          type="search"
          aria-label={t("memory.hub.ask")}
          placeholder={t("memory.hub.askPlaceholder")}
          maxLength={1000}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <Button variant="primary" type="submit" disabled={!query.trim() || asking}>
          {t("ask.submit")}
        </Button>
      </form>

      {request && (
        <AskPanel
          overview={overview}
          request={request}
          onCited={onCited}
          onOpenProject={(path) => {
            setTeam(null);
            go({ project: path, tab: "notes" });
          }}
          onWrite={(path, text) => setWriting({ project: path, text })}
          onBusy={setAsking}
          onClose={() => {
            setRequest(null);
            setAsking(false);
            setCited({ cited: [], asked: 0 });
          }}
        />
      )}

      <div className="memory__tabs" role="tablist" aria-label={t("memory.hub.tabs")}>
        {TABS.map(({ id, label }) => (
          <button
            key={id}
            role="tab"
            id={`memory-tab-${id}`}
            aria-selected={tab === id}
            aria-controls={`memory-panel-${id}`}
            className="side-tab"
            onClick={() => go({ tab: id })}
          >
            {t(label)}
            {id === "pending" && proposals.length > 0 && <span className="memory__badge">{proposals.length}</span>}
          </button>
        ))}
      </div>

      <div role="tabpanel" id={`memory-panel-${tab}`} aria-labelledby={`memory-tab-${tab}`} className="memory__panel-body">
        {tab === "notes" &&
          (projectPath ? (
            <NotesPanel key={projectPath} overview={overview} project={projectPath} query={searching} />
          ) : (
            <PickProject overview={overview} hint={shown.kind === "team" ? "memory.hub.teamNotes" : "memory.hub.pickNotes"} onPick={(path) => go({ project: path })} />
          ))}
        {tab === "skills" &&
          (projectPath ? (
            <>
              <div className="memory__panel-tools">
                <Button variant="primary" onClick={() => setAddingSkill(true)}>
                  {t("skills.newButton")}
                </Button>
                <Button variant="ghost" onClick={() => go({ project: undefined })}>
                  {t("memory.hub.allSkills")}
                </Button>
              </div>
              <SkillsPanel project={projectPath} skills={projectSkills} color={color} />
            </>
          ) : (
            <SkillsScreen overview={overview} embedded />
          ))}
        {tab === "pending" &&
          (proposals.length === 0 ? (
            <div className="empty">
              <p>{t("memory.hub.nothingPending")}</p>
              <p className="hint">{t("memory.hub.nothingPendingHint")}</p>
            </div>
          ) : (
            <ul className="memory__list">
              {proposals.map((entry) =>
                entry.kind === "note" ? (
                  <NoteCard
                    key={entry.key}
                    project={entry.project}
                    note={entry.proposal.note}
                    replaces={entry.proposal.replaces}
                    similar={entry.proposal.similar}
                  />
                ) : (
                  <SkillCard
                    key={entry.key}
                    project={entry.project}
                    skill={entry.proposal.skill}
                    replaces={entry.proposal.replaces}
                    canBeGlobal={!overview.projects.find((p) => p.path === entry.project)?.protected}
                  />
                ),
              )}
            </ul>
          ))}
        {tab === "map" &&
          (graph.data && graph.data.nodes.some((n) => n.kind === "note") ? (
            <div className="memory-hub__map">
              <MemoryGraphView graph={graph.data} cited={cited.cited} answer={cited.asked} onSelect={setNode} />
              {node && (
                <aside className="ask-node" aria-label={t(`ask.node.${node.kind}`)}>
                  <div className="ask-node__head">
                    <span className="ask__label">{t(`ask.node.${node.kind}`)}</span>
                    <button type="button" className="ask-node__close" onClick={() => setNode(null)}>
                      {t("ask.node.close")}
                    </button>
                  </div>
                  <p className="ask-node__text">{revealHidden(node.label)}</p>
                </aside>
              )}
            </div>
          ) : (
            <div className="ask-graph ask-graph--empty">
              <p role={graph.isError ? "alert" : undefined}>
                {graph.isLoading ? t("ask.thinking") : graph.isError ? errorMessage(graph.error, t) : t("ask.graph.empty")}
              </p>
            </div>
          ))}
      </div>

      <details className="memory-hub__how">
        <summary>{t("memory.hub.how")}</summary>
        <p className="small muted">{t("memory.hub.howText")}</p>
      </details>

      {writing && (
        <NewNoteDialog
          project={writing.project}
          // A protected project keeps its notes to itself (rule 4).
          canBeGlobal={!overview.projects.find((p) => p.path === writing.project)?.protected}
          initialText={writing.text}
          onClose={() => setWriting(null)}
        />
      )}
      {addingSkill && projectPath && (
        <NewSkillDialog project={projectPath} canBeGlobal={!info?.protected} onClose={() => setAddingSkill(false)} />
      )}
    </div>
  );
}

/** Notes live in a project: with "Para" on everything, which one. */
function PickProject({ overview, hint, onPick }: { overview: Overview; hint: MessageKey; onPick: (path: string) => void }) {
  const t = useT();
  return (
    <div className="memory-hub__pick">
      <p className="muted">{t(hint)}</p>
      <ul className="project-tasks__list">
        {overview.projects.map((p) => (
          <li key={p.path}>
            <button type="button" className="project-task" onClick={() => onPick(p.path)}>
              <span className="project-task__title">{p.name}</span>
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
