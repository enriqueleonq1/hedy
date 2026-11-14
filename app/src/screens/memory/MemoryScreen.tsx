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
}: {
  overview: Overview;
  project: string;
  onBack: () => void;
}) {
  const api = useApi();
  const t = useT();
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
          <h1>{t("memory.title", { name })}</h1>
          <p className="muted">{t("memory.subtitle")}</p>
        </div>
        <div className="memory__tools">
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
          <Button variant="ghost" onClick={onBack}>
            {t("memory.backToChat")}
          </Button>
        </div>
      </header>

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
              {query.trim() ? (
                <Results
                  project={project}
                  notes={search.data}
                  pending={search.isPending}
                  error={search.isError ? errorMessage(search.error, t) : null}
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
          <Button className="btn--warn-solid" disabled={busy} onClick={() => keep.mutate({ keep: a.id, forget: b.id })}>
            {keepLabel(a, b, "first", t)}
          </Button>
          <Button className="btn--warn" disabled={busy} onClick={() => keep.mutate({ keep: b.id, forget: a.id })}>
            {keepLabel(b, a, "second", t)}
          </Button>
          <Button className="btn--warn" disabled={busy} onClick={() => dismiss.mutate(undefined)}>
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
