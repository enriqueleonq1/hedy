import { useAction } from "../../api/context";
import type { Overview, Project } from "../../api/types";
import { shortAccountLabel } from "../../components/accounts";
import { projectColor, shortPath, useProjectActions } from "../../components/projects";
import { Button, ConfirmButton } from "../../components/ui/Button";
import { Switch } from "../../components/ui/controls";
import { accountPalette } from "../../theme/tokens";
import { AddProjectDialog, useAddProject } from "./AddProjectDialog";

export function ProjectsTable({ overview }: { overview: Overview }) {
  const adding = useAddProject();

  return (
    <section className="card table-card" aria-labelledby="projects-table-title">
      <div className="section-head">
        <h2 id="projects-table-title">Proyectos y cuentas</h2>
        <Button small onClick={adding.start} disabled={overview.accounts.length === 0}>
          + Agregar proyecto
        </Button>
      </div>
      {overview.projects.length === 0 ? (
        <p className="empty">Todavía no hay proyectos.</p>
      ) : (
        <div className="table-wrap">
          <table className="table">
            <thead>
              <tr>
                <th>Proyecto</th>
                <th>Carpeta</th>
                <th>Cuenta</th>
                <th>Modelos</th>
                <th>Protegido</th>
                <th>Color</th>
                <th>
                  <span className="sr-only">Acciones</span>
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
  const actions = useProjectActions();
  const assign = useAction((api, vars: { account: string; protect?: boolean }) =>
    api.useAccount(project.path, vars.account, vars.protect),
  );
  const setColor = useAction((api, color: string | null) => api.setProjectColor(project.path, color));
  const remove = useAction((api) => api.removeProject(project.path));
  const account = overview.accounts.find((a) => a.id === project.account);
  const offered = overview.accounts.filter((a) => project.offerableAccounts.includes(a.id) || a.id === project.account);

  return (
    <tr>
      <th scope="row">
        <span className="dot" style={{ background: projectColor(project, overview.accounts) }} aria-hidden />
        {project.name}
      </th>
      <td className="muted">{shortPath(project.path)}</td>
      <td>
        <select
          className="pill-select pill-select--account"
          aria-label={`Cuenta de ${project.name}`}
          value={project.account ?? ""}
          disabled={project.protected}
          title={project.protected ? "Quita la protección para cambiar la cuenta" : undefined}
          onChange={(e) => assign.mutate({ account: e.target.value })}
        >
          {!project.account && <option value="">Sin cuenta</option>}
          {offered.map((a) => (
            <option key={a.id} value={a.id}>
              {shortAccountLabel(a)}
            </option>
          ))}
        </select>
      </td>
      <td className="muted">
        {account ? account.providerName : "—"}
        {project.protected && " · protegido"}
      </td>
      <td>
        <Switch
          checked={project.protected}
          disabled={!project.account}
          label={`Proteger ${project.name}`}
          onChange={(protect) => project.account && assign.mutate({ account: project.account, protect })}
        />
      </td>
      <td>
        <select
          className="pill-select"
          aria-label={`Color de ${project.name}`}
          value={project.color ?? ""}
          onChange={(e) => setColor.mutate(e.target.value || null)}
        >
          <option value="">De la cuenta</option>
          {accountPalette.map((c) => (
            <option key={c.hex} value={c.hex}>
              {c.name}
            </option>
          ))}
        </select>
      </td>
      <td className="row-actions">
        <Button small variant="ghost" onClick={() => actions.openEditor(project)}>
          VS Code
        </Button>
        <ConfirmButton small confirmLabel="¿Quitar de Hedy?" onConfirm={() => remove.mutate(undefined)}>
          Quitar
        </ConfirmButton>
      </td>
    </tr>
  );
}
