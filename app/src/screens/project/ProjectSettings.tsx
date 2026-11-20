// A project's own settings ("Ajustes" on its home): who works on it, how,
// and what else belongs to it. Every control saves as soon as it changes.

import { useId } from "react";

import { OVERVIEW_KEY, useAction } from "../../api/context";
import type { Account, Assignment, Overview, Phase, PhasePlan, Project } from "../../api/types";
import { accountOption } from "../../components/accounts";
import type { View } from "../../components/Layout";
import { MODEL_HINTS, MODEL_SUGGESTIONS, modelLabel } from "../../components/models";
import { shortPath, useProjectActions } from "../../components/projects";
import { Button, ConfirmButton } from "../../components/ui/Button";
import { ColorPicker, Switch } from "../../components/ui/controls";
import { Select, type SelectOption } from "../../components/ui/Select";
import { useT, type MessageKey } from "../../i18n";
import { MemoryHomeCard } from "../sync/ProjectSync";
import { ProjectHead } from "../workspace/ProjectHome";

const STEPS: { phase: Phase; label: MessageKey; optional: boolean }[] = [
  { phase: "plan", label: "projectSettings.step.plan", optional: false },
  { phase: "implement", label: "projectSettings.step.implement", optional: false },
  { phase: "tests", label: "projectSettings.step.tests", optional: true },
  { phase: "review", label: "projectSettings.step.review", optional: true },
];

export function ProjectSettings({
  overview,
  project: path,
  onNavigate,
}: {
  overview: Overview;
  project: string;
  onNavigate?: (view: View) => void;
}) {
  const t = useT();
  const project = overview.projects.find((p) => p.path === path);

  if (!project) {
    return (
      <div className="screen project-settings">
        <p className="empty">{t("projectSettings.missing")}</p>
        <div className="actions">
          <Button onClick={() => onNavigate?.({ screen: "welcome" })}>{t("projectSettings.goHome")}</Button>
        </div>
      </div>
    );
  }

  return (
    <div className="screen project-settings">
      <div className="project-settings__head">
        <button type="button" className="link-button project-settings__back" onClick={() => onNavigate?.({ screen: "workspace", project: path })}>
          <span aria-hidden>← </span>
          {t("projectSettings.back", { name: project.name })}
        </button>
        <ProjectHead
          name={project.name}
          title={t("projectSettings.title", { name: project.name })}
          path={path}
          current="settings"
          onTab={(tab) =>
            onNavigate?.(
              tab === "memory"
                ? { screen: "memory", project: path }
                : tab === "summary"
                  ? { screen: "status", project: path }
                  : { screen: "workspace", project: path },
            )
          }
        />
      </div>
      <AccountCard project={project} overview={overview} />
      <StepsCard project={project} overview={overview} />
      <GeneralCard project={project} />
      <MemoryHomeCard project={path} />
      <DangerCard project={project} onRemoved={() => onNavigate?.({ screen: "welcome" })} />
    </div>
  );
}

/** The accounts that may work here: the offered ones and its own. */
function usableAccounts(project: Project, overview: Overview): Account[] {
  return overview.accounts.filter((a) => project.offerableAccounts.includes(a.id) || a.id === project.account);
}

function AccountCard({ project, overview }: { project: Project; overview: Overview }) {
  const t = useT();
  const titleId = useId();
  const accountId = useId();
  const assign = useAction(
    (api, vars: { account: string; protect?: boolean }) => api.useAccount(project.path, vars.account, vars.protect),
    { invalidate: [OVERVIEW_KEY, ["status", project.path]] },
  );

  return (
    <section className="card" aria-labelledby={titleId}>
      <h2 id={titleId}>{t("projectSettings.account.title")}</h2>
      <div className="field">
        <label htmlFor={accountId}>{t("projectSettings.account.label")}</label>
        <Select
          id={accountId}
          title={t("projectSettings.account.label")}
          value={project.account ?? ""}
          disabled={project.protected || assign.isPending}
          onChange={(account) => account && assign.mutate({ account })}
          options={[
            ...(project.account ? [] : [{ value: "", label: t("common.noAccount") }]),
            ...usableAccounts(project, overview).map((a) => accountOption(a)),
          ]}
        />
        <p className="hint">
          {project.protected ? t("projectSettings.account.unprotectToChange") : t("projectSettings.account.text")}
        </p>
      </div>
      <div className="row row--between">
        <div>
          <span className="field__label">{t("projectSettings.protect")}</span>
          <p className="hint">
            {project.account ? t("projectSettings.protectHint") : t("projectSettings.protectNeedsAccount")}
          </p>
        </div>
        <Switch
          checked={project.protected}
          disabled={!project.account || assign.isPending}
          label={t("projectSettings.protect")}
          onChange={(protect) => project.account && assign.mutate({ account: project.account, protect })}
        />
      </div>
    </section>
  );
}

/**
 * The plan after changing one step. A step back at its default goes: an
 * optional one without an account (its model too), a required one on the
 * project's account without a model.
 */
export function changeStep(project: Project, phase: Phase, optional: boolean, change: Partial<Assignment>): PhasePlan {
  const next = { ...(project.phases[phase] ?? { account: "", model: null }), ...change };
  const account = next.account || (optional ? null : project.account);
  const model = next.model?.trim() || null;
  const others = Object.fromEntries(Object.entries(project.phases).filter(([p]) => p !== phase)) as PhasePlan;
  if (!account || (!optional && account === project.account && !model)) return others;
  return { ...others, [phase]: { account, model } };
}

