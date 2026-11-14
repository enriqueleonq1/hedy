import { useState } from "react";

import type { NoteCategory } from "../../api/types";
import { Button } from "../../components/ui/Button";
import { Segmented } from "../../components/ui/controls";
import { Dialog } from "../../components/ui/Dialog";
import { CATEGORY_OPTIONS, useMemoryAction } from "./notes";

export function NewNoteDialog({
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
