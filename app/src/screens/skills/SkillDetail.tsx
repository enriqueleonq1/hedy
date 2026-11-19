import { useState } from "react";

import type { Project, SkillFinding, SkillOverview, SkillReview } from "../../api/types";
import { Button, ConfirmButton } from "../../components/ui/Button";
import { Switch } from "../../components/ui/controls";
import { useToast } from "../../components/ui/Toast";
import { timeAgo } from "../../format";
import { useT } from "../../i18n";
import { SkillForm } from "../memory/SkillForm";
import { Update } from "../memory/SkillCard";
import { findingText, findingWeight, fromAi, trialSummary, useLibraryAction, verdictKey } from "./library";
import { homeProject, shownVerdict } from "./SkillsScreen";

const SIGN = { good: "✓", warning: "!", bad: "✕", info: "·" } as const;

/** One skill in detail: what it costs, what Hedy measures, its trial, where it is used and its steps. */
export function SkillDetail({
  item,
  projects,
  onTry,
}: {
  item: SkillOverview;
  projects: Project[];
  onTry: () => void;
}) {
  const t = useT();
  const toast = useToast();
  const { skill } = item;
  const proposed = skill.status === "proposed";
  const home = homeProject(item, projects);
  const [editing, setEditing] = useState(false);
  const [notesOf, setNotesOf] = useState(home ?? "");
  const [review, setReview] = useState<SkillReview | null>(null);
  const verdict = shownVerdict(item);

  const approve = useLibraryAction((api) => api.approveSkill(skill.id));
  const reject = useLibraryAction((api) => api.rejectSkill(skill.id));
  const remove = useLibraryAction((api) => api.removeSkill(skill.id));
  const outside = useLibraryAction((api, on: boolean) => api.setSkillOutside(skill.id, on));
  const save = useLibraryAction((api, draft: Parameters<typeof api.editSkill>[2]) => api.editSkill(skill.id, home ?? "", draft));
  const ask = useLibraryAction((api, project: string) => api.reviewSkill(skill.id, project));
  const busy = approve.isPending || reject.isPending || remove.isPending;

  if (editing && home) {
    return (
      <section className="skill-detail" aria-label={skill.name}>
        <SkillForm
          initial={{ name: skill.name, description: skill.description, body: skill.body, global: skill.project === null }}
          canBeGlobal={!projects.find((p) => p.path === home)?.protected}
          busy={save.isPending}
          submitLabel={t("common.save")}
          onCancel={() => setEditing(false)}
          onSubmit={(draft) => save.mutate(draft, { onSuccess: () => setEditing(false) })}
        />
      </section>
    );
  }

  const measured = item.findings;
  const meta = [
    skill.project ? t("skillLib.detail.inProject", { name: item.projectName ?? skill.project }) : t("skillLib.detail.everywhere"),
    skill.author === "user" ? t("skillLib.detail.byYou") : t("skillLib.detail.byAi", { who: skill.author }),
    skill.sourceTask ? t("skillLib.detail.fromTask") : null,
    skill.updatedAt
      ? t("skillLib.detail.updated", { when: timeAgo(skill.updatedAt, t) })
      : t("skillLib.detail.since", { when: timeAgo(skill.createdAt, t) }),
  ].filter(Boolean);

  return (
    <section className="skill-detail" aria-label={skill.name}>
      <div className="skill-detail__head">
        <h2 className="skill-detail__name">{skill.name}</h2>
        <span className={`verdict verdict--${verdict}`}>{t(verdictKey[verdict])}</span>
        <span className="spacer" />
        {proposed ? (
          <>
            <Button small variant="primary" disabled={busy} onClick={() => approve.mutate(undefined)}>
              {t(item.replaces ? "skillLib.detail.approveUpdate" : "skillLib.detail.approve")}
            </Button>
            <Button small disabled={!home} onClick={() => setEditing(true)}>
              {t("skillLib.detail.edit")}
            </Button>
            <Button small variant="ghost" disabled={busy} onClick={() => reject.mutate(undefined)}>
              {t("skillLib.detail.reject")}
            </Button>
          </>
        ) : (
          <>
            <Button small disabled={!home} onClick={() => setEditing(true)}>
              {t("skillLib.detail.edit")}
            </Button>
            <Button small disabled={!home} onClick={onTry}>
              {t("skillLib.detail.try")}
            </Button>
            <ConfirmButton
              small
              className="note__forget"
              confirmLabel={t("skillLib.detail.removeConfirm")}
              onConfirm={() => remove.mutate(undefined)}
            >
              {t("skillLib.detail.remove")}
            </ConfirmButton>
          </>
        )}
      </div>
      <p className="skill-detail__description">{skill.description}</p>
      <p className="small muted">{meta.join(" · ")}</p>

      <div className="skill-detail__stats">
        <div className="stat-card">
          <span className="stat-card__label">{t("skillLib.stat.context")}</span>
          <strong>{t("skillLib.stat.contextValue", { n: item.tokens.toLocaleString() })}</strong>
          <span className="stat-card__hint">{t("skillLib.stat.contextHint")}</span>
        </div>
        <div className="stat-card">
          <span className="stat-card__label">{t("skillLib.stat.analysis")}</span>
          <strong>
            {item.verdict === "useful"
              ? t("skillLib.verdict.nothingAgainst")
              : t("skillLib.verdict.probably", { verdict: t(verdictKey[item.verdict]).toLowerCase() })}
          </strong>
          <span className="stat-card__hint">{t("skillLib.stat.analysisHint")}</span>
        </div>
        <div className="stat-card">
          <span className="stat-card__label">{t("skillLib.stat.trial")}</span>
          {item.trial ? (
            <>
              <strong>{t("skillLib.stat.trialValue", { with: item.trial.passedWith, n: item.trial.tasks })}</strong>
              <span className="stat-card__hint">
                {t("skillLib.stat.trialHint", { without: item.trial.passedWithout, n: item.trial.tasks })}
              </span>
            </>
          ) : (
            <>
              <strong>{t("skillLib.stat.trialNone")}</strong>
              <span className="stat-card__hint">{t("skillLib.stat.trialNoneHint")}</span>
            </>
          )}
        </div>
      </div>
      {item.trial && <p className="small">{trialSummary(item.trial, t)}</p>}

      {!proposed && (
        <div className="skill-detail__outside">
          <Switch
            checked={item.outside}
            label={t("skillLib.outside.title")}
            disabled={skill.project === null || outside.isPending}
            onChange={(on) =>
              outside.mutate(on, {
                onSuccess: (files) =>
                  toast.info(
                    on
                      ? t("skillLib.outside.on", { name: skill.name, n: files.written.length })
                      : t("skillLib.outside.off", { name: skill.name }),
                  ),
              })
            }
          />
          <span className="skill-detail__outside-text">
            <strong>{t("skillLib.outside.title")}</strong>
            <span className="small muted">
              {skill.project
                ? t("skillLib.outside.hint", { project: item.projectName ?? skill.project })
                : t("skillLib.outside.global")}
            </span>
          </span>
        </div>
      )}

      <div className="skill-detail__findings">
        <section aria-labelledby={`hedy-${skill.id}`}>
          <h3 id={`hedy-${skill.id}`} className="eyebrow">
            {t("skillLib.findings.hedy")}
          </h3>
          <FindingList findings={measured} />
        </section>
        <section aria-labelledby={`ai-${skill.id}`}>
          <h3 id={`ai-${skill.id}`} className="eyebrow">
            {t("skillLib.findings.ai")}
          </h3>
          {review ? (
            <>
              {review.unread ? (
                <p className="small">{t("skillLib.findings.unread")}</p>
              ) : review.findings.filter(fromAi).length === 0 ? (
                <p className="small muted">{t("skillLib.findings.noneAi")}</p>
              ) : (
                <FindingList findings={review.findings.filter(fromAi)} />
              )}
              <p className="small muted">
                {t("skillLib.findings.reviewedBy", {
                  account: review.account ?? "",
                  s: Math.round(review.durationMs / 1000),
                })}{" "}
                {t("skillLib.findings.opinion")}
              </p>
            </>
          ) : (
            <div className="skill-detail__review">
              {skill.project === null && projects.length > 1 && (
                <label className="skill-detail__notes-of">
                  <span className="small muted">{t("skillLib.findings.notesOf")}</span>
                  <select value={notesOf} onChange={(e) => setNotesOf(e.target.value)}>
                    {projects
                      .filter((p) => !p.protected)
                      .map((p) => (
                        <option key={p.path} value={p.path}>
                          {p.name}
                        </option>
                      ))}
                  </select>
                </label>
              )}
              <Button
                small
                disabled={!notesOf || ask.isPending}
                onClick={() => ask.mutate(notesOf, { onSuccess: (r) => setReview(r) })}
              >
                {ask.isPending ? t("skillLib.findings.reviewing") : t("skillLib.findings.review")}
              </Button>
            </div>
          )}
        </section>
      </div>

      {item.replaces ? (
        <>
          <p className="note__kicker">{t("skillLib.detail.update", { name: item.replaces.name })}</p>
          <Update before={item.replaces} after={skill} />
        </>
      ) : (
        <pre className="skill-detail__body" aria-label={t("skillLib.detail.steps")}>
          {skill.body}
        </pre>
      )}
    </section>
  );
}

export function FindingList({ findings }: { findings: SkillFinding[] }) {
  const t = useT();
  return (
    <ul className="findings">
      {findings.map((f, i) => {
        const weight = findingWeight(f);
        return (
          <li key={i} className={`finding finding--${weight}`}>
            <span className="finding__sign" aria-hidden>
              {SIGN[weight]}
            </span>
            <span>{findingText(f, t)}</span>
          </li>
        );
      })}
    </ul>
  );
}
