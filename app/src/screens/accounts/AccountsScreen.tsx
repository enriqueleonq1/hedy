import { useEffect, useRef, useState } from "react";

import { LOGIN_HINT } from "../../components/providers";
import { useAction } from "../../api/context";
import type { Account, Overview } from "../../api/types";
import { accountSubtitle, LoginBadge, UsageBars } from "../../components/accounts";
import { Mascot } from "../../components/Mascot/Mascot";
import { useTerminals } from "../../components/Terminal/TerminalProvider";
import { Button, ConfirmButton } from "../../components/ui/Button";
import { useT } from "../../i18n";
import { AddAccountDialog } from "./AddAccountDialog";
import { ApiKeyDialog } from "./ApiKeyDialog";
import { ProjectsTable } from "./ProjectsTable";
import { SpendByClient } from "./SpendByClient";

/** `focus`: the account whose card is scrolled to, highlighted and focused (from the sidebar or the top bar). */
export function AccountsScreen({ overview, focus }: { overview: Overview; focus?: string }) {
  const t = useT();
  const [adding, setAdding] = useState(false);
  const [keyFor, setKeyFor] = useState<Account | null>(null);
  const projectName = (path: string) => overview.projects.find((p) => p.path === path)?.name ?? path;

  return (
    <div className="screen">
      <header className="page-head">
        <div>
          <h1>{t("accounts.title")}</h1>
          <p className="muted">{t("accounts.subtitle")}</p>
        </div>
        <Button variant="primary" onClick={() => setAdding(true)}>
          {t("common.addAccountButton")}
        </Button>
      </header>

      {overview.accounts.length === 0 ? (
        <p className="empty">{t("accounts.empty")}</p>
      ) : (
        <div className="account-grid">
          {overview.accounts.map((a) => (
            <AccountCard
              key={a.id}
              account={a}
              projects={a.projects.map((path) => ({ path, name: projectName(path) }))}
              isDefault={overview.defaultAccount === a.id}
              focused={focus === a.id}
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

/** The name, and a pencil to rename it in place: Enter or leaving saves, Escape cancels. */
function AccountName({ account }: { account: Account }) {
  const t = useT();
  const rename = useAction((api, name: string) => api.renameAccount(account.id, name));
  const [editing, setEditing] = useState(false);

  if (!editing) {
    return (
      <div className="account-card__name">
        <h3>{account.name}</h3>
        <Button
          small
          variant="ghost"
          className="account-card__rename"
          aria-label={t("accounts.card.rename", { name: account.name })}
          title={t("accounts.card.renameHint")}
          onClick={() => setEditing(true)}
        >
          ✎
        </Button>
      </div>
    );
  }

  const save = (value: string) => {
    setEditing(false);
    const name = value.trim();
    if (name && name !== account.name) rename.mutate(name);
  };
  return (
    <input
      className="account-card__name-input"
      aria-label={t("accounts.card.nameLabel")}
      defaultValue={account.name}
      maxLength={60}
      autoFocus
      onFocus={(e) => e.target.select()}
      onBlur={(e) => save(e.target.value)}
      onKeyDown={(e) => {
        if (e.key === "Enter") e.currentTarget.blur();
        if (e.key === "Escape") setEditing(false);
      }}
    />
  );
}

function AccountCard({
  account,
  projects,
  isDefault,
  focused,
  onApiKey,
}: {
  account: Account;
  projects: { path: string; name: string }[];
  isDefault: boolean;
  focused: boolean;
  onApiKey: () => void;
}) {
  const card = useRef<HTMLElement>(null);
  useEffect(() => {
    if (!focused || !card.current) return;
    const reduced = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
    card.current.scrollIntoView?.({ block: "center", behavior: reduced ? "auto" : "smooth" });
    card.current.focus({ preventScroll: true });
  }, [focused]);
  const t = useT();
  const terminals = useTerminals();
  const makeDefault = useAction((api) => api.setDefaultAccount(account.id));
  const remove = useAction((api) => api.removeAccount(account.id));
  const loggedIn = account.login.state === "loggedIn";
  const classes = [
    "card",
    "account-card",
    isDefault && "account-card--current",
    account.kind === "apiKey" && "account-card--wide",
    focused && "account-card--focus",
  ];
  const loginHint = LOGIN_HINT[account.provider];

  return (
    <article ref={card} tabIndex={-1} className={classes.filter(Boolean).join(" ")} aria-label={account.name}>
      <header className="account-card__head">
        <Mascot
          color={account.color}
          state={loggedIn ? "idle" : "sleeping"}
          size={44}
          label={t("accounts.card.mascot", { name: account.name })}
        />
        <div className="account-card__title">
          <AccountName account={account} />
          <p className="muted">
            {accountSubtitle(account, t)}
            {isDefault && <span className="pill pill--current">{t("accounts.card.default")}</span>}
          </p>
        </div>
      </header>

      <UsageBars account={account} />
      <p className="muted small">
        {account.kind === "apiKey" ? t("accounts.card.apiNote") : t("accounts.card.subscriptionNote")}
        {account.defaultFolder && t("accounts.card.defaultFolderNote")}
      </p>
      {account.kind === "subscription" && !loggedIn && loginHint && <p className="small muted">{t(loginHint)}</p>}

      {projects.length > 0 && (
        <ul className="pills" aria-label={t("common.projects")}>
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
            {loggedIn ? t("accounts.card.reconnect") : t("accounts.card.signIn")}
          </Button>
        ) : (
          <Button small variant={loggedIn ? "ghost" : "primary"} onClick={onApiKey}>
            {loggedIn ? t("accounts.card.changeKey") : t("accounts.card.saveKey")}
          </Button>
        )}
        {!isDefault && (
          <Button small variant="ghost" onClick={() => makeDefault.mutate(undefined)}>
            {t("accounts.card.makeDefault")}
          </Button>
        )}
        <ConfirmButton
          small
          title={t("accounts.card.removeHint")}
          confirmLabel={t("accounts.card.removeConfirm")}
          onConfirm={() => remove.mutate(undefined)}
        >
          {t("accounts.card.remove")}
        </ConfirmButton>
      </footer>
    </article>
  );
}
