// A React wrapper around the CodeMirror view set up in `setup.ts`: it creates
// the view once and dispatches data (the document, its language, change
// markers, blame, the target line) as props change. With `editable`, the same
// view edits: it tracks whether the text differs from the last saved one and
// never replaces a buffer with unsaved changes.

import { redo, undo } from "@codemirror/commands";
import { closeSearchPanel, gotoLine, openSearchPanel } from "@codemirror/search";
import { EditorSelection, EditorState, type Text } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { forwardRef, useEffect, useImperativeHandle, useLayoutEffect, useRef, type MouseEvent } from "react";

import { blameGutter, setLineMarks, setTarget, type BlameMark, type TargetLine } from "./extensions";
import { loadLanguage } from "./languages";
import type { LineMark } from "./lineMarks";
import { createCompartments, editingExtensions, editorExtensions, readOnlyExtensions } from "./setup";

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
  /** The text as edited (lines joined by `\n`). */
  text(): string;
  /** Whether the text differs from the last saved one. */
  isDirty(): boolean;
  /** The text now is what the file holds (after a save). */
  markSaved(): void;
  /** Replaces the text and takes it as saved (reload, discard). */
  reset(text: string): void;
  /** Back to the saved text (one undoable change). */
  revert(): void;
  undo(): boolean;
  redo(): boolean;
  /** Moves the caret to a line and shows it. */
  revealLine(line: number): void;
}

export interface CodeEditorProps {
  /** The file's text. Ignored while the buffer has unsaved changes. */
  doc: string;
  /** Picks the highlighting. */
  path: string;
  label: string;
  lineMarks?: Map<number, LineMark>;
  /** `null` removes the blame gutter (it then takes no width). */
  blame?: BlameMark[] | null;
  /** The line to reveal and highlight; `seq` changes to reveal it again. */
  target?: TargetLine | null;
  seq?: number;
  /** CodeMirror's own texts (find panel, go to line) in the user's language. */
  phrases: Record<string, string>;
  /** Typing changes the text (undo/redo, brackets, several cursors). */
  editable?: boolean;
  /** The text started or stopped differing from the saved one. */
  onDirtyChange?: (dirty: boolean) => void;
  /** A double click in read-only mode: the user wants to edit. */
  onRequestEdit?: () => void;
  onContextMenu?: (point: EditorPoint, event: MouseEvent) => void;
  /** A click on a blame entry: its commit and first line. */
  onBlameClick?: (commit: string, line: number, anchor: HTMLElement) => void;
  onBlameHover?: (commit: string | null, anchor: HTMLElement | null) => void;
}

function sameText(a: Text, b: Text): boolean {
  return a.length === b.length && a.eq(b);
}

export const CodeEditor = forwardRef<CodeEditorHandle, CodeEditorProps>(function CodeEditor(
  {
    doc,
    path,
    label,
    lineMarks,
    blame = null,
    target = null,
    seq = 0,
    phrases,
    editable = false,
    onDirtyChange,
    onRequestEdit,
    onContextMenu,
    onBlameClick,
    onBlameHover,
  },
  ref,
) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const compartments = useRef(createCompartments());
  // The text as last saved (or loaded), and whether the buffer differs from it.
  const saved = useRef<Text | null>(null);
  const dirty = useRef(false);
  const dirtyChange = useRef(onDirtyChange);
  const editableNow = useRef(editable);
  useLayoutEffect(() => {
    dirtyChange.current = onDirtyChange;
    editableNow.current = editable;
  });

  const setDirty = (next: boolean) => {
    if (dirty.current === next) return;
    dirty.current = next;
    dirtyChange.current?.(next);
  };

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

  const freshState = (text: string) =>
    EditorState.create({
      doc: text,
      extensions: [
        editorExtensions(compartments.current, { readOnly: !editableNow.current, language: [], phrases }),
        EditorView.contentAttributes.of({ "aria-label": label }),
        EditorView.updateListener.of((update) => {
          if (update.docChanged && saved.current) setDirty(!sameText(update.state.doc, saved.current));
        }),
      ],
    });

  useLayoutEffect(() => {
    const v = view.current;
    if (!v) return;
    // Unsaved changes are never replaced; the same text (after a save) keeps history and caret.
    if (dirty.current) return;
    // (CodeMirror keeps lines joined by "\n" whatever the file uses.)
    if (saved.current && v.state.doc.toString() === doc.replace(/\r\n?/g, "\n")) return;
    const state = freshState(doc);
    v.setState(state);
    saved.current = state.doc;
    // `phrases` and `label` are applied with each new document.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [doc, path]);

  useEffect(() => {
    view.current?.dispatch({
      effects: [
        compartments.current.readOnly.reconfigure(readOnlyExtensions(!editable)),
        compartments.current.editing.reconfigure(editingExtensions(editable)),
      ],
    });
  }, [editable]);

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
    view.current?.dispatch({ effects: compartments.current.blame.reconfigure(blame ? blameGutter(blame) : []) });
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
    text() {
      return view.current?.state.doc.toString() ?? doc;
    },
    isDirty() {
      return dirty.current;
    },
    markSaved() {
      const v = view.current;
      if (!v) return;
      saved.current = v.state.doc;
      setDirty(false);
    },
    reset(text: string) {
      const v = view.current;
      if (!v) return;
      const state = freshState(text);
      v.setState(state);
      saved.current = state.doc;
      setDirty(false);
      v.dispatch({ effects: compartments.current.blame.reconfigure(blame ? blameGutter(blame) : []) });
    },
    revert() {
      const v = view.current;
      const text = saved.current;
      if (!v || !text || !dirty.current) return;
      // One undoable change back to the saved text.
      v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: text } });
    },
    undo() {
      const v = view.current;
      return v ? undo(v) : false;
    },
    redo() {
      const v = view.current;
      return v ? redo(v) : false;
    },
    revealLine(line: number) {
      const v = view.current;
      if (!v || line < 1 || line > v.state.doc.lines) return;
      const at = v.state.doc.line(line).from;
      v.dispatch({ selection: EditorSelection.cursor(at), effects: EditorView.scrollIntoView(at, { y: "center" }) });
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
      className={`code-editor${editable ? " code-editor--editing" : ""}`}
      onContextMenu={(e) => {
        if (!onContextMenu) return;
        const point = pointAt(e);
        if (!point) return;
        e.preventDefault();
        onContextMenu(point, e);
      }}
      onDoubleClick={(e) => {
        if (editable || !onRequestEdit) return;
        if (e.target instanceof Element && e.target.closest(".cm-content")) onRequestEdit();
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
