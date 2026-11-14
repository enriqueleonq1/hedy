import { useState } from "react";
import { useQuery } from "@tanstack/react-query";

import { errorMessage, useApi } from "../../api/context";
import type { Account, MemoryView, Note, NoteCategory, Overview } from "../../api/types";
import { Mascot } from "../../components/Mascot/Mascot";
import { shortPath } from "../../components/projects";
import { Button, ConfirmButton } from "../../components/ui/Button";
import { Switch } from "../../components/ui/controls";
import { timeAgo } from "../../format";
import { DEFAULT_ACCOUNT_COLOR } from "../../theme/tokens";
import { NewNoteDialog } from "./NewNoteDialog";
import { NoteCard } from "./NoteCard";
import { authorLabel, keepLabel, memoryKey, useMemoryAction } from "./notes";

const GROUPS: { category: NoteCategory; title: string }[] = [
  { category: "decision", title: "Decisiones" },
  { category: "convention", title: "Convenciones" },
  { category: "rule", title: "Reglas" },
  { category: "preference", title: "Preferencias" },
];

export function MemoryScreen({
  overview,
  project,
  onBack,
}: {
  overview: Overview;
  project: string;
  onBack: () => void;
}) {
  const api = useApi();
  const memory = useQuery({ queryKey: memoryKey(project), queryFn: () => api.memory(project) });
  const [query, setQuery] = useState("");
  const search = useQuery({
    queryKey: [...memoryKey(project), "search", query.trim()],
    queryFn: () => api.searchMemory(project, query.trim()),
    enabled: query.trim().length > 0,
  });
  const [adding, setAdding] = useState(false);
  const info = overview.projects.find((p) => p.path === project);
  const name = info?.name ?? shortPath(project);
  const color = overview.accounts.find((a) => a.id === info?.account)?.color ?? DEFAULT_ACCOUNT_COLOR;

  return (
    <div className="screen memory">
      <header className="page-head">
        <div>
          <h1>Memoria de {name}</h1>
          <p className="muted">Lo que Hedy y tus IAs saben de este proyecto. Todo se guarda en tu PC.</p>
        </div>
        <div className="memory__tools">
          <input
            type="search"
            className="memory__search"
            aria-label="Buscar en la memoria"
            placeholder="Buscar en la memoria…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          <Button variant="primary" onClick={() => setAdding(true)}>
            + Nueva nota
          </Button>
          <Button variant="ghost" onClick={onBack}>
            Volver al chat
          </Button>
        </div>
      </header>

      {memory.isPending && <p className="muted">Cargando la memoria…</p>}
      {memory.isError && <p role="alert">No se pudo leer la memoria: {errorMessage(memory.error)}</p>}
      {memory.data && (
        <>
          {memory.data.protected && (
            <p className="hint">
              Proyecto protegido: sus notas se quedan aquí y no lee tus notas de todos los proyectos.
            </p>
          )}
          {memory.data.contradictions.map(([a, b]) => (
            <Contradiction key={`${a.id}-${b.id}`} project={project} a={a} b={b} color={color} />
          ))}
          <div className="memory__layout">
            <div className="memory__main">
              {query.trim() ? (
                <Results
                  project={project}
                  notes={search.data}
                  pending={search.isPending}
                  error={search.isError ? errorMessage(search.error) : null}
                />
              ) : (
                <Groups project={project} view={memory.data} />
              )}
            </div>
            <aside className="memory__side">
              <UsedLastTask notes={memory.data.usedLastTask} />
              <Stale project={project} notes={memory.data.stale} />
              <Readers project={project} view={memory.data} accounts={overview.accounts} />
            </aside>
          </div>
        </>
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
  return (
    <section className="contradiction" aria-label="Estas dos notas se contradicen">
      <Mascot color={color} state="thinking" size={64} label="" />
      <div className="contradiction__body">
        <h2>Estas dos notas se contradicen</h2>
        <div className="contradiction__notes">
          {[a, b].map((n) => (
            <blockquote key={n.id} className="contradiction__note">
              <p>{n.text}</p>
              <p className="small muted">
                {authorLabel(n)} · {timeAgo(n.createdAt)}
              </p>
            </blockquote>
          ))}
        </div>
        <div className="actions">
          <Button className="btn--warn-solid" disabled={busy} onClick={() => keep.mutate({ keep: a.id, forget: b.id })}>
            {keepLabel(a, b, "primera")}
          </Button>
          <Button className="btn--warn" disabled={busy} onClick={() => keep.mutate({ keep: b.id, forget: a.id })}>
            {keepLabel(b, a, "segunda")}
          </Button>
          <Button className="btn--warn" disabled={busy} onClick={() => dismiss.mutate(undefined)}>
            Las dos son válidas
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
  return (
    <>
      {view.proposed.length > 0 && (
        <section aria-labelledby="memory-proposed">
          <h2 id="memory-proposed" className="memory__group-title">
            Propuestas por tus IAs · para aprobar
          </h2>
          <ul className="memory__list">
            {view.proposed.map((n) => (
              <NoteCard key={n.id} project={project} note={n} />
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
              {title}
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
            Tuyas · en todos los proyectos
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
          <p>Todavía no hay notas.</p>
          <p className="hint">
            Agrega decisiones y convenciones, o deja que tus IAs las propongan mientras trabajan: nada se guarda sin tu
            aprobación.
          </p>
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
  if (error) return <p className="error-text">{error}</p>;
  if (pending) return <p className="muted">Buscando…</p>;
  if (!notes?.length) return <p className="muted">Nada coincide con esa búsqueda.</p>;
  return (
    <section aria-label="Resultados">
      <ul className="memory__list">
        {notes.map((n) => (
          <NoteCard key={n.id} project={project} note={n} />
        ))}
      </ul>
    </section>
  );
}

function UsedLastTask({ notes }: { notes: Note[] }) {
  return (
    <section className="card memory__panel" aria-labelledby="memory-used">
      <h2 id="memory-used">Usada en la última tarea</h2>
      {notes.length === 0 ? (
        <p className="small muted">La última tarea no consultó la memoria.</p>
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
  if (notes.length === 0) return null;
  return (
    <section className="card memory__panel" aria-labelledby="memory-stale">
      <h2 id="memory-stale" className="memory__stale-title">
        Para revisar
      </h2>
      <p className="small muted">No se usan hace más de 60 días. ¿Siguen siendo ciertas?</p>
      <ul className="memory__stale">
        {notes.map((n) => (
          <li key={n.id}>
            <span>{n.text}</span>
            <ConfirmButton small className="note__forget" confirmLabel="¿Olvidarla?" onConfirm={() => forget.mutate(n.id)}>
              Olvidar
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
  return (
    <section className="card memory__panel" aria-labelledby="memory-readers">
      <h2 id="memory-readers">Quién la lee</h2>
      {view.readers.length === 0 && (
        <p className="small muted">Asigna una cuenta al proyecto para que sus IAs lean la memoria.</p>
      )}
      <ul className="memory__readers">
        {view.readers.map((r) => {
          const account = accounts.find((a) => a.id === r.account);
          const label = account ? `${account.name}${account.kind === "apiKey" ? " (API)" : ""}` : r.account;
          return (
            <li key={r.account}>
              <span>{label}</span>
              <span className={`small ${r.reads ? "memory__on" : "muted"}`}>{r.reads ? "activa" : "apagada"}</span>
              <Switch
                label={`${label} lee la memoria`}
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
