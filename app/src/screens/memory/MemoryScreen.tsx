import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { useAction, useApi } from "../../api/context";
import type { HedyApi } from "../../api/HedyApi";
import type { Account, MemoryView, Note, NoteCategory, Overview } from "../../api/types";
import { Mascot } from "../../components/Mascot/Mascot";
import { shortPath } from "../../components/projects";
import { Button, ConfirmButton } from "../../components/ui/Button";
import { Segmented, Switch } from "../../components/ui/controls";
import { Dialog } from "../../components/ui/Dialog";

export const CATEGORY_LABEL: Record<NoteCategory, string> = {
  decision: "Decisión",
  convention: "Convención",
  rule: "Regla",
  preference: "Preferencia",
};

const GROUPS: { category: NoteCategory; title: string }[] = [
  { category: "decision", title: "Decisiones" },
  { category: "convention", title: "Convenciones" },
  { category: "rule", title: "Reglas" },
  { category: "preference", title: "Preferencias" },
];

const CATEGORY_OPTIONS = (Object.keys(CATEGORY_LABEL) as NoteCategory[]).map((value) => ({
  value,
  label: CATEGORY_LABEL[value],
}));

export const memoryKey = (project: string) => ["memory", project] as const;

/** "hace 3 días" from Unix seconds. */
export function timeAgo(seconds: number, now = Date.now() / 1000): string {
  const minutes = Math.max(0, Math.round((now - seconds) / 60));
  if (minutes < 1) return "ahora";
  if (minutes < 60) return `hace ${minutes} min`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `hace ${hours} h`;
  const days = Math.round(hours / 24);
  if (days < 14) return days === 1 ? "ayer" : `hace ${days} días`;
  if (days < 60) return `hace ${Math.round(days / 7)} semanas`;
  const months = Math.round(days / 30);
  return months < 24 ? `hace ${months} meses` : `hace ${Math.round(months / 12)} años`;
}

const words = (text: string) => text.toLowerCase().match(/[\p{L}\p{N}]+/gu) ?? [];

/**
 * Button text to keep one of two contradicting notes: what makes it different
 * ("Quedarme con 24 h") when that is short, otherwise its position.
 */
export function keepLabel(note: Note, other: Note, position: "primera" | "segunda"): string {
  const others = new Set(words(other.text));
  const own = words(note.text);
  const start = own.findIndex((w) => !others.has(w));
  if (start >= 0) {
    // The differing word and, after a number, its unit ("24 h").
    const next = own[start + 1];
    const unit = /\d/.test(own[start] ?? "") && next && next.length <= 3 ? ` ${next}` : "";
    const phrase = `${own[start]}${unit}`;
    if (own.filter((w) => !others.has(w)).length <= 2 && phrase.length <= 14) return `Quedarme con ${phrase}`;
  }
  return `Quedarme con la ${position}`;
}

function authorLabel(note: Note): string {
  return note.author === "user" ? "tú" : note.author;
}

/** Runs a memory action and refreshes the project's memory. */
function useMemoryAction<TVars>(project: string, action: (api: HedyApi, vars: TVars) => Promise<unknown>) {
  const client = useQueryClient();
  return useAction(async (api, vars: TVars) => void (await action(api, vars)), {
    onSuccess: () => client.invalidateQueries({ queryKey: ["memory", project] }),
  });
}

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
    queryKey: ["memory", project, "search", query.trim()],
    queryFn: () => api.searchMemory(project, query.trim()),
    enabled: query.trim().length > 0,
  });
  const [adding, setAdding] = useState(false);
  const info = overview.projects.find((p) => p.path === project);
  const name = info?.name ?? shortPath(project);
  const color = overview.accounts.find((a) => a.id === info?.account)?.color ?? "#7AA2F7";

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
      {memory.isError && <p role="alert">No se pudo leer la memoria.</p>}
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
                <Results project={project} notes={search.data} pending={search.isPending} />
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

