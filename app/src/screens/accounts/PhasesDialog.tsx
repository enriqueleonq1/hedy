import { useState } from "react";

import { OVERVIEW_KEY, useAction } from "../../api/context";
import type { Assignment, Overview, Phase, PhasePlan, Project } from "../../api/types";
import { MODEL_SUGGESTIONS, PHASES } from "../../components/models";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { useT } from "../../i18n";

/** Who plans, implements, writes tests and reviews in a project. */
export function PhasesDialog({ project, overview, onClose }: { project: Project; overview: Overview; onClose: () => void }) {
  const t = useT();
  const [draft, setDraft] = useState<PhasePlan>(project.phases);
  const save = useAction((api, phases: PhasePlan) => api.setProjectPhases(project.path, phases), {
    invalidate: [OVERVIEW_KEY, ["status", project.path]],
    onSuccess: onClose,
  });
  const main = overview.accounts.find((a) => a.id === project.account);
  const offered = overview.accounts.filter((a) => project.offerableAccounts.includes(a.id));

  const update = (phase: Phase, optional: boolean, change: Partial<Assignment>) => {
    setDraft((old) => {
      const current = old[phase] ?? { account: "", model: null };
      const next = { ...current, ...change };
      // Back to the default for that phase: off for an optional one without an
      // account (its model goes too), the project's account for the others.
      if (!next.account && (optional || !next.model)) {
        return Object.fromEntries(Object.entries(old).filter(([p]) => p !== phase));
      }
      return { ...old, [phase]: next };
    });
  };

  const cleaned = (): PhasePlan => {
    const plan: PhasePlan = {};
    for (const { phase, optional } of PHASES) {
      const assigned = draft[phase];
      const account = assigned?.account || (optional ? null : project.account);
      if (assigned && account) plan[phase] = { account, model: assigned.model?.trim() || null };
    }
    return plan;
  };

  return (
    <Dialog
      title={t("accounts.phases.title", { name: project.name })}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button variant="primary" disabled={save.isPending} onClick={() => save.mutate(cleaned())}>
            {t("common.save")}
          </Button>
        </>
      }
    >
      <p className="hint">{t("accounts.phases.hint")}</p>
      <div className="phases">
        {PHASES.map(({ phase, label: labelKey, optional }) => {
          const label = t(labelKey);
          const assigned = draft[phase];
          const account = overview.accounts.find((a) => a.id === (assigned?.account || (optional ? "" : project.account)));
          const suggestions = account ? MODEL_SUGGESTIONS[account.provider] : [];
          const listId = `models-${phase}`;
          return (
            <div key={phase} className="phase-row">
              <span className="phase-row__label">{label}</span>
              <select
                aria-label={t("accounts.phases.accountFor", { phase: label })}
                value={assigned?.account ?? ""}
                onChange={(e) => update(phase, optional, { account: e.target.value })}
              >
                <option value="">
                  {optional
                    ? t("accounts.phases.skip")
                    : main
                      ? t("accounts.phases.projectAccountNamed", { name: main.name })
                      : t("accounts.phases.projectAccount")}
                </option>
                {offered.map((a) => (
                  <option key={a.id} value={a.id}>
                    {a.name} · {a.providerName}
                  </option>
                ))}
              </select>
              <input
                aria-label={t("accounts.phases.modelFor", { phase: label })}
                list={listId}
                placeholder={t("accounts.phases.auto")}
                disabled={optional && !assigned?.account}
                value={assigned?.model ?? ""}
                onChange={(e) => update(phase, optional, { model: e.target.value || null })}
              />
              <datalist id={listId}>
                {suggestions.map((m) => (
                  <option key={m} value={m} />
                ))}
              </datalist>
            </div>
          );
        })}
      </div>
    </Dialog>
  );
}
