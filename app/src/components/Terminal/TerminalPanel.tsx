import { useQueryClient } from "@tanstack/react-query";

import { OVERVIEW_KEY } from "../../api/context";
import { useT } from "../../i18n";
import { Button } from "../ui/Button";
import { useTerminals } from "./TerminalProvider";
import { TerminalView } from "./TerminalView";

export function TerminalPanel() {
  const { sessions, activeKey, panelOpen, open, close, activate, markExited, setPanelOpen } = useTerminals();
  const queryClient = useQueryClient();
  const t = useT();

  if (sessions.length === 0) return null;

  return (
    <section className="terminal-panel" hidden={!panelOpen} aria-label={t("common.terminal")}>
      <div className="terminal-panel__bar" role="tablist" aria-label={t("shell.terminals")}>
        {sessions.map((s) => (
          <div key={s.key} className={`terminal-tab${s.key === activeKey ? " terminal-tab--active" : ""}`}>
            <button
              role="tab"
              aria-selected={s.key === activeKey}
              className="terminal-tab__title"
              onClick={() => activate(s.key)}
            >
              <span className={`dot ${s.running ? "dot--running" : "dot--idle"}`} aria-hidden />
              {s.title}
            </button>
            <button className="terminal-tab__close" aria-label={t("common.closeNamed", { name: s.title })} onClick={() => close(s.key)}>
              ×
            </button>
          </div>
        ))}
        <Button small variant="ghost" onClick={() => open({ kind: "shell", path: null }, t("common.terminal"))}>
          + {t("common.terminal")}
        </Button>
        <span className="spacer" />
        <Button small variant="ghost" onClick={() => setPanelOpen(false)} aria-label={t("shell.hideTerminal")}>
          {t("shell.hide")}
        </Button>
      </div>
      <div className="terminal-panel__body">
        {sessions.map((s) => (
          <TerminalView
            key={s.key}
            request={s.request}
            active={panelOpen && s.key === activeKey}
            onExit={() => {
              markExited(s.key);
              // A finished login changes the account's state.
              void queryClient.invalidateQueries({ queryKey: OVERVIEW_KEY });
            }}
          />
        ))}
      </div>
    </section>
  );
}
