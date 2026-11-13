import type { ReactNode } from "react";

import type { Account, Overview } from "../api/types";
import { shortAccountLabel } from "./accounts";
import { Mascot } from "./Mascot/Mascot";
import { projectColor, useProjectActions } from "./projects";
import { useTerminals } from "./Terminal/TerminalProvider";
import { Button } from "./ui/Button";

export type Screen = "welcome" | "accounts" | "settings";

export const SCREENS: { screen: Screen; label: string }[] = [
  { screen: "welcome", label: "Inicio" },
  { screen: "accounts", label: "Cuentas y modelos" },
  { screen: "settings", label: "Ajustes" },
];

export function defaultAccount(overview: Overview): Account | undefined {
  return overview.accounts.find((a) => a.id === overview.defaultAccount) ?? overview.accounts[0];
}

export function Topbar({
  overview,
  screen,
  onNavigate,
}: {
  overview: Overview;
  screen: Screen;
  onNavigate: (s: Screen) => void;
}) {
  const terminals = useTerminals();
  const running = terminals.sessions.some((s) => s.running);
  const color = defaultAccount(overview)?.color ?? "#7AA2F7";

  return (
    <header className="topbar">
      <div className="brand">
        <Mascot color={color} state={running ? "working" : "idle"} size={30} label="Hedy" />
        <span className="brand__name">Hedy</span>
      </div>
      <nav className="tabs" aria-label="Secciones">
        {SCREENS.map((item) => (
          <button
            key={item.screen}
            className="tab"
            aria-current={item.screen === screen ? "page" : undefined}
            onClick={() => onNavigate(item.screen)}
          >
            {item.label}
          </button>
        ))}
      </nav>
      <span className="spacer" />
      {overview.accounts.length > 0 && (
        <div className="usage-pill" aria-label="Consumo por proveedor: aún sin datos">
          {providerSummary(overview).map(({ label, color }) => (
            <span key={label} className="usage-pill__item">
              <span className="dot" style={{ background: color }} aria-hidden />
              {label} <span className="muted">—</span>
            </span>
          ))}
        </div>
      )}
      <Button
        variant="secondary"
        small
        onClick={() =>
          terminals.sessions.length === 0
            ? terminals.open({ kind: "shell", path: null }, "Terminal")
            : terminals.setPanelOpen(!terminals.panelOpen)
        }
        aria-pressed={terminals.panelOpen}
      >
        Terminal{terminals.sessions.length > 0 ? ` · ${terminals.sessions.length}` : ""}
      </Button>
    </header>
  );
}

const PROVIDER_LABEL: Record<Account["provider"], string> = {
  claude: "Claude",
  codex: "ChatGPT",
  gemini: "Gemini",
  kimi: "Kimi",
};

/** One entry per provider, colored like the default (or first) account of it. */
function providerSummary(overview: Overview): { label: string; color: string }[] {
  const seen = new Map<string, string>();
  const preferred = [...overview.accounts].sort((a) => (a.id === overview.defaultAccount ? -1 : 0));
  for (const a of preferred) {
    const label = PROVIDER_LABEL[a.provider];
    if (!seen.has(label)) seen.set(label, a.color);
  }
  return [...seen].map(([label, color]) => ({ label, color }));
}

export function Sidebar({ overview, onNavigate }: { overview: Overview; onNavigate: (s: Screen) => void }) {
  const actions = useProjectActions();
  const accountById = new Map(overview.accounts.map((a) => [a.id, a]));
  const tipColor = defaultAccount(overview)?.color ?? "#BB9AF7";

  return (
    <aside className="sidebar" aria-label="Proyectos y cuentas">
      <section>
        <h2 className="sidebar__title">Proyectos</h2>
        {overview.projects.length === 0 && <p className="hint sidebar__empty">Sin proyectos todavía.</p>}
        <ul className="sidebar__list">
          {overview.projects.map((p) => {
            const account = p.account ? accountById.get(p.account) : undefined;
            return (
              <li key={p.path}>
                <button
                  className="sidebar__item"
                  onClick={() => actions.openCli(p)}
                  title={account?.kind === "subscription" ? `Abrir ${account.providerName} con ${account.name}` : undefined}
                  disabled={account?.kind !== "subscription"}
                >
                  <span className="dot" style={{ background: projectColor(p, overview.accounts) }} aria-hidden />
                  <span className="sidebar__name">{p.name}</span>
                  {account && (
                    <span className={`sidebar__meta${p.protected ? " sidebar__meta--protected" : ""}`}>
                      {shortAccountLabel(account)}
                    </span>
                  )}
                </button>
              </li>
            );
          })}
        </ul>
      </section>

      <section>
        <h2 className="sidebar__title">Cuentas</h2>
        {overview.accounts.length === 0 && (
          <Button small variant="primary" onClick={() => onNavigate("accounts")}>
            Agregar cuenta
          </Button>
        )}
        <ul className="sidebar__list">
          {overview.accounts.map((a) => (
            <li key={a.id} className="sidebar__account">
              <div className="row row--between">
                <span className="sidebar__account-name">{a.name}</span>
                <span className="muted small">—</span>
              </div>
              <div className="bar bar--thin" role="meter" aria-label={`${a.name}: sin datos de consumo`} aria-valuemin={0} aria-valuemax={100} aria-valuenow={0}>
                <div className="bar__fill" style={{ width: 0, background: a.color }} />
              </div>
            </li>
          ))}
        </ul>
      </section>

      <span className="spacer" />
      <div className="tip" role="note">
        <Mascot color={tipColor} state="idle" size={18} label="" />
        <p>
          {overview.accounts.length === 0
            ? "Agrega una cuenta y te ayudo a repartir el trabajo."
            : "Aún no llevo la cuenta de tu consumo. Cuando trabajemos desde el chat te diré cómo vas."}
        </p>
      </div>
    </aside>
  );
}

export function Shell({
  topbar,
  sidebar,
  children,
  terminal,
}: {
  topbar: ReactNode;
  sidebar: ReactNode;
  children: ReactNode;
  terminal: ReactNode;
}) {
  return (
    <div className="app">
      {topbar}
      {sidebar}
      <main className="main" id="main">
        {children}
      </main>
      {terminal}
    </div>
  );
}
