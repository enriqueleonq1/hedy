import { useState } from "react";

import type { Note, NoteCategory } from "../../api/types";
import { Button, ConfirmButton } from "../../components/ui/Button";
import { Segmented } from "../../components/ui/controls";
import { timeAgo } from "../../format";
import { authorLabel, CATEGORY_LABEL, CATEGORY_OPTIONS, useMemoryAction } from "./notes";

export function NoteCard({ project, note }: { project: string; note: Note }) {
  const [editing, setEditing] = useState(false);
  const [text, setText] = useState(note.text);
  const [category, setCategory] = useState(note.category);
  const save = useMemoryAction(project, (api) => api.editNote(note.id, text, category));
  const forget = useMemoryAction(project, (api) => api.forgetNote(note.id));
  const approve = useMemoryAction(project, (api) => api.approveNote(note.id));
  const proposed = note.status === "proposed";

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
            <Button small onClick={edit}>
              Editar
            </Button>
            <Button small className="btn--danger" disabled={forget.isPending} onClick={() => forget.mutate(undefined)}>
              Rechazar
            </Button>
          </>
        ) : (
          <>
            <Button small onClick={edit}>
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
