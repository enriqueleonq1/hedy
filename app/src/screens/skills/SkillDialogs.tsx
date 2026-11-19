import { useState } from "react";

import { errorMessage, useApi } from "../../api/context";
import type { ImportPreview, Project, SkillOverview, SkillReview } from "../../api/types";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { useToast } from "../../components/ui/Toast";
import { useT } from "../../i18n";
import { FindingList } from "./SkillDetail";
import { fromAi, useLibraryAction, verdictKey } from "./library";

/**
 * Importing skills: where from (a SKILL.md, a folder or a project) and where
 * to, then what comes in with what Hedy measured, each one checked, and an
 * optional AI review per skill. What is imported is read again by Hedy.
 */
export function ImportDialog({ projects, onClose }: { projects: Project[]; onClose: () => void }) {
  const t = useT();
  const api = useApi();
  const toast = useToast();
  const [source, setSource] = useState("");
  const [project, setProject] = useState(projects[0]?.path ?? "");
  const [global, setGlobal] = useState(false);
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [replace, setReplace] = useState<Set<string>>(new Set());
  const [reviews, setReviews] = useState<Record<string, SkillReview>>({});
  const [reviewing, setReviewing] = useState<string | null>(null);
  const canBeGlobal = !projects.find((p) => p.path === project)?.protected;

  const read = useLibraryAction((a, vars: { source: string; project: string; global: boolean }) =>
    a.previewSkillImport(vars.project, vars.source, vars.global),
  );
  const run = useLibraryAction((a) =>
    a.importSkills(project, source, global, [...picked], [...replace]),
  );

  const choose = async (pick: () => Promise<string | null>) => {
    const path = await pick();
    if (path) setSource(path);
  };

  const look = () =>
    read.mutate(
      { source: source.trim(), project, global: global && canBeGlobal },
      {
        onSuccess: (found) => {
          setPreview(found);
          // Ready ones start picked; one with a problem only by hand.
          setPicked(new Set(found.candidates.filter((c) => c.problem === null).map((c) => c.source)));
          setReplace(new Set());
          setReviews({});
        },
      },
    );

  const askAi = async (file: string) => {
    setReviewing(file);
    try {
      const review = await api.reviewSkillImport(project, source.trim(), global, file);
      setReviews((r) => ({ ...r, [file]: review }));
    } catch (error) {
      toast.error(errorMessage(error, t));
    } finally {
      setReviewing(null);
    }
  };

  const toggle = (set: Set<string>, value: string, on: boolean) => {
    const next = new Set(set);
    if (on) next.add(value);
    else next.delete(value);
    return next;
  };

  const footer = preview ? (
    <>
      <Button variant="ghost" onClick={() => setPreview(null)}>
        {t("skillLib.import.back")}
      </Button>
      <Button
        variant="primary"
        disabled={picked.size === 0 || run.isPending}
        onClick={() =>
          run.mutate(undefined, {
            onSuccess: (report) => {
              const imported = report.imported.length;
              const updated = report.updated.length;
              if (imported > 0) toast.info(imported === 1 ? t("skillLib.import.doneOne") : t("skillLib.import.done", { n: imported }));
              if (updated > 0) toast.info(updated === 1 ? t("skillLib.import.updatedOne") : t("skillLib.import.updated", { n: updated }));
              for (const s of report.skipped) toast.error(t("skillLib.import.skipped", { name: s.name, why: s.problem.message }));
              onClose();
            },
          })
        }
      >
        {t("skillLib.import.submit", { n: picked.size })}
      </Button>
    </>
  ) : (
    <>
      <Button variant="ghost" onClick={onClose}>
        {t("common.cancel")}
      </Button>
      <Button variant="primary" disabled={!source.trim() || !project || read.isPending} onClick={look}>
        {read.isPending ? t("skillLib.import.reading") : t("skillLib.import.read")}
      </Button>
    </>
  );

  return (
    <Dialog title={t("skillLib.import.title")} onClose={onClose} footer={footer}>
      {!preview ? (
        <div className="skill-import">
          <label className="field">
            <span className="field__label">{t("skillLib.import.source")}</span>
            <input value={source} onChange={(e) => setSource(e.target.value)} spellCheck={false} />
            <span className="hint">{t("skillLib.import.sourceHint")}</span>
          </label>
          <div className="skill-import__pick">
            <Button small onClick={() => choose(() => api.pickFolder())}>
              {t("skillLib.import.pickFolder")}
            </Button>
            <Button small onClick={() => choose(() => api.pickFile())}>
              {t("skillLib.import.pickFile")}
            </Button>
          </div>
          <label className="field">
            <span className="field__label">{t("skillLib.import.project")}</span>
            <select value={project} onChange={(e) => setProject(e.target.value)}>
              {projects.map((p) => (
                <option key={p.path} value={p.path}>
                  {p.name}
                </option>
              ))}
            </select>
          </label>
          <fieldset className="skill-import__where">
            <legend className="field__label">{t("skillLib.import.where")}</legend>
            <label className="check">
              <input type="radio" name="skill-import-where" checked={!global} onChange={() => setGlobal(false)} />
              {t("skillLib.import.only")}
            </label>
            <label className="check">
              <input
                type="radio"
                name="skill-import-where"
                checked={global}
                disabled={!canBeGlobal}
                onChange={() => setGlobal(true)}
              />
              {t("skillLib.import.global")}
            </label>
          </fieldset>
        </div>
      ) : (
        <div className="skill-import">
          <p className="small muted">
            {t("skillLib.import.found", { n: preview.total })}{" "}
            {preview.total > preview.candidates.length &&
              t("skillLib.import.foundMore", { shown: preview.candidates.length, total: preview.total })}
          </p>
          <ul className="skill-import__list">
            {preview.candidates.map((c) => {
              const review = reviews[c.source];
              const taken = c.problem?.kind === "nameTaken";
              const blocked = c.problem !== null && !(taken && replace.has(c.name));
              return (
                <li key={c.source} className="skill-import__item">
                  <div className="skill-import__row">
                    <label className="check">
                      <input
                        type="checkbox"
                        checked={picked.has(c.source)}
                        disabled={blocked && !taken}
                        onChange={(e) => setPicked((p) => toggle(p, c.source, e.target.checked))}
                      />
                      <code className="skill__name">{c.name || c.source}</code>
                    </label>
                    {c.problem === null && (
                      <span className={`verdict verdict--${c.verdict === "useful" ? "untested" : c.verdict}`}>
                        {c.verdict === "useful"
                          ? t("skillLib.verdict.nothingAgainst")
                          : t("skillLib.verdict.probably", { verdict: t(verdictKey[c.verdict]).toLowerCase() })}
                      </span>
                    )}
                  </div>
                  {c.description && <p className="small">{c.description}</p>}
                  {c.problem && <p className="small skill-import__problem">{c.problem.message}</p>}
                  {taken && (
                    <label className="check small">
                      <input
                        type="checkbox"
                        checked={replace.has(c.name)}
                        onChange={(e) => {
                          setReplace((r) => toggle(r, c.name, e.target.checked));
                          if (e.target.checked) setPicked((p) => toggle(p, c.source, true));
                        }}
                      />
                      {t("skillLib.import.replace")}
                    </label>
                  )}
                  {c.findings.length > 0 && <FindingList findings={c.findings} />}
                  {c.droppedKeys.length > 0 && (
                    <p className="small muted">{t("skillLib.import.dropped", { keys: c.droppedKeys.join(", ") })}</p>
                  )}
                  {c.extraFiles.length > 0 && (
                    <p className="small muted">{t("skillLib.import.extra", { files: c.extraFiles.join(", ") })}</p>
                  )}
                  {review ? (
                    review.unread ? (
                      <p className="small">{t("skillLib.findings.unread")}</p>
                    ) : review.findings.filter(fromAi).length > 0 ? (
                      <FindingList findings={review.findings.filter(fromAi)} />
                    ) : (
                      <p className="small muted">{t("skillLib.findings.noneAi")}</p>
                    )
                  ) : (
                    c.problem === null && (
                      <Button small variant="ghost" disabled={reviewing !== null} onClick={() => askAi(c.source)}>
                        {reviewing === c.source ? t("skillLib.findings.reviewing") : t("skillLib.findings.review")}
                      </Button>
                    )
                  )}
                </li>
              );
            })}
          </ul>
        </div>
      )}
    </Dialog>
  );
}

