import { Fragment, useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import type { ThemedToken } from "shiki/core";

import { useApi } from "../../api/context";
import type { CodeCopy } from "../../api/codeTypes";
import type { DiffLine, FileDiff } from "../../api/types";
import { ContextMenu, isMenuKey, pointFor, type MenuPoint } from "../../components/ui/ContextMenu";
import type { MenuItem } from "../../components/ui/Menu";
import { codeKeys, sameCopy, useFileNavigator } from "../../files/navigator";
import { useT } from "../../i18n";
import { CommentComposer, CommentThread } from "../../review/CommentThread";
import { useReviewComments, type ReviewComment } from "../../review/comments";
import {
  defaultVisible,
  gapSize,
  hunkSegments,
  indexOfLine,
  lineNumber,
  pairChanges,
  revealGap,
  segments as buildSegments,
  selectionTarget,
  sideOf,
  splitRows,
  wordDiff,
  EXPAND_STEP,
  type Range,
  type Segment,
  type Side,
  type SplitRow,
} from "../../review/diffModel";
import { useThemeName } from "../../theme/ThemeProvider";
import { highlightLines, highlightText } from "./highlight";

export type DiffLayout = "unified" | "split";

const SIGN: Record<DiffLine["kind"], string> = { added: "+", removed: "−", context: " " };
/** Past this many lines a file is not highlighted (it would take too long). */
const MAX_HIGHLIGHT = 5000;

type Gap = Extract<Segment, { kind: "gap" }>;
type Selection = { side: Side; anchor: number; head: number };
type Composing = { side: Side; line: number; endLine: number | null; code: string; at: number };

/** The whole file as context, when it is the same change as the task's diff; else the task's hunks. */
function useLines(copy: CodeCopy, file: FileDiff) {
  const api = useApi();
  const full = useQuery({
    queryKey: codeKeys.diff(copy, file.path, "base", true),
    queryFn: () => api.fileDiff(copy, file.path, { against: "base", full: true }),
    enabled: !file.binary,
    retry: false,
  });
  return useMemo(() => {
    const body = full.data?.body;
    if (body && body.kind !== "text") return { kind: body.kind, data: full.data } as const;
    const changed = body?.hunks.some((h) => h.lines.some((l) => l.kind !== "context"));
    // The full diff is used when it shows a change (or the task's diff shows none either).
    if (body && (changed || file.hunks.length === 0)) {
      return { kind: "full", lines: body.hunks.flatMap((h) => h.lines) } as const;
    }
    return { kind: "hunks", ...hunkSegments(file.hunks) } as const;
  }, [full.data, file.hunks]);
}

/** Syntax colors per line index (each side highlighted as a whole file when it is known). */
function useTokens(lines: DiffLine[], whole: boolean, path: string) {
  const theme = useThemeName();
  const [state, setState] = useState<{ lines: DiffLine[]; theme: string; tokens: Map<number, ThemedToken[]> } | null>(null);

  useEffect(() => {
    let cancelled = false;
    if (lines.length === 0 || lines.length > MAX_HIGHLIGHT) return;
    const done = (tokens: Map<number, ThemedToken[]>) => !cancelled && setState({ lines, theme, tokens });
    if (whole) {
      const old = lines.filter((l) => l.kind !== "added");
      const now = lines.filter((l) => l.kind !== "removed");
      Promise.all([
        highlightText(old.map((l) => l.text).join("\n"), path, theme),
        highlightText(now.map((l) => l.text).join("\n"), path, theme),
      ])
        .then(([before, after]) => {
          if (!before || !after) return;
          const map = new Map<number, ThemedToken[]>();
          let o = 0;
          let n = 0;
          lines.forEach((l, i) => {
            const tokens = l.kind === "added" ? after[n++] : l.kind === "removed" ? before[o++] : (o++, after[n++]);
            if (tokens) map.set(i, tokens);
          });
          done(map);
        })
        .catch(() => {});
    } else {
      highlightLines(
        lines.map((l) => l.text),
        path,
        theme,
      )
        .then((result) => result && done(new Map(result.map((tokens, i) => [i, tokens]))))
        .catch(() => {});
    }
    return () => {
      cancelled = true;
    };
  }, [lines, whole, path, theme]);

  // Ignore tokens computed for other lines or another theme.
  return state?.lines === lines && state.theme === theme ? state.tokens : null;
}

/** A line's code with its syntax colors and the changed words emphasized. */
function Code({ text, tokens, emphasis }: { text: string; tokens?: ThemedToken[]; emphasis?: Range[] }) {
  const pieces: { text: string; color?: string }[] =
    tokens && tokens.reduce((n, tk) => n + tk.content.length, 0) === text.length
      ? tokens.map((tk) => ({ text: tk.content, color: tk.color }))
      : [{ text }];
  if (text === "") return <span className="rd-empty"> </span>;
  const out: ReactNode[] = [];
  let offset = 0;
  pieces.forEach((piece, p) => {
    const start = offset;
    const end = offset + piece.text.length;
    offset = end;
    // Cut the piece where an emphasized stretch starts or ends.
    const cuts = new Set([start, end]);
    for (const r of emphasis ?? []) {
      if (r.start > start && r.start < end) cuts.add(r.start);
      if (r.end > start && r.end < end) cuts.add(r.end);
    }
    const points = [...cuts].sort((a, b) => a - b);
    points.slice(0, -1).forEach((from, k) => {
      const to = points[k + 1] ?? end;
      const content = piece.text.slice(from - start, to - start);
      const style = piece.color ? { color: piece.color } : undefined;
      const emphasized = emphasis?.some((r) => r.start <= from && r.end >= to);
      out.push(
        emphasized ? (
          <mark key={`${p}-${k}`} className="rd-emph" style={style}>
            {content}
          </mark>
        ) : (
          <span key={`${p}-${k}`} style={style}>
            {content}
          </span>
        ),
      );
    });
  });
  return <>{out}</>;
}

/**
 * One file's changes for review: unified or side by side, syntax colors,
 * changed words emphasized, context that expands between the changes, and
 * comments on lines (the "+" in the gutter, a drag or Shift+click for a range,
 * the right-click menu, or the keyboard: arrows move, Shift+arrows select, C or
 * Enter comments).
 */
export function DiffView({
  copy,
  file,
  layout = "unified",
  focusLine = null,
  focusSeq = 0,
}: {
  copy: CodeCopy;
  file: FileDiff;
  layout?: DiffLayout;
  /** A line of the new side to show (and move to) when `focusSeq` changes. */
  focusLine?: number | null;
  focusSeq?: number;
}) {
  const t = useT();
  const api = useApi();
  const nav = useFileNavigator();
  const review = useReviewComments();
  const source = useLines(copy, file);
  const lines = useMemo(() => (source.kind === "full" || source.kind === "hunks" ? source.lines : []), [source]);
  const full = source.kind === "full";
  const tokens = useTokens(lines, full, file.path);
  const pairs = useMemo(() => pairChanges(lines), [lines]);
  const emphasis = useMemo(() => {
    const map = new Map<number, Range[]>();
    for (const [a, b] of pairs) {
      const line = lines[a];
      const other = lines[b];
      if (line?.kind !== "removed" || !other) continue;
      const diff = wordDiff(line.text, other.text);
      if (!diff) continue;
      map.set(a, diff.old);
      map.set(b, diff.new);
    }
    return map;
  }, [pairs, lines]);

  const comments = useMemo(
    () => review.commentsFor(copy, file.path).filter((c) => sameCopy(c.copy, copy)),
    [review, copy, file.path],
  );
  const anchorOf = useCallback(
    (c: ReviewComment) => indexOfLine(lines, c.side, c.endLine ?? c.line),
    [lines],
  );

  // What is shown: the context around changes, what was expanded, commented lines.
  const [revealed, setRevealed] = useState<{ lines: DiffLine[]; set: Set<number> }>({ lines, set: new Set() });
  const extra = revealed.lines === lines ? revealed.set : null;
  const focusIndex = focusLine !== null ? indexOfLine(lines, "new", focusLine) : -1;
  const visible = useMemo(() => {
    const set = defaultVisible(lines);
    for (const i of extra ?? []) set.add(i);
    for (const c of comments) {
      const at = anchorOf(c);
      if (at >= 0) set.add(at);
    }
    if (focusIndex >= 0) set.add(focusIndex);
    return set;
  }, [lines, extra, comments, anchorOf, focusIndex]);
  const segs = useMemo(
    () => (source.kind === "full" ? buildSegments(lines, visible) : source.kind === "hunks" ? source.segments : []),
    [source, lines, visible],
  );
  const expand = (gap: Gap, mode: "up" | "down" | "all") =>
    setRevealed({ lines, set: revealGap(extra ?? new Set(), gap, mode) });

  // Rows in display order: one per line (unified) or per pair of lines (split).
  const split = layout === "split";
  const rowsOf = useCallback(
    (seg: Extract<Segment, { kind: "lines" }>): SplitRow[] =>
      split
        ? splitRows(lines, seg.from, seg.to)
        : Array.from({ length: seg.to - seg.from + 1 }, (_, k) => ({ left: seg.from + k, right: seg.from + k })),
    [split, lines],
  );
  const allRows = useMemo(
    () => segs.flatMap((s) => (s.kind === "lines" ? rowsOf(s) : [])),
    [segs, rowsOf],
  );

  const [cursor, setCursor] = useState<{ index: number; side: Side } | null>(null);
  const [selection, setSelection] = useState<Selection | null>(null);
  const [composing, setComposing] = useState<Composing | null>(null);
  const [menu, setMenu] = useState<{ at: MenuPoint; index: number; side: Side } | null>(null);
  const drag = useRef<Selection | null>(null);
  const table = useRef<HTMLDivElement>(null);

  // A new file (or new lines) starts clean.
  const [shownFor, setShownFor] = useState(lines);
  if (shownFor !== lines) {
    setShownFor(lines);
    setCursor(null);
    setSelection(null);
    setComposing(null);
  }

  /** The index of a row's line on a side (falls back to the other side when there is none). */
  const cellOf = (row: SplitRow, side: Side): { index: number; side: Side } | null => {
    if (!split) {
      const line = lines[row.left ?? -1];
      return line && row.left !== null ? { index: row.left, side: sideOf(line) } : null;
    }
    const own = side === "old" ? row.left : row.right;
    if (own !== null) return { index: own, side };
    const other = side === "old" ? row.right : row.left;
    return other !== null ? { index: other, side: side === "old" ? "new" : "old" } : null;
  };
  const rowIndexOf = (index: number, side: Side) =>
    allRows.findIndex((r) => (split ? (side === "old" ? r.left : r.right) === index : r.left === index));

  // The row Tab lands on: where the cursor is, else the first change.
  const firstRow =
    allRows.find((r) => [r.left, r.right].some((i) => i !== null && lines[i]?.kind !== "context")) ?? allRows[0];
  const tabbable = cursor ?? (firstRow ? cellOf(firstRow, "new") : null);

  const focusRow = (index: number, side: Side) => {
    const find = () => table.current?.querySelector<HTMLElement>(`[data-row="${split ? `${side}-${index}` : index}"]`);
    const row = find();
    // Not rendered yet (a line just revealed): once it is.
    if (row) row.focus();
    else requestAnimationFrame(() => find()?.focus());
  };

  // `selectFile(path, line)`: show that line and move there.
  const [focusSeen, setFocusSeen] = useState(focusSeq);
  if (focusSeen !== focusSeq) {
    setFocusSeen(focusSeq);
    if (focusIndex >= 0) setCursor({ index: focusIndex, side: "new" });
  }
  useEffect(() => {
    if (focusSeq === 0 || focusIndex < 0) return;
    requestAnimationFrame(() => {
      const row = table.current?.querySelector<HTMLElement>(`[data-line-new="${focusLine}"]`);
      row?.scrollIntoView?.({ block: "center" });
      row?.classList.add("rd-row--flash");
    });
  }, [focusSeq, focusIndex, focusLine]);

  const selected = (index: number, side: Side) => {
    const range = composing ? null : selection;
    if (range) {
      if (range.side !== side && split) return false;
      const line = lines[index];
      if (!line || lineNumber(line, range.side) === null) return false;
      return index >= Math.min(range.anchor, range.head) && index <= Math.max(range.anchor, range.head);
    }
    if (composing) {
      if (split && composing.side !== side) return false;
      const n = lines[index] ? lineNumber(lines[index], composing.side) : null;
      return n !== null && n >= composing.line && n <= (composing.endLine ?? composing.line);
    }
    return false;
  };

  const openComposer = (range: Selection) => {
    const target = selectionTarget(lines, range.side, range.anchor, range.head);
    if (!target) return;
    const at = indexOfLine(lines, range.side, target.endLine ?? target.line);
    setComposing({ side: range.side, line: target.line, endLine: target.endLine, code: target.code, at });
    setSelection(null);
  };

  // A drag from the "+" ends wherever the mouse is let go.
  useEffect(() => {
    const onUp = () => {
      const range = drag.current;
      if (!range) return;
      drag.current = null;
      openComposer(range);
    };
    window.addEventListener("mouseup", onUp);
    return () => window.removeEventListener("mouseup", onUp);
  });

  const startComment = (index: number, side: Side, shift: boolean) => {
    if (shift && selection && selection.side === side) {
      openComposer({ ...selection, head: index });
      return;
    }
    const range = { side, anchor: index, head: index };
    drag.current = range;
    setSelection(range);
    setCursor({ index, side });
  };

  const extendDrag = (index: number, side: Side) => {
    if (!drag.current || drag.current.side !== side) return;
    const line = lines[index];
    if (!line || lineNumber(line, side) === null) return;
    drag.current = { ...drag.current, head: index };
    setSelection(drag.current);
  };

  const clickNumber = (index: number, side: Side, shift: boolean) => {
    if (shift && selection && selection.side === side) setSelection({ ...selection, head: index });
    else setSelection({ side, anchor: index, head: index });
    setCursor({ index, side });
  };

  const onRowKey = (e: KeyboardEvent<HTMLElement>, index: number, side: Side) => {
    if (e.target !== e.currentTarget) return;
    const at = rowIndexOf(index, side);
    if (isMenuKey(e)) {
      e.preventDefault();
      setMenu({ at: pointFor(e.currentTarget), index, side });
      return;
    }
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const step = e.key === "ArrowDown" ? 1 : -1;
      let k = at + step;
      let next: { index: number; side: Side } | null = null;
      // With Shift the selection stays on its side: skip lines that are not there.
      while (k >= 0 && k < allRows.length) {
        const row = allRows[k];
        const cell = row ? cellOf(row, side) : null;
        const line = cell ? lines[cell.index] : undefined;
        if (cell && line && (!e.shiftKey || lineNumber(line, side) !== null)) {
          next = e.shiftKey ? { index: cell.index, side } : cell;
          break;
        }
        k += step;
      }
      if (!next) return;
      if (e.shiftKey) setSelection((s) => (s && s.side === side ? { ...s, head: next.index } : { side, anchor: index, head: next.index }));
      else setSelection(null);
      setCursor(next);
      focusRow(next.index, next.side);
      return;
    }
    if (split && (e.key === "ArrowLeft" || e.key === "ArrowRight")) {
      const row = allRows[at];
      const want: Side = e.key === "ArrowLeft" ? "old" : "new";
      const cell = row ? (want === "old" ? row.left : row.right) : null;
      if (cell === null || cell === undefined) return;
      e.preventDefault();
      setSelection(null);
      setCursor({ index: cell, side: want });
      focusRow(cell, want);
      return;
    }
    if (e.key === "Enter" || ((e.key === "c" || e.key === "C") && !e.ctrlKey && !e.metaKey && !e.altKey)) {
      e.preventDefault();
      openComposer(selection && selection.side === side ? selection : { side, anchor: index, head: index });
      return;
    }
    if (e.key === "Escape" && selection) {
      e.preventDefault();
      e.stopPropagation();
      setSelection(null);
    }
  };

  const menuItems = (index: number, side: Side): MenuItem[] => {
    const range = selection && selection.side === side && selected(index, side) ? selection : { side, anchor: index, head: index };
    const target = selectionTarget(lines, range.side, range.anchor, range.head);
    const line = lines[index];
    if (!target || !line) return [];
    const many = target.endLine !== null;
    const newLine = line.newLine ?? null;
    return [
      {
        label: many
          ? t("reviewer.line.commentRange", { from: target.line, to: target.endLine ?? target.line })
          : t("reviewer.line.comment", { n: target.line }),
        shortcut: "C",
        onSelect: () => openComposer(range),
      },
      {
        label: many ? t("reviewer.line.copyRange") : t("reviewer.line.copy"),
        onSelect: () => void navigator.clipboard?.writeText(target.code).catch(() => {}),
      },
      {
        label: t("reviewer.line.openFile"),
        separatorBefore: true,
        onSelect: () =>
          nav.open(
            side === "old"
              ? { copy, path: file.path, line: target.line, rev: "base", view: "file" }
              : { copy, path: file.path, line: target.line, view: "file" },
          ),
      },
      {
        label: t("edit.here"),
        disabled: file.status === "deleted",
        onSelect: () => nav.open({ copy, path: file.path, line: newLine ?? target.line, view: "file", edit: true }),
      },
      {
        label: t("reviewer.line.openEditor"),
        onSelect: () => void api.openFileInEditor(copy, file.path, newLine ?? target.line).catch(() => {}),
      },
    ];
  };

  if (file.binary || source.kind === "binary") return <p className="empty">{t("workspace.diff.binary")}</p>;
  if (source.kind === "locked") return <p className="empty">{t("reviewer.diff.locked")}</p>;
  if (source.kind === "tooLarge") {
    return (
      <div className="empty rd-state">
        <p>{t("reviewer.diff.tooLarge")}</p>
        <button type="button" className="btn btn--secondary btn--small" onClick={() => void api.openFileInEditor(copy, file.path)}>
          {t("common.openInVsCode")}
        </button>
      </div>
    );
  }
  if (lines.length === 0) return <p className="empty">{t("workspace.diff.empty")}</p>;

  const signLabel: Record<DiffLine["kind"], string | undefined> = {
    added: t("workspace.diff.added"),
    removed: t("workspace.diff.removed"),
    context: undefined,
  };
  const digits = String(lines.reduce((n, l) => Math.max(n, l.oldLine ?? 0, l.newLine ?? 0), 0)).length;
  const outdated = comments.filter((c) => anchorOf(c) < 0 || (!visible.has(anchorOf(c)) && !full));

  const isTabbable = (index: number, side: Side) => tabbable?.index === index && (!split || tabbable.side === side);

  /** The gutter, numbers, sign and code of one line on one side (or both, unified). */
  const lineCells = (line: DiffLine, index: number, side: Side, cellSide: Side | null) => {
    const n = lineNumber(line, side);
    const label = n === null ? undefined : side === "old" ? t("reviewer.line.addOld", { n }) : t("reviewer.line.add", { n });
    return (
      <>
        <span role="cell" className="rd-gutter">
          {n !== null && (
            <button
              type="button"
              tabIndex={-1}
              className="rd-add"
              aria-label={label}
              title={label}
              onMouseDown={(e) => {
                if (e.button !== 0) return;
                e.preventDefault();
                startComment(index, side, e.shiftKey);
              }}
            >
              +
            </button>
          )}
        </span>
        {cellSide === null ? (
          <>
            <span role="cell" className="rd-no" onMouseDown={(e) => e.button === 0 && clickNumber(index, sideOf(line), e.shiftKey)}>
              {line.kind === "added" ? "" : line.oldLine}
            </span>
            <span role="cell" className="rd-no" onMouseDown={(e) => e.button === 0 && clickNumber(index, sideOf(line), e.shiftKey)}>
              {line.kind === "removed" ? "" : line.newLine}
            </span>
          </>
        ) : (
          <span role="cell" className="rd-no" onMouseDown={(e) => e.button === 0 && clickNumber(index, side, e.shiftKey)}>
            {n}
          </span>
        )}
        <span role="cell" className="rd-sign" aria-label={signLabel[line.kind]}>
          {SIGN[line.kind]}
        </span>
        <code role="cell" className="rd-code">
          <Code text={line.text} tokens={tokens?.get(index)} emphasis={emphasis.get(index)} />
        </code>
      </>
    );
  };

  const threadFor = (list: ReviewComment[], side: Side | null) =>
    list.length > 0 && (
      <div role="row" className={`rd-thread${side ? ` rd-thread--${side}` : ""}`}>
        <div role="cell" className="rd-thread__cell">
          <CommentThread comments={list} />
        </div>
      </div>
    );

  const composerRow = (side: Side | null) =>
    composing && (
      <div role="row" className={`rd-thread${side ? ` rd-thread--${side}` : ""}`}>
        <div role="cell" className="rd-thread__cell">
          <CommentComposer
            where={`${file.path}:${composing.line}${composing.endLine ? `-${composing.endLine}` : ""}`}
            onCancel={() => {
              const back = composing;
              setComposing(null);
              focusRow(back.at, back.side);
            }}
            onSubmit={(text) => {
              review.add({
                copy,
                path: file.path,
                line: composing.line,
                endLine: composing.endLine,
                side: composing.side,
                text,
                code: composing.code,
              });
              const back = composing;
              setComposing(null);
              focusRow(back.at, back.side);
            }}
          />
        </div>
      </div>
    );

  const commentsAt = (index: number, side: Side | null) =>
    comments.filter((c) => anchorOf(c) === index && (side === null || c.side === side));

  const unifiedRow = (index: number) => {
    const line = lines[index];
    if (!line) return null;
    const side = sideOf(line);
    const isSelected = selected(index, side);
    return (
      <Fragment key={index}>
        <div
          role="row"
          data-row={index}
          data-line-new={line.kind === "removed" ? undefined : line.newLine ?? undefined}
          tabIndex={isTabbable(index, side) ? 0 : -1}
          className={`rd-row rd-row--${line.kind}${isSelected ? " rd-row--selected" : ""}`}
          aria-selected={isSelected || undefined}
          onFocus={(e) => e.target === e.currentTarget && setCursor({ index, side })}
          onKeyDown={(e) => onRowKey(e, index, side)}
          onMouseEnter={() => extendDrag(index, drag.current?.side ?? side)}
          onContextMenu={(e) => {
            e.preventDefault();
            if (!selected(index, side)) setSelection(null);
            setMenu({ at: { x: e.clientX, y: e.clientY }, index, side });
          }}
        >
          {lineCells(line, index, side, null)}
        </div>
        {threadFor(commentsAt(index, null), null)}
        {composing?.at === index && composerRow(null)}
      </Fragment>
    );
  };

  const splitRow = (row: SplitRow, key: string) => {
    const half = (index: number | null, side: Side) => {
      const line = index === null ? undefined : lines[index];
      if (index === null || !line) return <div className={`rd-half rd-half--${side} rd-half--empty`} aria-hidden />;
      const isSelected = selected(index, side);
      return (
        <div
          role="row"
          data-row={`${side}-${index}`}
          data-line-new={side === "new" ? line.newLine ?? undefined : undefined}
          tabIndex={isTabbable(index, side) ? 0 : -1}
          className={`rd-half rd-half--${side} rd-row rd-row--${line.kind}${isSelected ? " rd-row--selected" : ""}`}
          aria-selected={isSelected || undefined}
          onFocus={(e) => e.target === e.currentTarget && setCursor({ index, side })}
          onKeyDown={(e) => onRowKey(e, index, side)}
          onMouseEnter={() => extendDrag(index, side)}
          onContextMenu={(e) => {
            e.preventDefault();
            if (!selected(index, side)) setSelection(null);
            setMenu({ at: { x: e.clientX, y: e.clientY }, index, side });
          }}
        >
          {lineCells(line, index, side, side)}
        </div>
      );
    };
    const oldThreads = row.left !== null ? commentsAt(row.left, "old") : [];
    const newThreads = row.right !== null ? commentsAt(row.right, "new") : [];
    const composeOld = composing?.side === "old" && composing.at === row.left;
    const composeNew = composing?.side === "new" && composing.at === row.right;
    return (
      <Fragment key={key}>
        <div className="rd-pair" role="presentation">
          {half(row.left, "old")}
          {half(row.right, "new")}
        </div>
        {threadFor(oldThreads, "old")}
        {composeOld && composerRow("old")}
        {threadFor(newThreads, "new")}
        {composeNew && composerRow("new")}
      </Fragment>
    );
  };

  const gapRow = (gap: Gap, key: string) => {
    const size = gapSize(gap);
    return (
      <div key={key} role="row" className="rd-gap">
        <span role="cell" className="rd-gap__actions">
          {size > 0 &&
            (size <= EXPAND_STEP ? (
              <button type="button" className="rd-gap__btn" onClick={() => expand(gap, "all")}>
                {t("reviewer.expand.lines", { n: size })}
              </button>
            ) : (
              <>
                {gap.position !== "top" && (
                  <button
                    type="button"
                    className="rd-gap__btn"
                    aria-label={t("reviewer.expand.down", { n: EXPAND_STEP })}
                    title={t("reviewer.expand.down", { n: EXPAND_STEP })}
                    onClick={() => expand(gap, "down")}
                  >
                    ↓ {EXPAND_STEP}
                  </button>
                )}
                {gap.position !== "bottom" && (
                  <button
                    type="button"
                    className="rd-gap__btn"
                    aria-label={t("reviewer.expand.up", { n: EXPAND_STEP })}
                    title={t("reviewer.expand.up", { n: EXPAND_STEP })}
                    onClick={() => expand(gap, "up")}
                  >
                    ↑ {EXPAND_STEP}
                  </button>
                )}
                <button type="button" className="rd-gap__btn" onClick={() => expand(gap, "all")}>
                  {t("reviewer.expand.all", { n: size })}
                </button>
              </>
            ))}
        </span>
        <span role="cell" className="rd-gap__header">
          {gap.header}
        </span>
      </div>
    );
  };

  return (
    <>
      {outdated.length > 0 && (
        <div className="rd-outdated">
          <p className="muted small">{t("reviewer.outdated")}</p>
          <CommentThread comments={outdated} showWhere />
        </div>
      )}
      <div
        ref={table}
        className={`rd rd--${layout}`}
        role="table"
        aria-label={t("workspace.diff.of", { path: file.path })}
        style={{ "--rd-digits": Math.max(2, digits) } as React.CSSProperties}
      >
        {segs.map((seg, k) =>
          seg.kind === "gap" ? (
            gapRow(seg, `g${k}-${seg.from}`)
          ) : (
            <div key={`s${k}-${seg.from}`} role="rowgroup">
              {split
                ? rowsOf(seg).map((row, r) => splitRow(row, `${seg.from}-${r}`))
                : Array.from({ length: seg.to - seg.from + 1 }, (_, r) => unifiedRow(seg.from + r))}
            </div>
          ),
        )}
      </div>
      {menu && (
        <ContextMenu
          at={menu.at}
          label={t("reviewer.line.menu")}
          items={menuItems(menu.index, menu.side)}
          onClose={() => setMenu(null)}
        />
      )}
    </>
  );
}
