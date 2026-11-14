import { useQuery } from "@tanstack/react-query";

import { errorMessage, useApi } from "../api/context";
import type { Overview, Project, UsageSummary } from "../api/types";
import { AccountChip } from "../components/accounts";
import type { View } from "../components/Layout";
import { Mascot } from "../components/Mascot/Mascot";
import { projectColor, shortPath, useProjectActions } from "../components/projects";
import { Button } from "../components/ui/Button";
import { formatTokens, formatUsd } from "../format";
import { useLanguage, useT, type Language, type Translate } from "../i18n";
import { DEFAULT_ACCOUNT_COLOR } from "../theme/tokens";
import { AddProjectDialog, useAddProject } from "./accounts/AddProjectDialog";

/** Hedy's one-liner about the week: tokens, estimated cost and the busiest project. */
function weekUsageText(summary: UsageSummary, t: Translate, language: Language): string {
  const tokens = summary.projects.reduce((sum, p) => sum + p.tokens, 0);
  if (tokens === 0) return t("welcome.usage.empty");
  const costs = summary.projects.map((p) => p.costUsd).filter((c): c is number => c !== null);
  const cost =
    costs.length > 0
      ? t("welcome.usage.cost", { cost: formatUsd(costs.reduce((sum, c) => sum + c, 0), language) })
      : "";
  const busiest = summary.projects[0];
  const where = busiest && summary.projects.length > 1 ? t("welcome.usage.busiest", { name: busiest.name }) : "";
  return t("welcome.usage.total", { tokens: formatTokens(tokens), cost, busiest: where });
}

function WeekUsage() {
  const api = useApi();
  const t = useT();
  const language = useLanguage();
  const summary = useQuery({ queryKey: ["usageSummary", 7], queryFn: () => api.usageSummary(7) });
  if (summary.isError) return <p className="error-text">{errorMessage(summary.error, t)}</p>;
  return <p>{summary.data ? weekUsageText(summary.data, t, language) : t("welcome.usage.loading")}</p>;
}

export function Welcome({ overview, onNavigate }: { overview: Overview; onNavigate: (v: View) => void }) {
  const t = useT();
  const adding = useAddProject();
  const color = overview.accounts.find((a) => a.id === overview.defaultAccount)?.color ?? DEFAULT_ACCOUNT_COLOR;
  const hasAccounts = overview.accounts.length > 0;
  const greeting = overview.userName ? t("welcome.greetingNamed", { name: overview.userName }) : t("welcome.greeting");

  return (
    <div className="screen welcome">
      <section className="hero">
        <Mascot color={color} state={hasAccounts ? "done" : "sleeping"} size={300} label={t("welcome.mascot")} />
        <div className="hero__text">
          <h1>
            {greeting}
            <br />
            <span className="hero__question">{t("welcome.question")}</span>
          </h1>
          <div className="balloon">
            <strong>{t("welcome.weekUsage")}</strong>
            <WeekUsage />
          </div>
          {!hasAccounts && (
            <Button variant="primary" onClick={() => onNavigate({ screen: "accounts" })}>
              {t("welcome.addFirstAccount")}
            </Button>
          )}
        </div>
      </section>

      <section aria-labelledby="projects-title">
        <div className="section-head">
          <h2 id="projects-title">{t("common.projects")}</h2>
          <Button variant="primary" onClick={adding.start} disabled={!hasAccounts}>
            {t("common.addProjectButton")}
          </Button>
        </div>
        {overview.projects.length === 0 ? (
          <p className="empty">{hasAccounts ? t("welcome.empty.hasAccounts") : t("welcome.empty.noAccounts")}</p>
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
  const t = useT();
  const actions = useProjectActions();
  const account = overview.accounts.find((a) => a.id === project.account);
  const runnable = account?.kind === "subscription";

  return (
    <article className="card project-card" style={{ borderTopColor: projectColor(project, overview.accounts) }}>
      <header>
        <h3>{project.name}</h3>
        {project.protected && <span className="badge badge--warn">{t("common.protected")}</span>}
      </header>
      <p className="mono muted">{shortPath(project.path)}</p>
      {account ? <AccountChip account={account} /> : <span className="muted">{t("common.noAccount")}</span>}
      <div className="actions">
        <Button small variant="primary" onClick={onOpen}>
          {t("welcome.card.workHere")}
        </Button>
        <Button small disabled={!runnable} onClick={() => actions.openCli(project)}>
          {t("welcome.card.cliInTerminal", { cli: account?.providerName ?? "CLI" })}
        </Button>
        <Button small onClick={() => actions.openShell(project)}>
          {t("common.terminal")}
        </Button>
        <Button small variant="ghost" onClick={() => actions.openEditor(project)}>
          {t("common.openInVsCode")}
        </Button>
      </div>
    </article>
  );
}
