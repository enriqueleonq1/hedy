// The one place that sets up a CodeMirror editor for Hedy. Read-only for now:
// turning editing on later means flipping `readOnly` (its compartment) and adding
// `history()` with the editing keymaps here, nothing else.

import { defaultKeymap } from "@codemirror/commands";
import { bracketMatching, codeFolding, foldGutter, foldKeymap, indentUnit, syntaxHighlighting } from "@codemirror/language";
import { gotoLine, highlightSelectionMatches, openSearchPanel, search, searchKeymap } from "@codemirror/search";
import { Compartment, EditorState, type Extension } from "@codemirror/state";
import {
  drawSelection,
  EditorView,
  highlightActiveLine,
  highlightActiveLineGutter,
  keymap,
  lineNumbers,
} from "@codemirror/view";

import { blameGutter, changeMarkers, targetLineHighlight } from "./extensions";
import { hedyEditorTheme, hedyHighlightStyle } from "./theme";

/** Per-editor switches that change after creation. */
export interface EditorCompartments {
  language: Compartment;
  readOnly: Compartment;
  phrases: Compartment;
}

export function createCompartments(): EditorCompartments {
  return { language: new Compartment(), readOnly: new Compartment(), phrases: new Compartment() };
}

function foldMarker(open: boolean): HTMLElement {
  const el = document.createElement("span");
  el.className = `cm-foldMarker${open ? " cm-foldMarker--open" : ""}`;
  el.innerHTML =
    '<svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m6 9 6 6 6-6"/></svg>';
  return el;
}

/** Ctrl/⌘ G: go to line (Ctrl/⌘ Alt G too, CodeMirror's own). */
const viewerKeymap = [
  { key: "Mod-g", run: gotoLine, preventDefault: true },
  { key: "Mod-f", run: openSearchPanel, scope: "editor search-panel", preventDefault: true },
];

/** Everything an editor needs, given its compartments and starting values. */
export function editorExtensions(
  compartments: EditorCompartments,
  options: { readOnly: boolean; language: Extension; phrases: Record<string, string> },
): Extension[] {
  return [
    blameGutter(),
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
    keymap.of([...viewerKeymap, ...searchKeymap, ...foldKeymap, ...defaultKeymap]),
    syntaxHighlighting(hedyHighlightStyle, { fallback: true }),
    hedyEditorTheme,
    EditorState.tabSize.of(4),
    indentUnit.of("    "),
    // Read-only, but still focusable with a caret: find, select, copy, go to line.
    compartments.readOnly.of(EditorState.readOnly.of(options.readOnly)),
    compartments.language.of(options.language),
    compartments.phrases.of(EditorState.phrases.of(options.phrases)),
    EditorView.contentAttributes.of({ "aria-readonly": String(options.readOnly) }),
  ];
}
