// A React wrapper around the CodeMirror view set up in `setup.ts`: it creates
// the view once and dispatches data (the document, its language, change
// markers, blame, the target line) as props change.

import { closeSearchPanel, gotoLine, openSearchPanel } from "@codemirror/search";
import { EditorSelection, EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { forwardRef, useEffect, useImperativeHandle, useLayoutEffect, useRef, type MouseEvent } from "react";

import { setBlame, setLineMarks, setTarget, type BlameMark, type TargetLine } from "./extensions";
import { loadLanguage } from "./languages";
import type { LineMark } from "./lineMarks";
import { createCompartments, editorExtensions } from "./setup";

/** Where a right click landed: its line, and the selected lines when it fell inside the selection. */
export interface EditorPoint {
  line: number;
  /** The selection's lines (`start..=end`) when the click was inside it, else just `line`. */
  start: number;
  end: number;
  /** The selected text, when there is a selection under the click. */
  selection: string;
}

export interface CodeEditorHandle {
  openSearch(): void;
  closeSearch(): void;
  gotoLine(): void;
  focus(): void;
  /** The caret's line (1-based). */
  cursorLine(): number;
  /** The selected text (empty when nothing is selected). */
  selectedText(): string;
  /** The selection's first and last lines. */
  selectedLines(): { start: number; end: number };
}

export interface CodeEditorProps {
  /** The file's text. */
  doc: string;
  /** Picks the highlighting. */
  path: string;
  label: string;
  lineMarks?: Map<number, LineMark>;
  /** `null` hides the blame gutter. */
  blame?: BlameMark[] | null;
  /** The line to reveal and highlight; `seq` changes to reveal it again. */
  target?: TargetLine | null;
  seq?: number;
  /** CodeMirror's own texts (find panel, go to line) in the user's language. */
  phrases: Record<string, string>;
  onContextMenu?: (point: EditorPoint, event: MouseEvent) => void;
  /** A click on a blame entry: its commit and first line. */
  onBlameClick?: (commit: string, line: number, anchor: HTMLElement) => void;
  onBlameHover?: (commit: string | null, anchor: HTMLElement | null) => void;
}

export const CodeEditor = forwardRef<CodeEditorHandle, CodeEditorProps>(function CodeEditor(
  { doc, path, label, lineMarks, blame = null, target = null, seq = 0, phrases, onContextMenu, onBlameClick, onBlameHover },
  ref,
) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const compartments = useRef(createCompartments());

  // The view lives as long as the component; a new file replaces its state.
  useLayoutEffect(() => {
    const parent = host.current;
    if (!parent) return;
    const created = new EditorView({ parent });
    view.current = created;
    return () => {
      created.destroy();
      view.current = null;
    };
  }, []);

  useLayoutEffect(() => {
    const v = view.current;
    if (!v) return;
    v.setState(
      EditorState.create({
        doc,
        extensions: [
          editorExtensions(compartments.current, { readOnly: true, language: [], phrases }),
          EditorView.contentAttributes.of({ "aria-label": label }),
        ],
      }),
    );
    // `phrases` and `label` are applied with each new document.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [doc, path]);

  // The language pack arrives later (a lazy chunk); plain text meanwhile.
  useEffect(() => {
    const pending = loadLanguage(path);
    if (!pending) return;
    let live = true;
    pending
      .then((language) => {
        const v = view.current;
        if (live && v) v.dispatch({ effects: compartments.current.language.reconfigure(language) });
      })
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [path, doc]);

  useEffect(() => {
    view.current?.dispatch({ effects: setLineMarks.of(lineMarks ?? new Map()) });
  }, [lineMarks, doc, path]);

  useEffect(() => {
    view.current?.dispatch({ effects: setBlame.of(blame) });
  }, [blame, doc, path]);

  useEffect(() => {
    const v = view.current;
    if (!v) return;
    if (!target || target.line < 1 || target.line > v.state.doc.lines) {
      v.dispatch({ effects: setTarget.of(null) });
      return;
    }
    const line = v.state.doc.line(target.line);
    const first = target.ranges?.[0];
    const at = first ? line.from + Math.min(first.start, line.length) : line.from;
    v.dispatch({
      effects: [setTarget.of(target), EditorView.scrollIntoView(at, { y: "center" })],
      selection: EditorSelection.cursor(at),
    });
  }, [target, seq, doc, path]);

  useImperativeHandle(ref, () => ({
    openSearch() {
      const v = view.current;
      if (v) openSearchPanel(v);
    },
    closeSearch() {
      const v = view.current;
      if (v) closeSearchPanel(v);
    },
    gotoLine() {
      const v = view.current;
      if (v) gotoLine(v);
    },
    focus() {
      view.current?.focus();
    },
    cursorLine() {
      const v = view.current;
      return v ? v.state.doc.lineAt(v.state.selection.main.head).number : 1;
    },
    selectedText() {
      const v = view.current;
      if (!v) return "";
      const { from, to } = v.state.selection.main;
      return v.state.sliceDoc(from, to);
    },
    selectedLines() {
      const v = view.current;
      if (!v) return { start: 1, end: 1 };
      const { from, to } = v.state.selection.main;
      return { start: v.state.doc.lineAt(from).number, end: v.state.doc.lineAt(to).number };
    },
  }));

  const pointAt = (e: MouseEvent): EditorPoint | null => {
    const v = view.current;
    if (!v) return null;
    const pos = v.posAtCoords({ x: e.clientX, y: e.clientY }) ?? v.state.selection.main.head;
    const line = v.state.doc.lineAt(pos).number;
    const { from, to } = v.state.selection.main;
    if (from !== to && pos >= from && pos <= to) {
      return {
        line,
        start: v.state.doc.lineAt(from).number,
        end: v.state.doc.lineAt(to).number,
        selection: v.state.sliceDoc(from, to),
      };
    }
    return { line, start: line, end: line, selection: "" };
  };

  const blameAt = (target: EventTarget) => (target instanceof Element ? target.closest<HTMLElement>(".cm-blame[data-commit]") : null);

  return (
    <div
      ref={host}
      className="code-editor"
      onContextMenu={(e) => {
        if (!onContextMenu) return;
        const point = pointAt(e);
        if (!point) return;
        e.preventDefault();
        onContextMenu(point, e);
      }}
      onClick={(e) => {
        const el = blameAt(e.target);
        if (el && onBlameClick) onBlameClick(el.dataset.commit ?? "", Number(el.dataset.line), el);
      }}
      onMouseOver={(e) => {
        if (!onBlameHover) return;
        const el = blameAt(e.target);
        onBlameHover(el?.dataset.commit ?? null, el);
      }}
      onMouseLeave={() => onBlameHover?.(null, null)}
    />
  );
});
