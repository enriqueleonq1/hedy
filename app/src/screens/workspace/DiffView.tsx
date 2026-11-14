import { useEffect, useMemo, useState } from "react";
import type { ThemedToken } from "shiki/core";

import type { DiffLine, FileDiff } from "../../api/types";
import { useThemeName } from "../../theme/ThemeProvider";
import { useT } from "../../i18n";
import { highlightLines } from "./highlight";

const SIGN: Record<DiffLine["kind"], string> = { added: "+", removed: "-", context: " " };

/** A file's hunks with line numbers, +/- and syntax colors (when available). */
export function DiffView({ file }: { file: FileDiff }) {
  const t = useT();
  const theme = useThemeName();
  const lines = useMemo(() => file.hunks.flatMap((h) => h.lines), [file]);
  const [tokens, setTokens] = useState<{ key: FileDiff; theme: string; lines: ThemedToken[][] } | null>(null);

  useEffect(() => {
    let cancelled = false;
    highlightLines(
      lines.map((l) => l.text),
      file.path,
      theme,
    )
      .then((result) => !cancelled && result && setTokens({ key: file, theme, lines: result }))
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [file, lines, theme]);

  if (file.binary) return <p className="empty">{t("workspace.diff.binary")}</p>;
  if (lines.length === 0) return <p className="empty">{t("workspace.diff.empty")}</p>;

  // Ignore tokens computed for another file or theme.
  const colored = tokens?.key === file && tokens.theme === theme ? tokens.lines : null;
  let index = 0;
  const signLabel: Record<DiffLine["kind"], string | undefined> = {
    added: t("workspace.diff.added"),
    removed: t("workspace.diff.removed"),
    context: undefined,
  };

  return (
    <div className="diff" role="table" aria-label={t("workspace.diff.of", { path: file.path })}>
      {file.hunks.map((hunk) => (
        <div key={hunk.header} role="rowgroup">
          <div className="diff__hunk" role="row">
            <span role="cell">{hunk.header}</span>
          </div>
          {hunk.lines.map((line) => {
            const lineTokens = colored?.[index++];
            return (
              <div key={`${line.oldLine}-${line.newLine}-${line.kind}`} className={`diff__line diff__line--${line.kind}`} role="row">
                <span className="diff__no" role="cell">
                  {line.kind === "removed" ? line.oldLine : line.newLine}
                </span>
                <span className="diff__sign" role="cell" aria-label={signLabel[line.kind]}>
                  {SIGN[line.kind]}
                </span>
                <code className="diff__code" role="cell">
                  {lineTokens
                    ? lineTokens.map((token, i) => (
                        <span key={i} style={{ color: token.color }}>
                          {token.content}
                        </span>
                      ))
                    : line.text}
                </code>
              </div>
            );
          })}
        </div>
      ))}
    </div>
  );
}
