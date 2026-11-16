// Hedy's look for CodeMirror. Every color is a theme token (CSS variables set by
// `applyTheme` from `theme/tokens.ts`), so one definition serves light and dark
// and follows a theme switch without reconfiguring the editor.

import { HighlightStyle } from "@codemirror/language";
import { EditorView } from "@codemirror/view";
import { tags as t } from "@lezer/highlight";

export const hedyEditorTheme = EditorView.theme({
  "&": {
    height: "100%",
    color: "var(--ink)",
    backgroundColor: "var(--panel)",
    fontSize: "12.5px",
  },
  "&.cm-focused": { outline: "none" },
  ".cm-scroller": {
    fontFamily: "var(--font-mono)",
    lineHeight: "1.65",
    fontVariantLigatures: "none",
  },
  ".cm-content": { padding: "8px 0 48px", caretColor: "var(--accent)" },
  ".cm-line": { padding: "0 16px 0 6px" },
  ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--accent)", borderLeftWidth: "1.5px" },
  "&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, ::selection": {
    backgroundColor: "var(--text-selection) !important",
  },
  ".cm-activeLine": { backgroundColor: "color-mix(in srgb, var(--selection) 45%, transparent)" },
  ".cm-selectionMatch": { backgroundColor: "color-mix(in srgb, var(--text-selection) 55%, transparent)" },
  ".cm-matchingBracket": { backgroundColor: "var(--accent-soft)", outline: "1px solid var(--line-strong)" },

  ".cm-gutters": {
    backgroundColor: "var(--panel)",
    color: "var(--muted)",
    border: "none",
  },
  ".cm-lineNumbers .cm-gutterElement": {
    minWidth: "3.2ch",
    padding: "0 6px 0 12px",
    opacity: "0.55",
    fontVariantNumeric: "tabular-nums",
  },
  ".cm-activeLineGutter": { backgroundColor: "transparent" },
  ".cm-activeLineGutter.cm-gutterElement": { opacity: "1", color: "var(--ink)" },
  ".cm-foldGutter .cm-gutterElement": { width: "14px", display: "flex", alignItems: "center", justifyContent: "center" },
  ".cm-foldPlaceholder": {
    backgroundColor: "var(--panel-2)",
    border: "1px solid var(--line)",
    color: "var(--muted)",
    borderRadius: "4px",
    padding: "0 6px",
    margin: "0 4px",
  },

  // Changed lines (against the base): a slim bar next to the numbers.
  ".cm-changeGutter": { width: "4px", marginRight: "2px" },
  ".cm-changeGutter .cm-gutterElement": { padding: "0" },
  ".cm-change": { width: "3px", height: "100%", borderRadius: "1px" },
  ".cm-change--added": { backgroundColor: "var(--diff-add-ink)", opacity: "0.75" },
  ".cm-change--modified": { backgroundColor: "var(--accent)", opacity: "0.75" },
  ".cm-change--deleted": {
    height: "0",
    width: "0",
    borderTop: "4px solid transparent",
    borderBottom: "4px solid transparent",
    borderLeft: "5px solid var(--diff-del-ink)",
    transform: "translateY(-4px)",
  },

  // Blame: who last touched each run of lines.
  ".cm-blameGutter .cm-gutterElement": { padding: "0 10px 0 8px" },
  ".cm-blame": {
    width: "21ch",
    overflow: "hidden",
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
    fontFamily: "var(--font-ui)",
    fontSize: "11.5px",
    color: "var(--muted)",
    cursor: "pointer",
  },
  ".cm-blame:hover": { color: "var(--ink)" },
  ".cm-blame__date": { opacity: "0.7" },
  ".cm-blame--first": { borderTop: "1px solid var(--line)" },

  // The line a link pointed at, and the search hit on it.
  ".cm-targetLine": { backgroundColor: "var(--accent-soft)" },
  ".cm-searchHit": {
    backgroundColor: "color-mix(in srgb, var(--warning) 32%, transparent)",
    borderRadius: "2px",
    boxShadow: "0 0 0 1px color-mix(in srgb, var(--warning) 60%, transparent)",
  },

  // Find in file and go to line: a quiet bar at the top.
  ".cm-panels": { backgroundColor: "var(--panel)", color: "var(--ink)" },
  ".cm-panels.cm-panels-top": { borderBottom: "1px solid var(--line)" },
  ".cm-panel": { fontFamily: "var(--font-ui)", fontSize: "12.5px" },
  ".cm-panel.cm-search, .cm-panel.cm-gotoLine": {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    gap: "6px",
    padding: "8px 36px 8px 10px",
  },
  ".cm-panel.cm-search br": { display: "none" },
  ".cm-panel input[type=text], .cm-panel input:not([type])": {
    height: "26px",
    padding: "0 8px",
    border: "1px solid var(--line)",
    borderRadius: "6px",
    backgroundColor: "var(--panel-2)",
    color: "var(--ink)",
    font: "inherit",
    outline: "none",
  },
  ".cm-panel input[type=text]:focus, .cm-panel input:not([type]):focus": { borderColor: "var(--focus)" },
  ".cm-panel .cm-textfield": { width: "14em", margin: "0" },
  ".cm-panel button, .cm-panel .cm-button": {
    height: "26px",
    padding: "0 9px",
    border: "1px solid var(--line)",
    borderRadius: "6px",
    backgroundColor: "transparent",
    backgroundImage: "none",
    color: "var(--ink)",
    font: "inherit",
    cursor: "pointer",
  },
  ".cm-panel button:hover, .cm-panel .cm-button:hover": { backgroundColor: "var(--selection)" },
  ".cm-panel label": { display: "inline-flex", alignItems: "center", gap: "4px", color: "var(--muted)", fontSize: "12px" },
  ".cm-panel input[type=checkbox]": { accentColor: "var(--action)", margin: "0" },
  ".cm-panel button[name=close]": {
    position: "absolute",
    top: "8px",
    right: "8px",
    width: "26px",
    padding: "0",
    border: "none",
    color: "var(--muted)",
    fontSize: "16px",
  },
  ".cm-searchMatch": {
    backgroundColor: "color-mix(in srgb, var(--warning) 22%, transparent)",
    borderRadius: "2px",
  },
  ".cm-searchMatch-selected": { backgroundColor: "color-mix(in srgb, var(--warning) 45%, transparent)" },
  ".cm-tooltip": {
    border: "1px solid var(--line)",
    borderRadius: "8px",
    backgroundColor: "var(--panel)",
    color: "var(--ink)",
    boxShadow: "var(--shadow-pop)",
  },
});