function StepsCard({ project, overview }: { project: Project; overview: Overview }) {
  const t = useT();
  const titleId = useId();
  const save = useAction((api, phases: PhasePlan) => api.setProjectPhases(project.path, phases), {
    invalidate: [OVERVIEW_KEY, ["status", project.path]],
  });
  const main = overview.accounts.find((a) => a.id === project.account);
  const usable = usableAccounts(project, overview);

  return (
    <section className="card" aria-labelledby={titleId}>
      <h2 id={titleId}>{t("projectSettings.steps.title")}</h2>
      <p className="small muted">
        {project.protected ? t("projectSettings.steps.protected") : t("projectSettings.steps.text")}
      </p>
      <div className="phases">
        {STEPS.map(({ phase, label: labelKey, optional }) => {
          const step = t(labelKey);
          const assigned = project.phases[phase];
          // A required step on the project's own account reads as "Cuenta del proyecto".
          const chosen = !optional && assigned?.account === project.account ? "" : (assigned?.account ?? "");
          const worker = overview.accounts.find((a) => a.id === (chosen || (optional ? "" : project.account)));
          const accounts = optional ? usable : usable.filter((a) => a.id !== project.account);
          const accountOptions: SelectOption[] = [
            {
              value: "",
              label: optional
                ? t("projectSettings.step.skip")
                : main
                  ? t("projectSettings.step.projectAccountNamed", { name: main.name })
                  : t("projectSettings.step.projectAccount"),
            },
            ...accounts.map((a) => accountOption(a)),
          ];
          const models = worker ? MODEL_SUGGESTIONS[worker.provider] : [];
          const model = assigned?.model ?? "";
          const modelOptions: SelectOption[] = [
            { value: "", label: t("projectSettings.step.auto"), detail: t("projectSettings.step.autoDetail") },
            // A model typed before (or another provider's) stays visible.
            ...(model && !models.includes(model) ? [model] : []).concat(models).map((m) => ({
              value: m,
              label: modelLabel(m),
              detail: MODEL_HINTS[m] ? t(MODEL_HINTS[m]) : undefined,
            })),
          ];
          return (
            <div key={phase} className="phase-row">
              <span className="phase-row__label">{step}</span>
              <Select
                label={t("projectSettings.step.accountFor", { step })}
                title={step}
                value={chosen}
                disabled={save.isPending || accountOptions.length < 2}
                onChange={(account) => save.mutate(changeStep(project, phase, optional, { account }))}
                options={accountOptions}
              />
              <Select
                label={t("projectSettings.step.modelFor", { step })}
                title={t("projectSettings.step.modelFor", { step })}
                value={model}
                disabled={save.isPending || !worker || modelOptions.length < 2}
                onChange={(next) => save.mutate(changeStep(project, phase, optional, { model: next || null }))}
                options={modelOptions}
              />
            </div>
          );
        })}
      </div>
    </section>
  );
}

function GeneralCard({ project }: { project: Project }) {
  const t = useT();
  const titleId = useId();
  const clientId = useId();
  const clientHintId = useId();
  const actions = useProjectActions();
  const setClient = useAction((api, client: string) => api.setProjectClient(project.path, client), {
    invalidate: [OVERVIEW_KEY, ["usageSummary"]],
  });
  const setColor = useAction((api, color: string | null) => api.setProjectColor(project.path, color));

  return (
    <section className="card" aria-labelledby={titleId}>
      <h2 id={titleId}>{t("projectSettings.general.title")}</h2>
      <div className="field">
        <label htmlFor={clientId}>{t("projectSettings.client")}</label>
        <input
          id={clientId}
          aria-describedby={clientHintId}
          placeholder={t("common.noClient")}
          key={project.client ?? ""}
          defaultValue={project.client ?? ""}
          onBlur={(e) => e.target.value.trim() !== (project.client ?? "") && setClient.mutate(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
        />
        <p id={clientHintId} className="hint">
          {t("projectSettings.clientHint")}
        </p>
      </div>
      <div className="field">
        <span className="field__label">{t("projectSettings.color")}</span>
        <div className="row">
          <ColorPicker
            label={t("projectSettings.colorOf", { name: project.name })}
            value={project.color ?? ""}
            onChange={(color) => setColor.mutate(color)}
          />
          <Button small variant="ghost" aria-pressed={!project.color} onClick={() => project.color && setColor.mutate(null)}>
            {t("projectSettings.noColor")}
          </Button>
        </div>
        {!project.color && <p className="hint">{t("projectSettings.noColorHint")}</p>}
      </div>
      <div className="field">
        <span className="field__label">{t("projectSettings.folder")}</span>
        <div className="row row--between">
          <span className="mono project-settings__path" title={project.path}>
            {shortPath(project.path)}
          </span>
          <Button small onClick={() => actions.openEditor(project)}>
            {t("projectSettings.openEditor")}
          </Button>
        </div>
      </div>
    </section>
  );
}

function DangerCard({ project, onRemoved }: { project: Project; onRemoved: () => void }) {
  const t = useT();
  const titleId = useId();
  const remove = useAction((api) => api.removeProject(project.path), { onSuccess: onRemoved });

  return (
    <section className="card project-settings__danger" aria-labelledby={titleId}>
      <h2 id={titleId}>{t("projectSettings.danger.title")}</h2>
      <p className="small muted">{t("projectSettings.removeHint")}</p>
      <div className="actions">
        <ConfirmButton
          disabled={remove.isPending}
          confirmLabel={t("projectSettings.removeConfirm", { name: project.name })}
          onConfirm={() => remove.mutate(undefined)}
        >
          {t("projectSettings.remove")}
        </ConfirmButton>
      </div>
    </section>
  );
}
