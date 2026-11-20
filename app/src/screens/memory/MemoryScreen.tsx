import { useState } from "react";
import { useQuery } from "@tanstack/react-query";

import { errorMessage, useApi } from "../../api/context";
import type { Account, MemoryView, Note, NoteCategory, Overview } from "../../api/types";
import { Mascot } from "../../components/Mascot/Mascot";
import { shortPath } from "../../components/projects";
import { Button, ConfirmButton } from "../../components/ui/Button";
import { Switch } from "../../components/ui/controls";
import { timeAgo } from "../../format";
import { useT, type MessageKey } from "../../i18n";
import { DEFAULT_ACCOUNT_COLOR } from "../../theme/tokens";
import { NewNoteDialog } from "./NewNoteDialog";
import { NoteCard } from "./NoteCard";
import { authorLabel, keepLabel, memoryKey, useMemoryAction } from "./notes";
import { skillsKey } from "./skills";
import { NewSkillDialog, SkillsPanel } from "./SkillsPanel";
import { MemoryHomeCard } from "../sync/ProjectSync";
import { LayerFilter, TeamNotes, useProjectTeam, useTeamProjectNotes, type Layer } from "../team/TeamMemory";

type Tab = "notes" | "skills";

const GROUPS: { category: NoteCategory; title: MessageKey }[] = [
  { category: "decision", title: "memory.group.decision" },
  { category: "convention", title: "memory.group.convention" },
  { category: "rule", title: "memory.group.rule" },
  { category: "preference", title: "memory.group.preference" },
];

export function MemoryScreen({
  overview,
  project,
  onBack,
  onAsk,
  initialTab = "notes",
  onTab,
}: {
  overview: Overview;
  project: string;
  onBack: () => void;
  /** "Preguntar": ask this project's memory. */
  onAsk?: () => void;
  initialTab?: Tab;
  /** The tab changed (to remember it in the address). */
  onTab?: (tab: Tab) => void;
}) {
  const api = useApi();
  const t = useT();
  const [tab, setTab] = useState<Tab>(initialTab);
  const memory = useQuery({ queryKey: memoryKey(project), queryFn: () => api.memory(project) });
  const skills = useQuery({ queryKey: skillsKey(project), queryFn: () => api.skills(project) });
  const pendingSkills = skills.data?.proposed.length ?? 0;
  const pendingNotes = memory.data?.proposed.length ?? 0;
  const [query, setQuery] = useState("");
  const search = useQuery({
    queryKey: [...memoryKey(project), "search", query.trim()],
    queryFn: () => api.searchMemory(project, query.trim()),
    enabled: query.trim().length > 0,
  });
  const [adding, setAdding] = useState(false);
  const [addingSkill, setAddingSkill] = useState(false);
  // A team's project: one list, filtered by layer (Equipo, Proyecto, Solo tú).
  const team = useProjectTeam(project);
  const teamNotes = useTeamProjectNotes(team, project);
  const [layer, setLayer] = useState<Layer>("all");
  const mine = (memory.data?.notes.length ?? 0) + (memory.data?.proposed.length ?? 0);
  const layerCounts: Record<Layer, number> = {
    team: teamNotes.filter((n) => n.teamLayer).length,
    project: teamNotes.filter((n) => !n.teamLayer).length,
    mine,
    all: teamNotes.length + mine,
  };
  const info = overview.projects.find((p) => p.path === project);
  const name = info?.name ?? shortPath(project);
  const color = overview.accounts.find((a) => a.id === info?.account)?.color ?? DEFAULT_ACCOUNT_COLOR;

  return (
    <div className="screen memory">
      <header className="page-head">
        <div>
          <h1>{t("memory.title", { name })}</h1>
          <p className="muted">{t(tab === "notes" ? "memory.subtitle" : "skills.subtitle")}</p>
        </div>
        <div className="memory__tools">
          {tab === "notes" ? (
            <>
              <input
                type="search"
                className="memory__search"
                aria-label={t("memory.search")}
                placeholder={t("memory.searchPlaceholder")}
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
              <Button variant="primary" onClick={() => setAdding(true)}>
                {t("memory.newNoteButton")}
              </Button>
            </>
          ) : (
            <Button variant="primary" onClick={() => setAddingSkill(true)}>
              {t("skills.newButton")}
            </Button>
          )}
          {onAsk && <Button onClick={onAsk}>{t("ask.submit")}</Button>}
          <Button variant="ghost" onClick={onBack}>
            {t("memory.backToChat")}
          </Button>
        </div>
      </header>

      <div className="memory__tabs" role="tablist" aria-label={t("skills.tabs")}>
        {(["notes", "skills"] as const).map((id) => {
          const pending = id === "notes" ? pendingNotes : pendingSkills;
          return (
            <button
              key={id}
              role="tab"
              id={`memory-tab-${id}`}
              aria-selected={tab === id}
              aria-controls={`memory-panel-${id}`}
              className="side-tab"
              onClick={() => {
                setTab(id);
                onTab?.(id);
              }}
            >
              {t(id === "notes" ? "skills.tabNotes" : "skills.tabSkills")}
              {pending > 0 && <span className="memory__badge">{t("skills.pendingBadge", { n: pending })}</span>}
            </button>
          );
        })}
      </div>

      {tab === "skills" && (
        <div role="tabpanel" id="memory-panel-skills" aria-labelledby="memory-tab-skills" className="memory__panel-body">
          <SkillsPanel project={project} skills={skills} color={color} />
        </div>
      )}

      {tab === "notes" && (
        <div role="tabpanel" id="memory-panel-notes" aria-labelledby="memory-tab-notes" className="memory__panel-body">
          {memory.isPending && <p className="muted">{t("memory.loading")}</p>}
          {memory.isError && <p role="alert">{t("memory.loadError", { error: errorMessage(memory.error, t) })}</p>}
          {memory.data && (
            <>
              {memory.data.protected && <p className="hint">{t("memory.protected")}</p>}
              {memory.data.contradictions.map(([a, b]) => (
                <Contradiction key={`${a.id}-${b.id}`} project={project} a={a} b={b} color={color} />
              ))}
              <div className="memory__layout">
                <div className="memory__main">
                  {team && !query.trim() && <LayerFilter value={layer} onChange={setLayer} counts={layerCounts} />}
                  {query.trim() ? (
                    <Results
                      project={project}
                      notes={search.data}
                      pending={search.isPending}
                      error={search.isError ? errorMessage(search.error, t) : null}
                    />
                  ) : (
                    <>
                      {team && layer !== "mine" && <TeamNotes team={team} notes={teamNotes} layer={layer} />}
                      {(!team || layer === "all" || layer === "mine") && <Groups project={project} view={memory.data} />}
                    </>
                  )}
                </div>
                <aside className="memory__side">
                  <MemoryHomeCard project={project} />
                  <UsedLastTask notes={memory.data.usedLastTask} />
                  <Stale project={project} notes={memory.data.stale} />
                  <Readers project={project} view={memory.data} accounts={overview.accounts} />
                </aside>
              </div>
            </>
          )}
        </div>
      )}
      {addingSkill && (
        <NewSkillDialog project={project} canBeGlobal={!skills.data?.protected} onClose={() => setAddingSkill(false)} />
      )}
      {adding && (
        <NewNoteDialog
          project={project}
          canBeGlobal={!memory.data?.protected}
          onClose={() => setAdding(false)}
        />
      )}
    </div>
  );
}

