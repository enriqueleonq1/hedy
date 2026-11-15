import { useEffect, useRef, useState } from "react";

import { LOGIN_HINT } from "../../components/providers";
import { useAction } from "../../api/context";
import type { Account, Overview } from "../../api/types";
import { accountSubtitle, LoginStatus, UsageBars, usageSource } from "../../components/accounts";
import { Mascot } from "../../components/Mascot/Mascot";
import { useTerminals } from "../../components/Terminal/TerminalProvider";
import { Button } from "../../components/ui/Button";
import { Dialog } from "../../components/ui/Dialog";
import { MenuButton, type MenuItem } from "../../components/ui/Menu";
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
          {/* Said once for every card: where the figures come from. */}
          <p className="page-head__note">{t("accounts.note")}</p>
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

/**
 * The name, renamed in place: from the pencil (shown on hover or focus), a
 * double-click on the name or the card's menu. Enter or leaving saves, Escape
 * cancels; Enter and Escape give the focus back to the pencil.
 */
function AccountName({
  account,
  editing,
  setEditing,
}: {
  account: Account;
  editing: boolean;
  setEditing: (editing: boolean) => void;
}) {
  const t = useT();
  const rename = useAction((api, name: string) => api.renameAccount(account.id, name));
  const wrap = useRef<HTMLDivElement>(null);
  const refocus = useRef(false);
  useEffect(() => {
    if (editing || !refocus.current) return;
    refocus.current = false;
    wrap.current?.querySelector<HTMLButtonElement>(".account-card__rename")?.focus();
  }, [editing]);

  if (!editing) {
    return (
      <div ref={wrap} className="account-card__name">
        <h3 title={t("accounts.card.renameHint")} onDoubleClick={() => setEditing(true)}>
          {account.name}
        </h3>
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
        if (e.key === "Enter") {
          refocus.current = true;
          e.currentTarget.blur();
        }
        if (e.key === "Escape") {
          refocus.current = true;
          setEditing(false);
        }
      }}
    />
  );
}

/** Asks before deleting: Hedy stops using the account, its login folder stays. */
function RemoveAccountDialog({
  account,
  onConfirm,
  onClose,
}: {
  account: Account;
  onConfirm: () => void;
  onClose: () => void;
}) {
  const t = useT();
  return (
    <Dialog
      title={t("accounts.card.removeTitle", { name: account.name })}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button
            variant="danger"
            onClick={() => {
              onClose();
              onConfirm();
            }}
          >
            {t("accounts.card.remove")}
          </Button>
        </>
      }
    >
      <p>{t("accounts.card.removeHint")}.</p>
    </Dialog>
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
  const [editing, setEditing] = useState(false);
  const [removing, setRemoving] = useState(false);
  const loggedIn = account.login.state === "loggedIn";
  const subscription = account.kind === "subscription";
  const classes = [
    "card",
    "account-card",
    isDefault && "account-card--current",
    account.kind === "apiKey" && "account-card--wide",
    focused && "account-card--focus",
  ];
  const loginHint = LOGIN_HINT[account.provider];
  const login = () => terminals.open({ kind: "login", account: account.id }, `Login · ${account.name}`);
  const source = usageSource(account, t);

  // The one action the card needs stays visible (signing in, saving the key);
  // the rest live in the menu, with deleting set apart at the end.
  const items: MenuItem[] = [
    { label: t("accounts.card.renameItem"), onSelect: () => setEditing(true) },
    ...(isDefault ? [] : [{ label: t("accounts.card.makeDefault"), onSelect: () => makeDefault.mutate(undefined) }]),
    ...(!loggedIn
      ? []
      : subscription
        ? [{ label: t("accounts.card.reconnect"), onSelect: login }]
        : [{ label: t("accounts.card.changeKey"), onSelect: onApiKey }]),
    { label: t("accounts.card.remove"), onSelect: () => setRemoving(true), danger: true, separatorBefore: true },
  ];

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
          <AccountName account={account} editing={editing} setEditing={setEditing} />
          <p className="account-card__sub">
            <span>{accountSubtitle(account, t)}</span>
            <LoginStatus account={account} />
            {isDefault && <span className="pill pill--current">{t("accounts.card.default")}</span>}
          </p>
        </div>
        <MenuButton label={t("accounts.card.actions", { name: account.name })} items={items} />
      </header>

      <UsageBars account={account} />

      {projects.length > 0 && (
        <ul className="pills" aria-label={t("common.projects")}>
          {projects.map((p) => (
            <li key={p.path} className="pill">
              {p.name}
            </li>
          ))}
        </ul>
      )}

      {!loggedIn && (
        <div className="account-card__action">
          {subscription && loginHint && <p className="small muted">{t(loginHint)}</p>}
          {subscription ? (
            <Button small variant="primary" onClick={login}>
              {t("accounts.card.signIn")}
            </Button>
          ) : (
            <Button small variant="primary" onClick={onApiKey}>
              {t("accounts.card.saveKey")}
            </Button>
          )}
        </div>
      )}

      {(source || account.defaultFolder) && (
        <footer className="account-card__foot">
          {source && <p>{source}</p>}
          {account.defaultFolder && <p>{t("accounts.card.defaultFolderNote")}</p>}
        </footer>
      )}

      {removing && (
        <RemoveAccountDialog
          account={account}
          onConfirm={() => remove.mutate(undefined)}
          onClose={() => setRemoving(false)}
        />
      )}
    </article>
  );
}
