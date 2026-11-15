// A read-only unified diff inside the viewer ("comparar con la actual", the
// folder's own changes): virtualized lines with soft tints, old and new numbers.

import type { CodeDiff } from "../../api/codeTypes";
import type { DiffLine } from "../../api/types";
import { VirtualList } from "../../components/ui/VirtualList";
import { useT } from "../../i18n";
import { CodeState, formatBytes } from "./states";

const ROW = 20;

type Row = { kind: "hunk"; header: string } | { kind: "line"; line: DiffLine };

export function InlineDiff({
  diff,
  onOpenLine,
  caption,
}: {
  diff: CodeDiff;
  onOpenLine?: (line: number) => void;
  /** What the two sides are, next to the counts. */
  caption?: string;
}) {
  const t = useT();
  const body = diff.body;
  if (body.kind === "binary") return <CodeState title={t("code.viewer.binary")} text={t("code.diff.binary")} />;
  if (body.kind === "tooLarge") return <CodeState title={t("code.viewer.tooLarge")} text={formatBytes(body.size)} />;
  if (body.kind === "locked") return <CodeState tone="locked" title={t("code.locked.title")} text={t("code.locked.text")} />;
  if (body.hunks.length === 0) return <CodeState title={t("code.diff.same")} />;

  const rows: Row[] = [];
  for (const hunk of body.hunks) {
    rows.push({ kind: "hunk", header: hunk.header });
    for (const line of hunk.lines) rows.push({ kind: "line", line });
  }

  return (
    <div className="inline-diff">
      <p className="inline-diff__stats">
        <span className="plus">+{body.additions}</span> <span className="minus">-{body.deletions}</span>
        {caption && <span className="inline-diff__caption">{caption}</span>}
      </p>
      <VirtualList
        className="inline-diff__lines"
        role="table"
        aria-label={t("code.diff.label", { path: diff.path })}
        count={rows.length}
        rowHeight={ROW}
        renderRow={(index) => {
          const row = rows[index];
          if (!row) return null;
          if (row.kind === "hunk") {
            return (
              <div key={index} role="row" className="inline-diff__hunk" style={{ height: ROW }}>
                <span role="cell">{row.header}</span>
              </div>
            );
          }
          const { line } = row;
          const sign = line.kind === "added" ? "+" : line.kind === "removed" ? "−" : " ";
          return (
            <div
              key={index}
              role="row"
              className={`inline-diff__line inline-diff__line--${line.kind}`}
              style={{ height: ROW }}
              onDoubleClick={() => line.newLine && onOpenLine?.(line.newLine)}
            >
              <span role="cell" className="inline-diff__no">
                {line.oldLine ?? ""}
              </span>
              <span role="cell" className="inline-diff__no">
                {line.newLine ?? ""}
              </span>
              <span role="cell" className="inline-diff__sign" aria-hidden>
                {sign}
              </span>
              <span role="cell" className="inline-diff__text">
                {line.kind !== "context" && <span className="sr-only">{t(line.kind === "added" ? "code.diff.added" : "code.diff.removed")} </span>}
                {line.text}
              </span>
            </div>
          );
        }}
      />
    </div>
  );
}