/** Exporting approved skills as plain SKILL.md folders, to a folder the user picks. */
export function ExportDialog({ skills, onClose }: { skills: SkillOverview[]; onClose: () => void }) {
  const t = useT();
  const toast = useToast();
  const approved = skills.filter((s) => s.skill.status === "approved");
  const [picked, setPicked] = useState<Set<number>>(
    new Set(approved.filter((s) => s.verdict === "useful").map((s) => s.skill.id)),
  );
  const run = useLibraryAction(async (a) => {
    const to = await a.pickFolder();
    if (!to) return null;
    return { to, report: await a.exportSkills([...picked], to) };
  });
  return (
    <Dialog
      title={t("skillLib.export.title")}
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button
            variant="primary"
            disabled={picked.size === 0 || run.isPending}
            onClick={() =>
              run.mutate(undefined, {
                onSuccess: (done) => {
                  if (!done) return;
                  const n = done.report.written.length;
                  toast.info(n === 1 ? t("skillLib.export.doneOne", { to: done.to }) : t("skillLib.export.done", { n, to: done.to }));
                  for (const path of done.report.skipped) toast.error(t("skillLib.export.skipped", { path }));
                  onClose();
                },
              })
            }
          >
            {t("skillLib.export.submit", { n: picked.size })}
          </Button>
        </>
      }
    >
      <p className="small muted">{t("skillLib.export.intro")}</p>
      {approved.length === 0 ? (
        <p>{t("skillLib.export.none")}</p>
      ) : (
        <ul className="skill-export__list">
          {approved.map((s) => (
            <li key={s.skill.id}>
              <label className="check">
                <input
                  type="checkbox"
                  checked={picked.has(s.skill.id)}
                  onChange={(e) =>
                    setPicked((p) => {
                      const next = new Set(p);
                      if (e.target.checked) next.add(s.skill.id);
                      else next.delete(s.skill.id);
                      return next;
                    })
                  }
                />
                <code className="skill__name">{s.skill.name}</code>
                <span className="small muted">· {s.projectName ?? t("skillLib.meta.global")}</span>
              </label>
            </li>
          ))}
        </ul>
      )}
    </Dialog>
  );
}
