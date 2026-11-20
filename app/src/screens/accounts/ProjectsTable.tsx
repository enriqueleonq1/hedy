import { useRef } from "react";

import type { Overview, Project } from "../../api/types";
import { phaseSummary } from "../../components/models";
import { projectColor } from "../../components/projects";
import { Button } from "../../components/ui/Button";
import { useT } from "../../i18n";
import { AddProjectDialog, useAddProject } from "./AddProjectDialog";

/**
 * Every project at a glance, read-only: each row opens its settings sheet
 * (`#ps=`, App's route), where all of it is changed.
 */
export function ProjectsTable({ overview }: { overview: Overview }) {
  const t = useT();
  const adding = useAddProject();
  const noAccounts = overview.accounts.length === 0;

  return (
    <section className="card table-card" aria-labelledby="projects-table-title">
      <div className="section-head">
        <h2 id="projects-table-title">{t("common.projectsAndAccounts")}</h2>
        <Button
          small
          onClick={adding.start}
          disabled={noAccounts}
          aria-describedby={noAccounts ? "projects-table-needs-account" : undefined}
        >
          {t("common.addProjectButton")}
        </Button>
      </div>
      {/* A disabled button says why. */}
      {noAccounts && (
        <p id="projects-table-needs-account" className="hint">
          {t("accounts.table.needsAccount")}
        </p>
      )}
      {overview.projects.length === 0 ? (
        <p className="empty">{t("accounts.table.empty")}</p>
      ) : (
        <div className="table-wrap">
          <table className="table" aria-labelledby="projects-table-title">
            <thead>
              <tr>
                <th>{t("accounts.table.project")}</th>
                <th>{t("common.account")}</th>
                <th>{t("common.protected")}</th>
                <th>{t("common.client")}</th>
                <th>{t("accounts.table.steps")}</th>
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
  const link = useRef<HTMLAnchorElement>(null);
  const account = overview.accounts.find((a) => a.id === project.account);

  return (
    // A click anywhere on the row follows its link; the link is what the keyboard reaches.
    <tr className="table__row--link" onClick={(e) => !(e.target as Element).closest("a") && link.current?.click()}>
      <th scope="row">
        <a
          ref={link}
          className="link-button"
          href={`#ps=${encodeURIComponent(project.path)}`}
          title={t("projectSettings.title", { name: project.name })}
        >
          <span className="dot" style={{ background: projectColor(project, overview.accounts) }} aria-hidden />
          {project.name}
        </a>
      </th>
      <td>{account?.name ?? <span className="muted">{t("common.noAccount")}</span>}</td>
      <td>{project.protected ? t("accounts.table.protectedYes") : <span className="muted">{t("accounts.table.protectedNo")}</span>}</td>
      <td>{project.client ?? <span className="muted">{t("common.noClient")}</span>}</td>
      <td>{phaseSummary(project, overview.accounts, t)}</td>
    </tr>
  );
}
