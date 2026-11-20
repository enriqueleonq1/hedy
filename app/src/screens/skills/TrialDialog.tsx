import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";

import { errorMessage, useApi } from "../../api/context";
import type { Project, SkillOverview, StartedTrial, TrialOutcome } from "../../api/types";
import { Button, ConfirmButton } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { useToast } from "../../components/ui/Toast";
import { useT } from "../../i18n";
import { useTasks } from "../../tasks/hooks";
import { SKILLS_OVERVIEW_KEY, trialSummary, verdictKey, voidKey } from "./library";
import { homeProject } from "./SkillsScreen";

/** How often a running trial is looked at (its runs end in the backend). */
const POLL_MS = 2000;
const TASKS_MAX = 3;

/**
 * Trying a skill: up to three tasks (the user's, or proposed by the AI from
 * the skill), each run twice in separate copies, with and without it, and
 * judged by the same check Hedy runs in each copy. The runs are ordinary
 * tasks: their permission cards reach the user in Activity.
 */
export function TrialDialog({ item, projects, onClose }: { item: SkillOverview; projects: Project[]; onClose: () => void }) {
  const t = useT();
  const api = useApi();
  const toast = useToast();
  const client = useQueryClient();
  const { skill } = item;
  const [project, setProject] = useState(homeProject(item, projects) ?? "");
  const [tasks, setTasks] = useState<string[]>([""]);
  const [proposing, setProposing] = useState(false);
  // null until the user edits it: the detected command shows meanwhile.
  const [edited, setCheck] = useState<string | null>(null);
  const [noCheck, setNoCheck] = useState(false);
  const [started, setStarted] = useState<StartedTrial | null>(null);
  const [outcome, setOutcome] = useState<TrialOutcome | null>(null);
  const [starting, setStarting] = useState(false);

  const checkInfo = useQuery({
    queryKey: ["trial-check", project],
    queryFn: () => api.trialCheck(project),
    enabled: project !== "",
  });
  const check = edited ?? checkInfo.data?.check ?? "";

  // While it runs, look for its end; the runs' statuses come from the tasks.
  // A failed look is said once, not every round.
  useEffect(() => {
    if (!started || outcome) return;
    let warned = false;
    const timer = setInterval(async () => {
      try {
        const ended = await api.takeTrialOutcome(started.id);
        if (ended) {
          setOutcome(ended);
          await client.invalidateQueries({ queryKey: SKILLS_OVERVIEW_KEY });
        }
      } catch (error) {
        if (!warned) toast.error(errorMessage(error, t));
        warned = true;
      }
    }, POLL_MS);
    return () => clearInterval(timer);
  }, [started, outcome, api, client, toast, t]);

  const prompts = tasks.map((task) => task.trim()).filter(Boolean);
  const runs = prompts.length * 2;

  const propose = async () => {
    setProposing(true);
    try {
      const proposed = await api.proposeTrialTasks(project, skill.id);
      if (proposed.length > 0) setTasks(proposed.slice(0, TASKS_MAX));
    } catch (error) {
      toast.error(errorMessage(error, t));
    } finally {
      setProposing(false);
    }
  };

  const start = async () => {
    setStarting(true);
    try {
      setOutcome(null);
      setStarted(await api.startSkillTrial(project, skill.id, prompts, noCheck ? null : check.trim() || null));
    } catch (error) {
      toast.error(errorMessage(error, t));
    } finally {
      setStarting(false);
    }
  };

  const cancel = async () => {
    if (started && !outcome) {
      await api.cancelSkillTrial(project, started.id).catch(() => undefined);
      toast.info(t("skillLib.try.cancelled"));
    }
    onClose();
  };

  const running = started !== null && outcome === null;
  const footer = running ? (
    <>
      <ConfirmButton variant="ghost" confirmLabel={t("skillLib.try.cancelConfirm")} onConfirm={cancel}>
        {t("skillLib.try.cancel")}
      </ConfirmButton>
      <Button title={t("skillLib.try.backgroundHint")} onClick={onClose}>
        {t("skillLib.try.background")}
      </Button>
    </>
  ) : outcome ? (
    <>
      <Button
        onClick={() => {
          setStarted(null);
          setOutcome(null);
        }}
      >
        {t("skillLib.try.again")}
      </Button>
      <Button variant="primary" onClick={onClose}>
        {t("common.close")}
      </Button>
    </>
  ) : (
    <>
      <Button variant="ghost" onClick={onClose}>
        {t("common.cancel")}
      </Button>
      <Button
        variant="primary"
        disabled={item.outside || prompts.length === 0 || project === "" || starting || (!noCheck && check.trim() === "" && checkInfo.isPending)}
        onClick={start}
      >
        {t("skillLib.try.start")}
      </Button>
    </>
  );

  return (
    // While it runs, Escape or a click outside never throws the runs away.
    <Dialog title={t("skillLib.try.title", { name: skill.name })} onClose={onClose} footer={footer} dismissible={!running} wide>
      <div className="trial">
        <p className="small muted">{t("skillLib.try.intro")}</p>
        {item.outside && <p className="hint trial__warn">{t("skillLib.try.outsideFirst")}</p>}

        {!started && (
          <>
            {skill.project === null && (
              <label className="field">
                <span className="field__label">{t("skillLib.try.project")}</span>
                <select value={project} onChange={(e) => setProject(e.target.value)}>
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
            <fieldset className="trial__tasks">
              <legend className="eyebrow">{t("skillLib.try.tasks")}</legend>
              {tasks.map((task, i) => (
                <div key={i} className="trial__task">
                  <input
                    aria-label={t("skillLib.try.task", { n: i + 1 })}
                    placeholder={t("skillLib.try.taskPlaceholder")}
                    value={task}
                    onChange={(e) => setTasks((all) => all.map((old, j) => (j === i ? e.target.value : old)))}
                  />
                  {tasks.length > 1 && (
                    <Button
                      small
                      variant="ghost"
                      aria-label={t("skillLib.try.removeTask", { n: i + 1 })}
                      onClick={() => setTasks((all) => all.filter((_, j) => j !== i))}
                    >
                      ✕
                    </Button>
                  )}
                </div>
              ))}
              <div className="trial__task-actions">
                {tasks.length < TASKS_MAX && (
                  <Button small variant="ghost" onClick={() => setTasks((all) => [...all, ""])}>
                    {t("skillLib.try.addTask")}
                  </Button>
                )}
                <Button small disabled={proposing || project === ""} onClick={propose}>
                  {proposing ? t("skillLib.try.proposing") : t("skillLib.try.propose")}
                </Button>
              </div>
            </fieldset>
            <fieldset className="trial__check">
              <legend className="eyebrow">{t("skillLib.try.check")}</legend>
              <input
                className="mono"
                aria-label={t("skillLib.try.check")}
                value={check}
                disabled={noCheck}
                spellCheck={false}
                onChange={(e) => setCheck(e.target.value)}
              />
              <label className="check small">
                <input type="checkbox" checked={noCheck} onChange={(e) => setNoCheck(e.target.checked)} />
                {t("skillLib.try.checkNone")}
              </label>
              <p className="small muted">{t("skillLib.try.checkHint")}</p>
              {!noCheck && check.trim() === "" && checkInfo.isSuccess && (
                <p className="small trial__warn">{t("skillLib.try.noCheckFound")}</p>
              )}
              {checkInfo.data && (
                <p className={`small ${checkInfo.data.isolated ? "muted" : "trial__warn"}`}>
                  {checkInfo.data.isolated ? t("skillLib.try.isolated") : t("skillLib.try.notIsolated")}
                </p>
              )}
            </fieldset>
            {prompts.length > 0 && <p className="small">{t("skillLib.try.cost", { n: runs })}</p>}
          </>
        )}

        {started && <Runs project={project} trial={started} done={outcome !== null} />}
        {running && <p className="small muted">{t("skillLib.try.backgroundHint")}</p>}

        {outcome && (
          <section aria-labelledby="trial-result" className="trial__result">
            <h3 id="trial-result" className="eyebrow">
              {t("skillLib.try.result")}
            </h3>
            {"judged" in outcome ? (
              <>
                <div className={`trial__verdict trial__verdict--${outcome.judged.verdict}`}>
                  <span className={`verdict verdict--${outcome.judged.verdict}`}>{t(verdictKey[outcome.judged.verdict])}</span>
                  <span>{trialSummary(outcome.judged, t)}</span>
                </div>
                <p className="small muted">{t("skillLib.try.hint")}</p>
              </>
            ) : (
              <p className="trial__void">{t(voidKey[outcome.void])}</p>
            )}
          </section>
        )}
      </div>
    </Dialog>
  );
}

/** The trial's runs, with and without the skill, as their tasks stand. */
function Runs({ project, trial, done }: { project: string; trial: StartedTrial; done: boolean }) {
  const t = useT();
  const tasks = useTasks(project).data ?? [];
  const status = (id: string) => tasks.find((task) => task.id === id)?.status;
  const finished = trial.tasks.filter((id) => {
    const s = status(id);
    return s !== undefined && s !== "running" && s !== "waitingApproval";
  }).length;
  return (
    <div className="trial__runs">
      {!done && <p className="small">{t("skillLib.try.running", { done: finished, n: trial.tasks.length })}</p>}
      <ul className="trial__run-list">
        {trial.tasks.map((id, i) => {
          const s = status(id);
          const state =
            s === "waitingApproval" ? "waiting" : s === "failed" ? "failed" : s === undefined || s === "running" ? "running" : "done";
          return (
            <li key={id} className={`trial__run trial__run--${state}`}>
              <span>
                {t("skillLib.try.run", {
                  n: Math.floor(i / 2) + 1,
                  side: i % 2 === 0 ? t("skillLib.try.with") : t("skillLib.try.without"),
                })}
              </span>
              <span className="small muted">{t(`skillLib.try.runState.${done && state !== "failed" ? "done" : state}`)}</span>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
