import { useState } from "react";

import { useAction } from "../../api/context";
import type { Overview, PromptTemplate } from "../../api/types";
import { shortPath } from "../../components/projects";
import { Button, ConfirmButton } from "../../components/ui/Button";
import { TEMPLATES_KEY, useAllTemplates } from "../../tasks/hooks";
import { useT } from "../../i18n";
import { TemplateDialog } from "./TemplateDialog";

/** "Plantillas" in Settings: every template, global and per project. */
export function TemplatesSection({ overview }: { overview: Overview }) {
  const t = useT();
  const templates = useAllTemplates();
  const [editing, setEditing] = useState<PromptTemplate | "new" | null>(null);
  const restore = useAction((api) => api.restoreBuiltinTemplates(), { invalidate: [TEMPLATES_KEY] });
  const projects = overview.projects.map((p) => ({ path: p.path, name: p.name || shortPath(p.path) }));
  const projectName = (path: string) => projects.find((p) => p.path === path)?.name ?? shortPath(path);
  const list = templates.data ?? [];

  return (
    // Ajustes draws the title ("Plantillas"); the section is named after it.
    <section className="card settings__section" aria-label={t("templates.title")}>
      <div className="row row--between">
        <p className="hint">{t("templates.hint", { example: `{${t("templates.exampleName")}}` })}</p>
        <div className="actions">
          <Button small variant="ghost" disabled={restore.isPending} onClick={() => restore.mutate(undefined)}>
            {t("templates.restore")}
          </Button>
          <Button small variant="primary" onClick={() => setEditing("new")}>
            {t("templates.new")}
          </Button>
        </div>
      </div>
      {templates.isSuccess && list.length === 0 && <p className="hint">{t("templates.empty")}</p>}
      <ul className="template-list">
        {list.map((template) => (
          <li key={template.id} className="template-row">
            <div className="template-row__main">
              <div className="template-row__head">
                <strong>{template.name}</strong>
                <span className="chip chip--muted">
                  {template.project ? t("templates.ofProject", { project: projectName(template.project) }) : t("templates.global")}
                </span>
                {template.builtin && <span className="chip chip--muted">{t("templates.example")}</span>}
              </div>
              <p className="template-row__text">{template.text}</p>
            </div>
            <div className="template-row__actions">
              <Button small variant="ghost" aria-label={t("templates.editNamed", { name: template.name })} onClick={() => setEditing(template)}>
                {t("templates.edit")}
              </Button>
              <DeleteTemplate template={template} />
            </div>
          </li>
        ))}
      </ul>
      {editing && (
        <TemplateDialog
          template={editing === "new" ? undefined : editing}
          projects={projects}
          onClose={() => setEditing(null)}
        />
      )}
    </section>
  );
}

function DeleteTemplate({ template }: { template: PromptTemplate }) {
  const t = useT();
  const remove = useAction((api) => api.removeTemplate(template.id), { invalidate: [TEMPLATES_KEY] });
  return (
    <ConfirmButton
      small
      aria-label={t("templates.deleteNamed", { name: template.name })}
      confirmLabel={t("templates.confirmDelete")}
      disabled={remove.isPending}
      onConfirm={() => remove.mutate(undefined)}
    >
      {t("templates.delete")}
    </ConfirmButton>
  );
}
