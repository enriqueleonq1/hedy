import { useState } from "react";

import type { Skill, SkillDraft } from "../../api/types";
import { Button, ConfirmButton } from "../../components/ui/Button";
import { timeAgo } from "../../format";
import { useT } from "../../i18n";
import { SkillForm } from "./SkillForm";
import { lineDiff, skillAuthor, useFilesNotice, useSkillAction } from "./skills";

/**
 * One skill. A proposal may update an approved skill with the same name:
 * then it shows what changes (description and steps, line by line).
 */
export function SkillCard({
  project,
  skill,
  replaces = null,
  canBeGlobal,
}: {
  project: string;
  skill: Skill;
  replaces?: Skill | null;
  canBeGlobal: boolean;
}) {
  const t = useT();
  const notice = useFilesNotice();
  const [editing, setEditing] = useState(false);
  const proposed = skill.status === "proposed";
  const approve = useSkillAction(project, (api) => api.approveSkill(skill.id));
  const reject = useSkillAction(project, (api) => api.rejectSkill(skill.id));
  const remove = useSkillAction(project, (api) => api.removeSkill(skill.id));
  const save = useSkillAction(project, async (api, vars: { draft: SkillDraft; approve: boolean }) => {
    const edited = await api.editSkill(skill.id, project, vars.draft);
    return vars.approve ? api.approveSkill(skill.id) : edited;
  });
  const busy = approve.isPending || reject.isPending || remove.isPending;

  if (editing) {
    return (
      <li className="skill skill--editing">
        <SkillForm
          initial={{
            name: skill.name,
            description: skill.description,
            body: skill.body,
            global: skill.project === null,
          }}
          canBeGlobal={canBeGlobal}
          busy={save.isPending}
          onCancel={() => setEditing(false)}
          submitLabel={t("common.save")}
          onSubmit={(draft) =>
            save.mutate(
              { draft, approve: false },
              {
                onSuccess: (change) => {
                  setEditing(false);
                  if (change.skill.status === "approved") notice("saved", change.skill.name, change.files, t);
                },
              },
            )
          }
          extraSubmit={
            proposed
              ? {
                  label: t("skills.form.saveApprove"),
                  onSubmit: (draft) =>
                    save.mutate(
                      { draft, approve: true },
                      {
                        onSuccess: (change) => {
                          setEditing(false);
                          notice("saved", change.skill.name, change.files, t);
                        },
                      },
                    ),
                }
              : undefined
          }
        />
      </li>
    );
  }

  return (
    <li className={`skill${proposed ? " skill--proposed" : ""}`} aria-label={skill.name}>
      {replaces && <p className="note__kicker">{t("skills.update.title")}</p>}
      <div className="skill__head">
        <code className="skill__name">{skill.name}</code>
        {skill.project === null && <span className="skill__scope">{t("skills.card.global")}</span>}
      </div>
      {replaces ? (
        <Update before={replaces} after={skill} />
      ) : (
        <>
          <p className="skill__description">{skill.description}</p>
          <details className="skill__steps" open={proposed}>
            <summary>{proposed ? t("skills.card.steps") : t("skills.card.showSteps")}</summary>
            <pre className="skill__body">{skill.body}</pre>
          </details>
        </>
      )}
      <div className="note__foot">
        <span className={`note__author${skill.author === "user" ? "" : " note__author--ai"}`}>{skillAuthor(skill, t)}</span>
        <span className="small muted">
          {proposed ? `${t("skills.card.proposed")} ` : ""}
          {timeAgo(skill.createdAt, t)}
          {!proposed && skill.updatedAt !== null && t("skills.card.updated", { when: timeAgo(skill.updatedAt, t) })}
          {proposed && skill.sourceTask ? ` · ${t("skills.card.fromTask")}` : ""}
        </span>
        <span className="spacer" />
        {proposed ? (
          <>
            <Button
              small
              variant="primary"
              disabled={busy}
              onClick={() =>
                approve.mutate(undefined, { onSuccess: (change) => notice("saved", change.skill.name, change.files, t) })
              }
            >
              {t(replaces ? "skills.update.approve" : "skills.card.approve")}
            </Button>
            <Button small onClick={() => setEditing(true)}>
              {t("common.edit")}
            </Button>
            <Button small variant="ghost" disabled={busy} onClick={() => reject.mutate(undefined)}>
              {t("common.reject")}
            </Button>
          </>
        ) : (
          <>
            <Button small onClick={() => setEditing(true)}>
              {t("common.edit")}
            </Button>
            <ConfirmButton
              small
              className="note__forget"
              confirmLabel={t("skills.card.removeConfirm")}
              onConfirm={() => remove.mutate(undefined, { onSuccess: (files) => notice("removed", skill.name, files, t) })}
            >
              {t("skills.card.remove")}
            </ConfirmButton>
          </>
        )}
      </div>
    </li>
  );
}

/** What a proposed update changes: the description before and after, and the steps line by line. */
export function Update({ before, after }: { before: Skill; after: Skill }) {
  const t = useT();
  return (
    <div className="skill__update">
      {before.description !== after.description && (
        <div className="note__change">
          <div className="note__version">
            <p className="note__version-label">{t("skills.update.before")}</p>
            <p className="note__old">{before.description}</p>
          </div>
          <div className="note__version">
            <p className="note__version-label">{t("skills.update.after")}</p>
            <p className="skill__description">{after.description}</p>
          </div>
        </div>
      )}
      <pre className="skill__body skill__diff" aria-label={t("skills.card.steps")}>
        {lineDiff(before.body, after.body).map((line, i) => (
          <span key={i} className={`skill__line skill__line--${line.kind}`}>
            {line.kind !== "same" && (
              <span className="sr-only">{t(line.kind === "added" ? "skills.update.added" : "skills.update.removed")}: </span>
            )}
            <span aria-hidden className="skill__sign">
              {line.kind === "added" ? "+" : line.kind === "removed" ? "−" : " "}
            </span>
            {line.text}
            {"\n"}
          </span>
        ))}
      </pre>
    </div>
  );
}
