import { useState } from "react";

import type { Note, NoteCategory } from "../../api/types";
import { Button, ConfirmButton } from "../../components/ui/Button";
import { Segmented } from "../../components/ui/controls";
import { timeAgo } from "../../format";
import { useT } from "../../i18n";
import { authorLabel, CATEGORY_LABEL, categoryOptions, useMemoryAction } from "./notes";

export function NoteCard({ project, note }: { project: string; note: Note }) {
  const t = useT();
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
      <div className="note__head">
        <p className="note__text">{note.text}</p>
        <span className={`category category--${note.category}`}>{t(CATEGORY_LABEL[note.category])}</span>
      </div>
      <div className="note__foot">
        <span className={`note__author${note.author === "user" ? "" : " note__author--ai"}`}>{authorLabel(note, t)}</span>
        <span className="small muted">
          {proposed ? t("memory.note.proposed") : ""} {timeAgo(note.createdAt, t)}
          {!proposed && (note.uses === 1 ? t("memory.note.usedOnce") : t("memory.note.usedTimes", { n: note.uses }))}
        </span>
        <span className="spacer" />
        {proposed ? (
          <>
            <Button small variant="primary" disabled={approve.isPending} onClick={() => approve.mutate(undefined)}>
              {t("memory.note.approve")}
            </Button>
            <Button small onClick={edit}>
              {t("common.edit")}
            </Button>
            <Button small className="btn--danger" disabled={forget.isPending} onClick={() => forget.mutate(undefined)}>
              {t("common.reject")}
            </Button>
          </>
        ) : (
          <>
            <Button small onClick={edit}>
              {t("common.edit")}
            </Button>
            <ConfirmButton small className="note__forget" confirmLabel={t("common.forgetConfirm")} onConfirm={() => forget.mutate(undefined)}>
              {t("common.forget")}
            </ConfirmButton>
          </>
        )}
      </div>
    </li>
  );
}