function Results({ project, notes, pending }: { project: string; notes: Note[] | undefined; pending: boolean }) {
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

function NoteCard({ project, note }: { project: string; note: Note }) {
  const [editing, setEditing] = useState(false);
  const [text, setText] = useState(note.text);
  const [category, setCategory] = useState(note.category);
  const save = useMemoryAction(project, (api) => api.editNote(note.id, text, category));
  const forget = useMemoryAction(project, (api) => api.forgetNote(note.id));
  const approve = useMemoryAction(project, (api) => api.approveNote(note.id));
  const proposed = note.status === "proposed";

  if (editing) {
    return (
      <li className="note note--editing">
        <form
          className="note__form"
          onSubmit={(e) => {
            e.preventDefault();
            save.mutate(undefined, { onSuccess: () => setEditing(false) });
          }}
        >
          <textarea aria-label="Texto de la nota" value={text} rows={2} onChange={(e) => setText(e.target.value)} />
          <Segmented<NoteCategory> label="Tipo" value={category} options={CATEGORY_OPTIONS} onChange={setCategory} />
          <div className="actions">
            <Button type="submit" variant="primary" small disabled={!text.trim() || save.isPending}>
              Guardar
            </Button>
            <Button small variant="ghost" onClick={() => setEditing(false)}>
              Cancelar
            </Button>
          </div>
        </form>
      </li>
    );
  }

  return (
    <li className={`note${proposed ? " note--proposed" : ""}`}>
      <div className="note__head">
        <p className="note__text">{note.text}</p>
        <span className={`category category--${note.category}`}>{CATEGORY_LABEL[note.category]}</span>
      </div>
      <div className="note__foot">
        <span className={`note__author${note.author === "user" ? "" : " note__author--ai"}`}>{authorLabel(note)}</span>
        <span className="small muted">
          {proposed ? "propuso" : ""} {timeAgo(note.createdAt)}
          {!proposed && ` · usada ${note.uses === 1 ? "1 vez" : `${note.uses} veces`}`}
        </span>
        <span className="spacer" />
        {proposed ? (
          <>
            <Button small variant="primary" disabled={approve.isPending} onClick={() => approve.mutate(undefined)}>
              Aprobar
            </Button>
            <Button small onClick={() => setEditing(true)}>
              Editar
            </Button>
            <Button small className="btn--danger" disabled={forget.isPending} onClick={() => forget.mutate(undefined)}>
              Rechazar
            </Button>
          </>
        ) : (
          <>
            <Button small onClick={() => setEditing(true)}>
              Editar
            </Button>
            <ConfirmButton small className="note__forget" confirmLabel="¿Olvidarla?" onConfirm={() => forget.mutate(undefined)}>
              Olvidar
            </ConfirmButton>
          </>
        )}
      </div>
    </li>
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

function NewNoteDialog({
  project,
  canBeGlobal,
  onClose,
}: {
  project: string;
  canBeGlobal: boolean;
  onClose: () => void;
}) {
  const [text, setText] = useState("");
  const [category, setCategory] = useState<NoteCategory>("decision");
  const [scope, setScope] = useState<"project" | "global">("project");
  const add = useMemoryAction(project, (api) =>
    api.addNote(scope === "global" ? null : project, text, category),
  );
  return (
    <Dialog
      title="Nueva nota"
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            Cancelar
          </Button>
          <Button
            variant="primary"
            disabled={!text.trim() || add.isPending}
            onClick={() => add.mutate(undefined, { onSuccess: onClose })}
          >
            Guardar nota
          </Button>
        </>
      }
    >
      <label className="field">
        <span className="field__label">Nota</span>
        <textarea
          value={text}
          rows={3}
          placeholder="Por ejemplo: los precios se guardan en USD."
          onChange={(e) => setText(e.target.value)}
        />
      </label>
      <div className="field">
        <span className="field__label">Tipo</span>
        <Segmented<NoteCategory> label="Tipo" value={category} options={CATEGORY_OPTIONS} onChange={setCategory} />
      </div>
      {canBeGlobal && (
        <div className="field">
          <span className="field__label">Dónde</span>
          <Segmented<"project" | "global">
            label="Dónde"
            value={scope}
            options={[
              { value: "project", label: "Solo este proyecto" },
              { value: "global", label: "Todos los proyectos" },
            ]}
            onChange={setScope}
          />
        </div>
      )}
    </Dialog>
  );
}