function Contradiction({ project, a, b, color }: { project: string; a: Note; b: Note; color: string }) {
  const keep = useMemoryAction(project, (api, vars: { keep: number; forget: number }) =>
    api.keepNote(vars.keep, vars.forget),
  );
  const dismiss = useMemoryAction(project, (api) => api.dismissContradiction(a.id, b.id));
  const busy = keep.isPending || dismiss.isPending;
  const t = useT();
  return (
    <section className="contradiction" aria-label={t("memory.contradiction")}>
      <Mascot color={color} state="thinking" size={64} label="" />
      <div className="contradiction__body">
        <h2>{t("memory.contradiction")}</h2>
        <div className="contradiction__notes">
          {[a, b].map((n) => (
            <blockquote key={n.id} className="contradiction__note">
              <p>{n.text}</p>
              <p className="small muted">
                {authorLabel(n, t)} · {timeAgo(n.createdAt, t)}
              </p>
            </blockquote>
          ))}
        </div>
        <div className="actions">
          <Button disabled={busy} onClick={() => keep.mutate({ keep: a.id, forget: b.id })}>
            {keepLabel(a, b, "first", t)}
          </Button>
          <Button disabled={busy} onClick={() => keep.mutate({ keep: b.id, forget: a.id })}>
            {keepLabel(b, a, "second", t)}
          </Button>
          <Button variant="ghost" disabled={busy} onClick={() => dismiss.mutate(undefined)}>
            {t("memory.keepBoth")}
          </Button>
        </div>
      </div>
    </section>
  );
}

