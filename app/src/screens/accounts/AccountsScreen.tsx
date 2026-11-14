import { useState } from "react";

import { LOGIN_HINT } from "../../components/providers";
import { useAction } from "../../api/context";
import type { Account, Overview } from "../../api/types";
import { accountSubtitle, LoginBadge, UsageBars } from "../../components/accounts";
import { Mascot } from "../../components/Mascot/Mascot";
import { useTerminals } from "../../components/Terminal/TerminalProvider";
import { Button, ConfirmButton } from "../../components/ui/Button";
import { AddAccountDialog } from "./AddAccountDialog";
import { ApiKeyDialog } from "./ApiKeyDialog";
import { ProjectsTable } from "./ProjectsTable";
import { SpendByClient } from "./SpendByClient";

export function AccountsScreen({ overview }: { overview: Overview }) {
  const [adding, setAdding] = useState(false);
  const [keyFor, setKeyFor] = useState<Account | null>(null);
  const projectName = (path: string) => overview.projects.find((p) => p.path === path)?.name ?? path;

  return (
    <div className="screen">
      <header className="page-head">
        <div>
          <h1>Tus cuentas y modelos</h1>
          <p className="muted">Claude, ChatGPT, Kimi y los que agregues. Cada proyecto decide con qué cuenta trabaja.</p>
        </div>
        <Button variant="primary" onClick={() => setAdding(true)}>
          + Agregar cuenta
        </Button>
      </header>

      {overview.accounts.length === 0 ? (
        <p className="empty">
          Agrega tu primera cuenta. Las de suscripción (Claude, Codex) inician sesión con el login oficial de su CLI;
          Hedy nunca ve tus credenciales.
        </p>
      ) : (
        <div className="account-grid">
          {overview.accounts.map((a) => (
            <AccountCard
              key={a.id}
              account={a}
              projects={a.projects.map((path) => ({ path, name: projectName(path) }))}
              isDefault={overview.defaultAccount === a.id}
              onApiKey={() => setKeyFor(a)}
            />
          ))}
        </div>
      )}

      <ProjectsTable overview={overview} />
      <SpendByClient />

      {adding && (
        <AddAccountDialog overview={overview} onClose={() => setAdding(false)} onNeedsKey={(account) => setKeyFor(account)} />
      )}
      {keyFor && <ApiKeyDialog account={keyFor} onClose={() => setKeyFor(null)} />}
    </div>
  );
}

function AccountCard({
  account,
  projects,
  isDefault,
  onApiKey,
}: {
  account: Account;
  projects: { path: string; name: string }[];
  isDefault: boolean;
  onApiKey: () => void;
}) {
  const terminals = useTerminals();
  const makeDefault = useAction((api) => api.setDefaultAccount(account.id));
  const remove = useAction((api) => api.removeAccount(account.id));
  const loggedIn = account.login.state === "loggedIn";
  const classes = ["card", "account-card", isDefault && "account-card--current", account.kind === "apiKey" && "account-card--wide"];

  return (
    <article className={classes.filter(Boolean).join(" ")} aria-label={account.name}>
      <header className="account-card__head">
        <Mascot color={account.color} state={loggedIn ? "idle" : "sleeping"} size={44} label={`Mascota de ${account.name}`} />
        <div className="account-card__title">
          <h3>{account.name}</h3>
          <p className="muted">
            {accountSubtitle(account)}
            {isDefault && <span className="pill pill--current">por defecto</span>}
          </p>
        </div>
      </header>

      <UsageBars account={account} />
      <p className="muted small">
        {account.kind === "apiKey"
          ? "Gasto según las estimaciones de cada respuesta, contado por Hedy."
          : "El % aparece solo si la CLI lo informa; los tokens los cuenta Hedy."}
        {account.defaultFolder && " Usa la sesión de la carpeta por defecto de la CLI."}
      </p>
      {account.kind === "subscription" && !loggedIn && LOGIN_HINT[account.provider] && (
        <p className="small muted">{LOGIN_HINT[account.provider]}</p>
      )}

      {projects.length > 0 && (
        <ul className="pills" aria-label="Proyectos">
          {projects.map((p) => (
            <li key={p.path} className="pill">
              {p.name}
            </li>
          ))}
        </ul>
      )}

      <footer className="account-card__foot">
        <LoginBadge account={account} />
        {account.kind === "subscription" ? (
          <Button
            small
            variant={loggedIn ? "ghost" : "primary"}
            onClick={() => terminals.open({ kind: "login", account: account.id }, `Login · ${account.name}`)}
          >
            {loggedIn ? "Reconectar" : "Iniciar sesión"}
          </Button>
        ) : (
          <Button small variant={loggedIn ? "ghost" : "primary"} onClick={onApiKey}>
            {loggedIn ? "Cambiar API key" : "Guardar API key"}
          </Button>
        )}
        {!isDefault && (
          <Button small variant="ghost" onClick={() => makeDefault.mutate(undefined)}>
            Por defecto
          </Button>
        )}
        <ConfirmButton small confirmLabel="¿Quitar? Confirmar" onConfirm={() => remove.mutate(undefined)}>
          Quitar
        </ConfirmButton>
      </footer>
    </article>
  );
}
