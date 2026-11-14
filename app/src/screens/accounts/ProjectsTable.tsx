import { useState } from "react";

import { OVERVIEW_KEY, useAction } from "../../api/context";
import { phaseSummary } from "../../components/models";
import type { Overview, Project } from "../../api/types";
import { shortAccountLabel } from "../../components/accounts";
import { projectColor, shortPath, useProjectActions } from "../../components/projects";
import { Button, ConfirmButton } from "../../components/ui/Button";
import { Switch } from "../../components/ui/controls";
import { useT } from "../../i18n";
import { accountPalette } from "../../theme/tokens";
import { AddProjectDialog, useAddProject } from "./AddProjectDialog";
import { PhasesDialog } from "./PhasesDialog";

export function ProjectsTable({ overview }: { overview: Overview }) {
  const t = useT();
  const adding = useAddProject();

  return (
    <section className="card table-card" aria-labelledby="projects-table-title">
      <div className="section-head">
        <h2 id="projects-table-title">{t("common.projectsAndAccounts")}</h2>
        <Button small onClick={adding.start} disabled={overview.accounts.length === 0}>
          {t("common.addProjectButton")}
        </Button>
      </div>
      {overview.projects.length === 0 ? (
        <p className="empty">{t("accounts.table.empty")}</p>
      ) : (
        <div className="table-wrap">
          <table className="table">
            <thead>
              <tr>
                <th>{t("accounts.table.project")}</th>
                <th>{t("accounts.project.folder")}</th>
                <th>{t("common.client")}</th>
                <th>{t("common.account")}</th>
                <th>{t("common.models")}</th>
                <th>{t("common.protected")}</th>
                <th>{t("common.color")}</th>
                <th>
                  <span className="sr-only">{t("accounts.table.actions")}</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {overview.projects.map((p) => (
                <ProjectRow key={p.path} project={p} overview={overview} />
              ))}
            </tbody>
          </table>
        </div>
      )}
      {adding.path && <AddProjectDialog overview={overview} path={adding.path} onClose={adding.close} />}
    </section>
  );
}

function ProjectRow({ project, overview }: { project: Project; overview: Overview }) {
  const t = useT();
  const actions = useProjectActions();
  const assign = useAction(
    (api, vars: { account: string; protect?: boolean }) => api.useAccount(project.path, vars.account, vars.protect),
    { invalidate: [OVERVIEW_KEY, ["status", project.path]] },
  );
  const setColor = useAction((api, color: string | null) => api.setProjectColor(project.path, color));
  const setClient = useAction((api, client: string) => api.setProjectClient(project.path, client), {
    invalidate: [OVERVIEW_KEY, ["usageSummary"]],
  });
  const remove = useAction((api) => api.removeProject(project.path));
  const [editingPhases, setEditingPhases] = useState(false);
  const offered = overview.accounts.filter((a) => project.offerableAccounts.includes(a.id) || a.id === project.account);

  return (
    <tr>
      <th scope="row">
        <span className="dot" style={{ background: projectColor(project, overview.accounts) }} aria-hidden />
        {project.name}
      </th>
      <td className="muted">{shortPath(project.path)}</td>
      <td>
        <input
          className="table__input"
          aria-label={t("accounts.table.clientOf", { name: project.name })}
          placeholder={t("common.noClient")}
          key={project.client ?? ""}
          defaultValue={project.client ?? ""}
          onBlur={(e) => e.target.value.trim() !== (project.client ?? "") && setClient.mutate(e.target.value)}
        />
      </td>
      <td>
        <select
          className="pill-select pill-select--account"
          aria-label={t("accounts.table.accountOf", { name: project.name })}
          value={project.account ?? ""}
          disabled={project.protected}
          title={project.protected ? t("accounts.table.unprotectToChange") : undefined}
          onChange={(e) => assign.mutate({ account: e.target.value })}
        >
          {!project.account && <option value="">{t("common.noAccount")}</option>}
          {offered.map((a) => (
            <option key={a.id} value={a.id}>
              {shortAccountLabel(a)}
            </option>
          ))}
        </select>
      </td>
      <td>
        <button className="link-button" aria-label={t("accounts.table.phasesOf", { name: project.name })} onClick={() => setEditingPhases(true)}>
          {phaseSummary(project, overview.accounts, t)}
          {project.protected && t("accounts.table.protectedSuffix")}
        </button>
        {editingPhases && <PhasesDialog project={project} overview={overview} onClose={() => setEditingPhases(false)} />}
      </td>
      <td>
        <Switch
          checked={project.protected}
          disabled={!project.account}
          label={t("accounts.table.protectNamed", { name: project.name })}
          onChange={(protect) => project.account && assign.mutate({ account: project.account, protect })}
        />
      </td>
      <td>
        <select
          className="pill-select"
          aria-label={t("common.colorOf", { name: project.name })}
          value={project.color ?? ""}
          onChange={(e) => setColor.mutate(e.target.value || null)}
        >
          <option value="">{t("accounts.table.fromAccount")}</option>
          {accountPalette.map((c) => (
            <option key={c.hex} value={c.hex}>
              {t(c.label)}
            </option>
          ))}
        </select>
      </td>
      <td className="row-actions">
        <Button small variant="ghost" onClick={() => actions.openEditor(project)}>
          VS Code
        </Button>
        <ConfirmButton small confirmLabel={t("accounts.table.removeConfirm")} onConfirm={() => remove.mutate(undefined)}>
          {t("common.remove")}
        </ConfirmButton>
      </td>
    </tr>
  );
}
