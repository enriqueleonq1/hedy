import type { UseQueryResult } from "@tanstack/react-query";

import { errorMessage } from "../../api/context";
import type { SkillsView } from "../../api/types";
import { Mascot } from "../../components/Mascot/Mascot";
import { Dialog } from "../../components/ui/Dialog";
import { EmptyState } from "../../components/ui/EmptyState";
import { useT } from "../../i18n";
import { SkillCard } from "./SkillCard";
import { SkillForm } from "./SkillForm";
import { useFilesNotice, useSkillAction } from "./skills";

/** The "Habilidades" tab of the memory screen: proposals first, then approved skills. */
export function SkillsPanel({
  project,
  skills,
  color,
}: {
  project: string;
  skills: UseQueryResult<SkillsView>;
  color: string;
}) {
  const t = useT();
  if (skills.isPending) return <p className="muted">{t("skills.loading")}</p>;
  if (skills.isError) {
    return (
      <EmptyState
        error
        title={t("skills.loadError")}
        text={errorMessage(skills.error, t)}
        onRetry={() => void skills.refetch()}
      />
    );
  }
  const view = skills.data;
  const own = view.approved.filter((s) => s.project !== null);
  const global = view.approved.filter((s) => s.project === null);
  const canBeGlobal = !view.protected;
  const empty = view.approved.length === 0 && view.proposed.length === 0;
  return (
    <>
      {view.protected && <p className="hint">{t("skills.protected")}</p>}
      <div className="memory__layout">
        <div className="memory__main">
          {view.proposed.length > 0 && (
            <section aria-labelledby="skills-proposed">
              <h2 id="skills-proposed" className="memory__group-title">
                {t("skills.group.proposed")}
              </h2>
              <ul className="memory__list">
                {view.proposed.map((p) => (
                  <SkillCard
                    key={p.skill.id}
                    project={project}
                    skill={p.skill}
                    replaces={p.replaces}
                    canBeGlobal={canBeGlobal}
                  />
                ))}
              </ul>
            </section>
          )}
          {own.length > 0 && (
            <section aria-labelledby="skills-project">
              <h2 id="skills-project" className="memory__group-title">
                {t("skills.group.project")}
              </h2>
              <ul className="memory__list">
                {own.map((s) => (
                  <SkillCard key={s.id} project={project} skill={s} canBeGlobal={canBeGlobal} />
                ))}
              </ul>
            </section>
          )}
          {global.length > 0 && (
            <section aria-labelledby="skills-global">
              <h2 id="skills-global" className="memory__group-title">
                {t("skills.group.global")}
              </h2>
              <ul className="memory__list">
                {global.map((s) => (
                  <SkillCard key={s.id} project={project} skill={s} canBeGlobal={canBeGlobal} />
                ))}
              </ul>
            </section>
          )}
          {empty && (
            <div className="skills__empty">
              <Mascot color={color} state="sleeping" size={140} />
              <p className="skills__empty-title">{t("skills.empty")}</p>
              <p className="hint">{t("skills.emptyHint")}</p>
            </div>
          )}
        </div>
        <aside className="memory__side">
          <section className="card memory__panel" aria-labelledby="skills-where">
            <h2 id="skills-where">{t("skills.where.title")}</h2>
            <p className="small">{t("skills.where.native")}</p>
            <p className="small">{t("skills.where.tools")}</p>
            <p className="small muted">{t("skills.where.own")}</p>
          </section>
        </aside>
      </div>
    </>
  );
}

/** A skill written by the user: approved right away. */
export function NewSkillDialog({
  project,
  canBeGlobal,
  onClose,
}: {
  project: string;
  canBeGlobal: boolean;
  onClose: () => void;
}) {
  const t = useT();
  const notice = useFilesNotice();
  const add = useSkillAction(project, (api, draft: Parameters<typeof api.addSkill>[1]) => api.addSkill(project, draft));
  return (
    <Dialog title={t("skills.new.title")} onClose={onClose}>
      <SkillForm
        initial={{ name: "", description: "", body: "", global: false }}
        canBeGlobal={canBeGlobal}
        busy={add.isPending}
        submitLabel={t("skills.new.save")}
        onCancel={onClose}
        onSubmit={(draft) =>
          add.mutate(draft, {
            onSuccess: (change) => {
              notice("saved", change.skill.name, change.files, t);
              onClose();
            },
          })
        }
      />
    </Dialog>
  );
}
