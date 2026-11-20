import { useState } from "react";
import { useQuery } from "@tanstack/react-query";

import { errorMessage, useApi } from "../../api/context";
import type { Account, MemoryView, Note, NoteCategory, Overview } from "../../api/types";
import { Mascot } from "../../components/Mascot/Mascot";
import { Button, ConfirmButton } from "../../components/ui/Button";
import { Switch } from "../../components/ui/controls";
import { timeAgo } from "../../format";
import { useT, type MessageKey } from "../../i18n";
import { DEFAULT_ACCOUNT_COLOR } from "../../theme/tokens";
import { NewNoteDialog } from "./NewNoteDialog";
import { NoteCard } from "./NoteCard";
import { authorLabel, keepLabel, memoryKey, useMemoryAction } from "./notes";
import { MemoryHomeCard } from "../sync/ProjectSync";
import { LayerFilter, TeamNotes, useProjectTeam, useTeamProjectNotes, type Layer } from "../team/TeamMemory";

const GROUPS: { category: NoteCategory; title: MessageKey; hint: MessageKey }[] = [
  { category: "decision", title: "memory.group.decision", hint: "memory.hint.decision" },
  { category: "convention", title: "memory.group.convention", hint: "memory.hint.convention" },
  { category: "rule", title: "memory.group.rule", hint: "memory.hint.rule" },
  { category: "preference", title: "memory.group.preference", hint: "memory.hint.preference" },
];

/**
 * A project's notes in Memoria: what disagrees first, the notes by kind (or
 * what `query` finds), and beside them where they live, what the last task
 * used, what may be stale and which accounts read them.
 */
export function NotesPanel({ overview, project, query }: { overview: Overview; project: string; query: string }) {
  const api = useApi();
  const t = useT();
  const memory = useQuery({ queryKey: memoryKey(project), queryFn: () => api.memory(project) });
  const search = useQuery({
    queryKey: [...memoryKey(project), "search", query.trim()],
    queryFn: () => api.searchMemory(project, query.trim()),
    enabled: query.trim().length > 0,
  });
  const [adding, setAdding] = useState(false);
  // A team's project: one list, filtered by layer (Equipo, Proyecto, Solo tú).
  const team = useProjectTeam(project);
  const teamNotes = useTeamProjectNotes(team, project);
  const [layer, setLayer] = useState<Layer>("all");
  const mine = memory.data?.notes.length ?? 0;
  const layerCounts: Record<Layer, number> = {
    team: teamNotes.filter((n) => n.teamLayer).length,
    project: teamNotes.filter((n) => !n.teamLayer).length,
    mine,
    all: teamNotes.length + mine,
  };
  const info = overview.projects.find((p) => p.path === project);
  const color = overview.accounts.find((a) => a.id === info?.account)?.color ?? DEFAULT_ACCOUNT_COLOR;

  return (
    <>
      <div className="memory__panel-tools">
        <Button variant="primary" onClick={() => setAdding(true)}>
          {t("memory.newNoteButton")}
        </Button>
      </div>
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
      {adding && <NewNoteDialog project={project} canBeGlobal={!memory.data?.protected} onClose={() => setAdding(false)} />}
    </>
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
  const empty = view.notes.length === 0;
  const t = useT();
  return (
    <>
      {GROUPS.map(({ category, title, hint }) => {
        const notes = own.filter((n) => n.category === category);
        if (notes.length === 0) return null;
        return (
          <section key={category} aria-labelledby={`memory-${category}`}>
            <h2 id={`memory-${category}`} className="memory__group-title">
              {t(title)}
            </h2>
            <p className="memory__group-hint">{t(hint)}</p>
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
