import { useState } from "react";

import type { NoteCategory } from "../../api/types";
import { Button } from "../../components/ui/Button";
import { Segmented } from "../../components/ui/controls";
import { Dialog } from "../../components/ui/Dialog";
import { useT } from "../../i18n";
import { categoryOptions, useMemoryAction } from "./notes";

export function NewNoteDialog({
  project,
  canBeGlobal,
  initialText = "",
  onClose,
}: {
  project: string;
  canBeGlobal: boolean;
  /** "Anotarlo" after a question the memory could not answer: the question, to answer. */
  initialText?: string;
  onClose: () => void;
}) {
  const t = useT();
  const [text, setText] = useState(initialText);
  const [category, setCategory] = useState<NoteCategory>("decision");
  const [scope, setScope] = useState<"project" | "global">("project");
  const add = useMemoryAction(project, (api) =>
    api.addNote(scope === "global" ? null : project, text, category),
  );
  return (
    <Dialog
      title={t("memory.new.title")}
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button
            variant="primary"
            disabled={!text.trim() || add.isPending}
            onClick={() => add.mutate(undefined, { onSuccess: onClose })}
          >
            {t("memory.new.save")}
          </Button>
        </>
      }
    >
      <label className="field">
        <span className="field__label">{t("memory.new.note")}</span>
        <textarea
          value={text}
          rows={3}
          placeholder={t("memory.new.placeholder")}
          onChange={(e) => setText(e.target.value)}
        />
      </label>
      <div className="field">
        <span className="field__label">{t("common.type")}</span>
        <Segmented<NoteCategory>
          label={t("common.type")}
          value={category}
          options={categoryOptions(t)}
          onChange={setCategory}
        />
      </div>
      {canBeGlobal && (
        <div className="field">
          <span className="field__label">{t("memory.new.where")}</span>
          <Segmented<"project" | "global">
            label={t("memory.new.where")}
            value={scope}
            options={[
              { value: "project", label: t("memory.new.project") },
              { value: "global", label: t("memory.new.global") },
            ]}
            onChange={setScope}
          />
        </div>
      )}
    </Dialog>
  );
}
