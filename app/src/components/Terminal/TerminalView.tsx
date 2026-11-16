import { useEffect, useRef } from "react";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";

import { useApi } from "../../api/context";
import type { TerminalRequest } from "../../api/types";
import { useT } from "../../i18n";
import { useThemeTokens } from "../../theme/ThemeProvider";
import { terminalTheme } from "../../theme/tokens";

function decode(base64: string): Uint8Array {
  return Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
}

interface TerminalViewProps {
  request: TerminalRequest;
  active: boolean;
  onExit: (code: number | null) => void;
}

/** One xterm.js instance bound to one backend PTY for its whole lifetime. */
export function TerminalView({ request, active, onExit }: TerminalViewProps) {
  const api = useApi();
  const theme = useThemeTokens();
  const host = useRef<HTMLDivElement>(null);
  const term = useRef<Terminal | null>(null);
  const fit = useRef<FitAddon | null>(null);
  const t = useT();
  const onExitRef = useRef(onExit);
  const tRef = useRef(t);
  useEffect(() => {
    onExitRef.current = onExit;
    tRef.current = t;
  });

  useEffect(() => {
    const element = host.current;
    if (!element) return;
    const xterm = new Terminal({
      fontFamily: "'JetBrains Mono Variable', monospace",
      fontSize: 13,
      cursorBlink: true,
      theme: terminalTheme(theme),
      allowProposedApi: false,
    });
    const fitAddon = new FitAddon();
    xterm.loadAddon(fitAddon);
    xterm.open(element);
    term.current = xterm;
    fit.current = fitAddon;

    let id: number | null = null;
    let disposed = false;
    // Deferred so React's StrictMode double mount never starts two processes
    // (a duplicated login would open the browser twice).
    const start = setTimeout(async () => {
      fitAddon.fit();
      try {
        const opened = await api.terminalOpen(request, { cols: xterm.cols, rows: xterm.rows }, (event) => {
          if (event.event === "data") {
            xterm.write(decode(event.data));
          } else {
            const exited = tRef.current("shell.processExited");
            const code = event.code === null ? "" : ` · ${tRef.current("shell.exitCode", { code: event.code })}`;
            xterm.write(`\r\n\x1b[2m[${exited}${code}]\x1b[0m\r\n`);
            onExitRef.current(event.code);
          }
        });
        if (disposed) {
          void api.terminalClose(opened);
          return;
        }
        id = opened;
      } catch (error) {
        xterm.write(`\x1b[31m${String(error)}\x1b[0m\r\n`);
        onExitRef.current(null);
      }
    }, 0);

    const input = xterm.onData((data) => id !== null && void api.terminalWrite(id, data));
    const resize = xterm.onResize(({ cols, rows }) => id !== null && void api.terminalResize(id, { cols, rows }));
    const observer = new ResizeObserver(() => element.offsetParent && fitAddon.fit());
    observer.observe(element);

    return () => {
      disposed = true;
      clearTimeout(start);
      observer.disconnect();
      input.dispose();
      resize.dispose();
      xterm.dispose();
      if (id !== null) void api.terminalClose(id);
    };
    // The session is bound to its first request; theme changes are applied below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [api]);

  useEffect(() => {
    if (term.current) term.current.options.theme = terminalTheme(theme);
  }, [theme]);

  useEffect(() => {
    if (!active) return;
    fit.current?.fit();
    term.current?.focus();
  }, [active]);

  return <div ref={host} className="terminal-view" hidden={!active} />;
}