/**
 * Syntax colors from the theme's `syn*` tokens (in Hedy's own themes: violet keywords,
 * green strings, amber numbers and constants, teal functions, muted comments). Calm on purpose.
 */
export const hedyHighlightStyle = HighlightStyle.define([
  { tag: [t.keyword, t.controlKeyword, t.moduleKeyword, t.operatorKeyword, t.definitionKeyword, t.modifier], color: "var(--syn-keyword)" },
  { tag: [t.string, t.special(t.string), t.regexp, t.character, t.attributeValue], color: "var(--syn-string)" },
  { tag: [t.number, t.bool, t.null, t.atom, t.constant(t.variableName), t.unit], color: "var(--syn-number)" },
  { tag: [t.function(t.variableName), t.function(t.propertyName), t.macroName], color: "var(--syn-function)" },
  { tag: [t.typeName, t.className, t.namespace, t.definition(t.typeName)], color: "var(--syn-type)", fontStyle: "italic" },
  { tag: [t.tagName, t.angleBracket], color: "var(--syn-tag)" },
  { tag: [t.attributeName, t.propertyName], color: "var(--ink)" },
  { tag: [t.comment, t.lineComment, t.blockComment, t.docComment], color: "var(--syn-comment)", fontStyle: "italic" },
  { tag: [t.meta, t.processingInstruction, t.annotation], color: "var(--muted)" },
  { tag: [t.operator, t.punctuation, t.separator, t.bracket], color: "var(--muted)" },
  { tag: t.heading, color: "var(--accent)", fontWeight: "700" },
  { tag: [t.emphasis], fontStyle: "italic" },
  { tag: [t.strong], fontWeight: "700" },
  { tag: [t.link, t.url], color: "var(--action-text)", textDecoration: "underline" },
  { tag: t.invalid, color: "var(--danger)" },
  { tag: [t.inserted], color: "var(--diff-add-ink)" },
  { tag: [t.deleted], color: "var(--diff-del-ink)" },
]);
