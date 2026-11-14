import { useState } from "react";

import { OVERVIEW_KEY, useAction } from "../../api/context";
import type { Assignment, Overview, Phase, PhasePlan, Project } from "../../api/types";
import { MODEL_SUGGESTIONS, PHASES } from "../../components/models";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";

/** Who plans, implements, writes tests and reviews in a project. */
export function PhasesDialog({ project, overview, onClose }: { project: Project; overview: Overview; onClose: () => void }) {
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
      title={`Modelos por fase · ${project.name}`}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancelar</Button>
          <Button variant="primary" disabled={save.isPending} onClick={() => save.mutate(cleaned())}>
            Guardar
          </Button>
        </>
      }
    >
      <p className="hint">
        Cada fase puede hacerla otra cuenta o modelo. Si la misma IA planifica e implementa, sigue en la misma sesión; si no, la
        siguiente recibe el pedido, el plan aprobado y lo hecho hasta ahora.
      </p>
      <div className="phases">
        {PHASES.map(({ phase, label, optional }) => {
          const assigned = draft[phase];
          const account = overview.accounts.find((a) => a.id === (assigned?.account || (optional ? "" : project.account)));
          const suggestions = account ? MODEL_SUGGESTIONS[account.provider] : [];
          const listId = `models-${phase}`;
          return (
            <div key={phase} className="phase-row">
              <span className="phase-row__label">{label}</span>
              <select
                aria-label={`Cuenta para ${label}`}
                value={assigned?.account ?? ""}
                onChange={(e) => update(phase, optional, { account: e.target.value })}
              >
                <option value="">{optional ? "No usar esta fase" : `Cuenta del proyecto${main ? ` (${main.name})` : ""}`}</option>
                {offered.map((a) => (
                  <option key={a.id} value={a.id}>
                    {a.name} · {a.providerName}
                  </option>
                ))}
              </select>
              <input
                aria-label={`Modelo para ${label}`}
                list={listId}
                placeholder="automático"
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
