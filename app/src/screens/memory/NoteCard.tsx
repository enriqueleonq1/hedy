import { useState } from "react";

import type { Note, NoteCategory } from "../../api/types";
import { Button, ConfirmButton } from "../../components/ui/Button";
import { Segmented } from "../../components/ui/controls";
import { timeAgo } from "../../format";
import { useT } from "../../i18n";
import { authorLabel, CATEGORY_LABEL, categoryOptions, useMemoryAction } from "./notes";
import { shortPath } from "../../components/projects";
import { useProjectTeam, WhoForDialog } from "../team/TeamMemory";

/**
 * One note. A proposal may come with the approved note it changes (same
 * topic: shown as before/after) or with approved notes that look like it
 * ("parecida a": keep both, replace that one or discard the proposal).
 */
export function NoteCard({
  project,
  note,
  replaces = null,
  similar = [],
}: {
  project: string;
  note: Note;
  replaces?: Note | null;
  similar?: Note[];
}) {
  const t = useT();
  const [editing, setEditing] = useState(false);
  // A team's project: approving or sharing asks who it is for (6a).
  const team = useProjectTeam(project);
  const [whoFor, setWhoFor] = useState(false);
  const [text, setText] = useState(note.text);
  const [category, setCategory] = useState(note.category);
  const save = useMemoryAction(project, (api) => api.editNote(note.id, text, category));
  const forget = useMemoryAction(project, (api) => api.forgetNote(note.id));
  const approve = useMemoryAction(project, (api) => api.approveNote(note.id));
  const replace = useMemoryAction(project, (api, target: number) => api.replaceNote(note.id, target));
  const proposed = note.status === "proposed";
  const looksLike = proposed && !replaces ? similar : [];
  const busy = approve.isPending || forget.isPending || replace.isPending;

  // Every edit starts from the note as it is, not from an abandoned draft.
  const edit = () => {
    setText(note.text);
    setCategory(note.category);
    setEditing(true);
  };

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
          <textarea aria-label={t("memory.note.text")} value={text} rows={2} onChange={(e) => setText(e.target.value)} />
          <Segmented<NoteCategory>
            label={t("common.type")}
            value={category}
            options={categoryOptions(t)}
            onChange={setCategory}
          />
          <div className="actions">
            <Button type="submit" variant="primary" small disabled={!text.trim() || save.isPending}>
              {t("common.save")}
            </Button>
            <Button small variant="ghost" onClick={() => setEditing(false)}>
              {t("common.cancel")}
            </Button>
          </div>
        </form>
      </li>
    );
  }

  return (
    <li className={`note${proposed ? " note--proposed" : ""}`}>
      {replaces && <p className="note__kicker">{t("memory.change.title")}</p>}
      <div className="note__head">
        {replaces ? (
          <div className="note__change">
            <div className="note__version">
              <p className="note__version-label">{t("memory.change.before")}</p>
              <p className="note__old">{replaces.text}</p>
            </div>
            <div className="note__version">
              <p className="note__version-label">{t("memory.change.after")}</p>
              <p className="note__text">{note.text}</p>
            </div>
          </div>
        ) : (
          <p className="note__text">{note.text}</p>
        )}
        <span className={`category category--${note.category}`}>{t(CATEGORY_LABEL[note.category])}</span>
      </div>
      {looksLike.length > 0 && (
        <section className="note__similar" aria-label={t("memory.similar.title")}>
          <p className="note__version-label">{t("memory.similar.title")}</p>
          <ul>
            {looksLike.map((other) => (
              <li key={other.id}>
                <span className="note__similar-text">
                  {other.text}
                  <span className="small muted">
                    {" · "}
                    {authorLabel(other, t)}
                    {other.project === null ? ` · ${t("memory.similar.global")}` : ""}
                  </span>
                </span>
                <Button small disabled={busy} onClick={() => replace.mutate(other.id)}>
                  {t("memory.similar.replace")}
                </Button>
              </li>
            ))}
          </ul>
        </section>
      )}
      <div className="note__foot">
        <span className={`note__author${note.author === "user" ? "" : " note__author--ai"}`}>{authorLabel(note, t)}</span>
        {note.topic && (
          <span className="note__topic" title={t("memory.note.topic")}>
            <span className="sr-only">{t("memory.note.topic")}: </span>
            {note.topic}
          </span>
        )}
        <span className="small muted">
          {proposed ? t("memory.note.proposed") : ""} {timeAgo(note.createdAt, t)}
          {!proposed && note.updatedAt !== null && t("memory.note.updated", { when: timeAgo(note.updatedAt, t) })}
          {!proposed && (note.uses === 1 ? t("memory.note.usedOnce") : t("memory.note.usedTimes", { n: note.uses }))}
        </span>
        <span className="spacer" />
        {proposed ? (
          <>
            <Button
              small
              variant="primary"
              disabled={busy}
              onClick={() => (team && !replaces ? setWhoFor(true) : approve.mutate(undefined))}
            >
              {t(replaces ? "memory.change.approve" : looksLike.length > 0 ? "memory.similar.keepBoth" : "memory.note.approve")}
            </Button>
            <Button small onClick={edit}>
              {t("common.edit")}
            </Button>
            <Button small variant="ghost" disabled={busy} onClick={() => forget.mutate(undefined)}>
              {t(looksLike.length > 0 ? "memory.similar.discard" : "common.reject")}
            </Button>
          </>
        ) : (
          <>
            {team && note.project !== null && (
              <Button small onClick={() => setWhoFor(true)}>
                {t("team.who.share")}
              </Button>
            )}
            <Button small onClick={edit}>
              {t("common.edit")}
            </Button>
            <ConfirmButton small className="note__forget" confirmLabel={t("common.forgetConfirm")} onConfirm={() => forget.mutate(undefined)}>
              {t("common.forget")}
            </ConfirmButton>
          </>
        )}
      </div>
      {whoFor && team && (
        <WhoForDialog
          note={note}
          project={project}
          projectName={shortPath(project)}
          team={team}
          onClose={() => setWhoFor(false)}
        />
      )}
    </li>
  );
}
