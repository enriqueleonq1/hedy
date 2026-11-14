import { useState } from "react";

import type { SkillDraft } from "../../api/types";
import { Button } from "../../components/ui/Button";
import { Segmented } from "../../components/ui/controls";
import { useT } from "../../i18n";

/** Name, "when to use it", steps and scope of a skill, for a new one or an edit. */
export function SkillForm({
  initial,
  canBeGlobal,
  busy,
  submitLabel,
  onSubmit,
  onCancel,
  extraSubmit,
}: {
  initial: SkillDraft;
  canBeGlobal: boolean;
  busy: boolean;
  submitLabel: string;
  onSubmit: (draft: SkillDraft) => void;
  onCancel: () => void;
  /** A second way to submit, such as "Guardar y aprobar". */
  extraSubmit?: { label: string; onSubmit: (draft: SkillDraft) => void };
}) {
  const t = useT();
  const [draft, setDraft] = useState(initial);
  const set = (change: Partial<SkillDraft>) => setDraft((d) => ({ ...d, ...change }));
  const ready = draft.name.trim() !== "" && draft.description.trim() !== "" && draft.body.trim() !== "";
  return (
    <form
      className="skill__form"
      onSubmit={(e) => {
        e.preventDefault();
        if (ready) onSubmit(draft);
      }}
    >
      <label className="field">
        <span className="field__label">{t("skills.form.name")}</span>
        <input value={draft.name} spellCheck={false} onChange={(e) => set({ name: e.target.value })} />
        <span className="small muted">{t("skills.form.nameHint")}</span>
      </label>
      <label className="field">
        <span className="field__label">{t("skills.form.description")}</span>
        <input value={draft.description} onChange={(e) => set({ description: e.target.value })} />
        <span className="small muted">{t("skills.form.descriptionHint")}</span>
      </label>
      <label className="field">
        <span className="field__label">{t("skills.form.body")}</span>
        <textarea
          className="skill__body-input"
          value={draft.body}
          rows={6}
          spellCheck={false}
          placeholder={t("skills.new.bodyPlaceholder")}
          onChange={(e) => set({ body: e.target.value })}
        />
      </label>
      {canBeGlobal && (
        <div className="field">
          <span className="field__label">{t("skills.form.where")}</span>
          <Segmented<"project" | "global">
            label={t("skills.form.where")}
            value={draft.global ? "global" : "project"}
            options={[
              { value: "project", label: t("skills.form.project") },
              { value: "global", label: t("skills.form.global") },
            ]}
            onChange={(where) => set({ global: where === "global" })}
          />
        </div>
      )}
      <div className="actions">
        <Button type="submit" variant={extraSubmit ? "secondary" : "primary"} small disabled={!ready || busy}>
          {submitLabel}
        </Button>
        {extraSubmit && (
          <Button variant="primary" small disabled={!ready || busy} onClick={() => extraSubmit.onSubmit(draft)}>
            {extraSubmit.label}
          </Button>
        )}
        <Button small variant="ghost" onClick={onCancel}>
          {t("common.cancel")}
        </Button>
      </div>
    </form>
  );
}