function Groups({ project, view }: { project: string; view: MemoryView }) {
  const own = view.notes.filter((n) => n.project !== null);
  const global = view.notes.filter((n) => n.project === null);
  const empty = view.notes.length === 0 && view.proposed.length === 0;
  const t = useT();
  return (
    <>
      {view.proposed.length > 0 && (
        <section aria-labelledby="memory-proposed">
          <h2 id="memory-proposed" className="memory__group-title">
            {t("memory.group.proposed")}
          </h2>
          <ul className="memory__list">
            {view.proposed.map((p) => (
              <NoteCard key={p.note.id} project={project} note={p.note} replaces={p.replaces} similar={p.similar} />
            ))}
          </ul>
        </section>
      )}
      {GROUPS.map(({ category, title }) => {
        const notes = own.filter((n) => n.category === category);
        if (notes.length === 0) return null;
        return (
          <section key={category} aria-labelledby={`memory-${category}`}>
            <h2 id={`memory-${category}`} className="memory__group-title">
              {t(title)}
            </h2>
            <ul className="memory__list">
              {notes.map((n) => (
                <NoteCard key={n.id} project={project} note={n} />
              ))}
            </ul>
          </section>
        );
      })}
      {global.length > 0 && (
        <section aria-labelledby="memory-global">
          <h2 id="memory-global" className="memory__group-title">
            {t("memory.group.global")}
          </h2>
          <ul className="memory__list">
            {global.map((n) => (
              <NoteCard key={n.id} project={project} note={n} />
            ))}
          </ul>
        </section>
      )}
      {empty && (
        <div className="empty">
          <p>{t("memory.empty")}</p>
          <p className="hint">{t("memory.emptyHint")}</p>
        </div>
      )}
    </>
  );
}

function Results({
  project,
  notes,
  pending,
  error,
}: {
  project: string;
  notes: Note[] | undefined;
  pending: boolean;
  error: string | null;
}) {
  const t = useT();
  if (error) return <p className="error-text">{error}</p>;
  if (pending) return <p className="muted">{t("memory.searching")}</p>;
  if (!notes?.length) return <p className="muted">{t("memory.noResults")}</p>;
  return (
    <section aria-label={t("memory.results")}>
      <ul className="memory__list">
        {notes.map((n) => (
          <NoteCard key={n.id} project={project} note={n} />
        ))}
      </ul>
    </section>
  );
}

function UsedLastTask({ notes }: { notes: Note[] }) {
  const t = useT();
  return (
    <section className="card memory__panel" aria-labelledby="memory-used">
      <h2 id="memory-used">{t("memory.usedLastTask")}</h2>
      {notes.length === 0 ? (
        <p className="small muted">{t("memory.usedLastTaskNone")}</p>
      ) : (
        <ul className="memory__checks">
          {notes.map((n) => (
            <li key={n.id}>
              <span className="memory__check" aria-hidden>
                ✓
              </span>
              {n.text}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function Stale({ project, notes }: { project: string; notes: Note[] }) {
  const forget = useMemoryAction(project, (api, id: number) => api.forgetNote(id));
  const t = useT();
  if (notes.length === 0) return null;
  return (
    <section className="card memory__panel" aria-labelledby="memory-stale">
      <h2 id="memory-stale" className="memory__stale-title">
        {t("memory.stale")}
      </h2>
      <p className="small muted">{t("memory.staleHint")}</p>
      <ul className="memory__stale">
        {notes.map((n) => (
          <li key={n.id}>
            <span>{n.text}</span>
            <ConfirmButton
              small
              className="note__forget"
              confirmLabel={t("common.forgetConfirm")}
              onConfirm={() => forget.mutate(n.id)}
            >
              {t("common.forget")}
            </ConfirmButton>
          </li>
        ))}
      </ul>
    </section>
  );
}

function Readers({ project, view, accounts }: { project: string; view: MemoryView; accounts: Account[] }) {
  const toggle = useMemoryAction(project, (api, vars: { account: string; reads: boolean }) =>
    api.setMemoryReader(project, vars.account, vars.reads),
  );
  const t = useT();
  return (
    <section className="card memory__panel" aria-labelledby="memory-readers">
      <h2 id="memory-readers">{t("memory.readers")}</h2>
      {view.readers.length === 0 && <p className="small muted">{t("memory.readersEmpty")}</p>}
      <ul className="memory__readers">
        {view.readers.map((r) => {
          const account = accounts.find((a) => a.id === r.account);
          const label = account ? `${account.name}${account.kind === "apiKey" ? " (API)" : ""}` : r.account;
          return (
            <li key={r.account}>
              <span>{label}</span>
              <span className={`small ${r.reads ? "memory__on" : "muted"}`}>{r.reads ? t("memory.readerOn") : t("memory.readerOff")}</span>
              <Switch
                label={t("memory.readerSwitch", { name: label })}
                checked={r.reads}
                disabled={toggle.isPending}
                onChange={(reads) => toggle.mutate({ account: r.account, reads })}
              />
            </li>
          );
        })}
      </ul>
    </section>
  );
}
