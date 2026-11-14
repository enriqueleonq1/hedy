import { useQuery } from "@tanstack/react-query";

import { errorMessage, useApi } from "../api/context";
import type { Overview, Project, UsageSummary } from "../api/types";
import { AccountChip } from "../components/accounts";
import type { View } from "../components/Layout";
import { Mascot } from "../components/Mascot/Mascot";
import { projectColor, shortPath, useProjectActions } from "../components/projects";
import { Button } from "../components/ui/Button";
import { formatTokens, formatUsd } from "../format";
import { DEFAULT_ACCOUNT_COLOR } from "../theme/tokens";
import { AddProjectDialog, useAddProject } from "./accounts/AddProjectDialog";

/** Hedy's one-liner about the week: tokens, estimated cost and the busiest project. */
function weekUsageText(summary: UsageSummary): string {
  const tokens = summary.projects.reduce((sum, p) => sum + p.tokens, 0);
  if (tokens === 0) return "Aún no hay datos. Empezaré a registrarlo cuando trabajemos desde el chat de Hedy.";
  const costs = summary.projects.map((p) => p.costUsd).filter((c): c is number => c !== null);
  const cost = costs.length > 0 ? ` (unos ${formatUsd(costs.reduce((sum, c) => sum + c, 0))})` : "";
  const busiest = summary.projects[0];
  const where = busiest && summary.projects.length > 1 ? `; ${busiest.name} fue el proyecto más activo` : "";
  return `Llevamos ${formatTokens(tokens)} tokens${cost}${where}.`;
}

function WeekUsage() {
  const api = useApi();
  const summary = useQuery({ queryKey: ["usageSummary", 7], queryFn: () => api.usageSummary(7) });
  if (summary.isError) return <p className="error-text">{errorMessage(summary.error)}</p>;
  return <p>{summary.data ? weekUsageText(summary.data) : "Sumando lo de esta semana…"}</p>;
}

export function Welcome({ overview, onNavigate }: { overview: Overview; onNavigate: (v: View) => void }) {
  const adding = useAddProject();
  const color = overview.accounts.find((a) => a.id === overview.defaultAccount)?.color ?? DEFAULT_ACCOUNT_COLOR;
  const hasAccounts = overview.accounts.length > 0;
  const greeting = overview.userName ? `¡Hola, ${overview.userName}!` : "¡Hola!";

  return (
    <div className="screen welcome">
      <section className="hero">
        <Mascot color={color} state={hasAccounts ? "done" : "sleeping"} size={300} label="Hedy te saluda" />
        <div className="hero__text">
          <h1>
            {greeting}
            <br />
            <span className="hero__question">¿En qué trabajamos hoy?</span>
          </h1>
          <div className="balloon">
            <strong>Consumo de esta semana</strong>
            <WeekUsage />
          </div>
          {!hasAccounts && (
            <Button variant="primary" onClick={() => onNavigate({ screen: "accounts" })}>
              Agregar mi primera cuenta
            </Button>
          )}
        </div>
      </section>

      <section aria-labelledby="projects-title">
        <div className="section-head">
          <h2 id="projects-title">Proyectos</h2>
          <Button variant="primary" onClick={adding.start} disabled={!hasAccounts}>
            + Agregar proyecto
          </Button>
        </div>
        {overview.projects.length === 0 ? (
          <p className="empty">
            {hasAccounts
              ? "Agrega una carpeta de código y elige con qué cuenta trabaja."
              : "Primero agrega una cuenta; luego asígnala a tus proyectos."}
          </p>
        ) : (
          <div className="project-grid">
            {overview.projects.map((p) => (
              <ProjectCard key={p.path} project={p} overview={overview} onOpen={() => onNavigate({ screen: "workspace", project: p.path })} />
            ))}
          </div>
        )}
      </section>
      {adding.path && <AddProjectDialog overview={overview} path={adding.path} onClose={adding.close} />}
    </div>
  );
}

function ProjectCard({ project, overview, onOpen }: { project: Project; overview: Overview; onOpen: () => void }) {
  const actions = useProjectActions();
  const account = overview.accounts.find((a) => a.id === project.account);
  const runnable = account?.kind === "subscription";

  return (
    <article className="card project-card" style={{ borderTopColor: projectColor(project, overview.accounts) }}>
      <header>
        <h3>{project.name}</h3>
        {project.protected && <span className="badge badge--warn">Protegido</span>}
      </header>
      <p className="mono muted">{shortPath(project.path)}</p>
      {account ? <AccountChip account={account} /> : <span className="muted">Sin cuenta</span>}
      <div className="actions">
        <Button small variant="primary" onClick={onOpen}>
          Trabajar aquí
        </Button>
        <Button small disabled={!runnable} onClick={() => actions.openCli(project)}>
          {account?.providerName ?? "CLI"} en terminal
        </Button>
        <Button small onClick={() => actions.openShell(project)}>
          Terminal
        </Button>
        <Button small variant="ghost" onClick={() => actions.openEditor(project)}>
          Abrir en VS Code
        </Button>
      </div>
    </article>
  );
}
