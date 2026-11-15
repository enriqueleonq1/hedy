// The one place that sets up a CodeMirror editor for Hedy. It starts read-only;
// editing flips two compartments: `readOnly`, and `editing` (history, bracket
// closing, indentation on input, multiple cursors and their keys).

import { closeBrackets, closeBracketsKeymap } from "@codemirror/autocomplete";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import {
  bracketMatching,
  codeFolding,
  foldGutter,
  foldKeymap,
  indentOnInput,
  indentUnit,
  syntaxHighlighting,
} from "@codemirror/language";
import { gotoLine, highlightSelectionMatches, openSearchPanel, search, searchKeymap } from "@codemirror/search";
import { Compartment, EditorState, type Extension } from "@codemirror/state";
import {
  drawSelection,
  EditorView,
  highlightActiveLine,
  highlightActiveLineGutter,
  keymap,
  lineNumbers,
  rectangularSelection,
} from "@codemirror/view";

import { changeMarkers, targetLineHighlight } from "./extensions";
import { hedyEditorTheme, hedyHighlightStyle } from "./theme";

/** Per-editor switches that change after creation. */
export interface EditorCompartments {
  language: Compartment;
  readOnly: Compartment;
  phrases: Compartment;
  /** The blame gutter: present only while blame is on, so it takes no width otherwise. */
  blame: Compartment;
  /** What only editing needs (history, bracket closing, multiple cursors). */
  editing: Compartment;
}

export function createCompartments(): EditorCompartments {
  return {
    language: new Compartment(),
    readOnly: new Compartment(),
    phrases: new Compartment(),
    blame: new Compartment(),
    editing: new Compartment(),
  };
}

function foldMarker(open: boolean): HTMLElement {
  const el = document.createElement("span");
  el.className = `cm-foldMarker${open ? " cm-foldMarker--open" : ""}`;
  el.innerHTML =
    '<svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m6 9 6 6 6-6"/></svg>';
  return el;
}

/** Ctrl/⌘ G: go to line (Ctrl/⌘ Alt G too, CodeMirror's own); Ctrl/⌘ H: find and replace. */
const viewerKeymap = [
  { key: "Mod-g", run: gotoLine, preventDefault: true },
  { key: "Mod-f", run: openSearchPanel, scope: "editor search-panel", preventDefault: true },
  { key: "Mod-h", run: openSearchPanel, scope: "editor search-panel", preventDefault: true },
];

/** Read-only, but still focusable with a caret: find, select, copy, go to line. */
export function readOnlyExtensions(readOnly: boolean): Extension {
  return [EditorState.readOnly.of(readOnly), EditorView.contentAttributes.of({ "aria-readonly": String(readOnly) })];
}

/**
 * Editing: undo/redo, indentation as you type, closing brackets and quotes,
 * Tab to indent (Esc then Tab leaves the editor), several cursors (Alt/⌥ click
 * adds one, Ctrl/⌘ D the next occurrence) and column selection (Alt/⌥ ⇧ drag).
 */
export function editingExtensions(on: boolean): Extension {
  if (!on) return [];
  return [
    history(),
    indentOnInput(),
    closeBrackets(),
    EditorState.allowMultipleSelections.of(true),
    EditorView.clickAddsSelectionRange.of((e) => e.altKey),
    rectangularSelection({ eventFilter: (e) => e.altKey && e.shiftKey }),
    keymap.of([...closeBracketsKeymap, ...historyKeymap, indentWithTab]),
  ];
}

/** Everything an editor needs, given its compartments and starting values. */
export function editorExtensions(
  compartments: EditorCompartments,
  options: { readOnly: boolean; language: Extension; phrases: Record<string, string>; blame?: Extension },
): Extension[] {
  return [
    compartments.blame.of(options.blame ?? []),
    lineNumbers(),
    changeMarkers(),
    foldGutter({ markerDOM: foldMarker }),
    codeFolding(),
    highlightActiveLineGutter(),
    drawSelection(),
    highlightActiveLine(),
    highlightSelectionMatches(),
    bracketMatching(),
    targetLineHighlight(),
    search({ top: true }),
    compartments.editing.of(editingExtensions(!options.readOnly)),
    keymap.of([...viewerKeymap, ...searchKeymap, ...foldKeymap, ...defaultKeymap]),
    syntaxHighlighting(hedyHighlightStyle, { fallback: true }),
    hedyEditorTheme,
    EditorState.tabSize.of(4),
    indentUnit.of("    "),
    compartments.readOnly.of(readOnlyExtensions(options.readOnly)),
    compartments.language.of(options.language),
    compartments.phrases.of(EditorState.phrases.of(options.phrases)),
  ];
}
