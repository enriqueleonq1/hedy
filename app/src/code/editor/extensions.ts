// The editor's own extensions: change markers, a blame gutter and the target
// line (with its search hits). Each is a state field fed by an effect, so React
// only dispatches data and the editor draws it.

import { RangeSet, RangeSetBuilder, StateEffect, StateField, type Extension } from "@codemirror/state";
import { Decoration, EditorView, gutter, GutterMarker, type DecorationSet } from "@codemirror/view";

import type { LineMark } from "./lineMarks";

// ------------------------------------------------------------------ change markers

class ChangeMarker extends GutterMarker {
  constructor(readonly kind: LineMark) {
    super();
  }
  eq(other: ChangeMarker) {
    return other.kind === this.kind;
  }
  toDOM() {
    const el = document.createElement("div");
    el.className = `cm-change cm-change--${this.kind}`;
    return el;
  }
}

const MARKERS: Record<LineMark, ChangeMarker> = {
  added: new ChangeMarker("added"),
  modified: new ChangeMarker("modified"),
  deleted: new ChangeMarker("deleted"),
};

export const setLineMarks = StateEffect.define<Map<number, LineMark>>();

const lineMarksField = StateField.define<RangeSet<GutterMarker>>({
  create: () => RangeSet.empty,
  update(value, tr) {
    for (const effect of tr.effects) {
      if (!effect.is(setLineMarks)) continue;
      const builder = new RangeSetBuilder<GutterMarker>();
      const doc = tr.state.doc;
      for (const line of [...effect.value.keys()].sort((a, b) => a - b)) {
        if (line < 1 || line > doc.lines) continue;
        const kind = effect.value.get(line);
        if (kind) builder.add(doc.line(line).from, doc.line(line).from, MARKERS[kind]);
      }
      return builder.finish();
    }
    return value.map(tr.changes);
  },
});

export function changeMarkers(): Extension {
  return [
    lineMarksField,
    gutter({ class: "cm-changeGutter", markers: (view) => view.state.field(lineMarksField) }),
  ];
}

// ------------------------------------------------------------------ blame

export interface BlameMark {
  /** First line of the run (1-based) and how many lines it spans. */
  start: number;
  count: number;
  commit: string;
  /** "Ana · hace 3 días" (already localized). */
  author: string;
  date: string;
}

class BlameMarker extends GutterMarker {
  constructor(readonly mark: BlameMark | null) {
    super();
  }
  eq(other: BlameMarker) {
    return other.mark === this.mark;
  }
  toDOM() {
    const el = document.createElement("div");
    el.className = "cm-blame";
    if (this.mark) {
      el.classList.add("cm-blame--first");
      el.dataset.commit = this.mark.commit;
      el.dataset.line = String(this.mark.start);
      const who = document.createElement("span");
      who.textContent = this.mark.author;
      el.append(who);
      if (this.mark.date) {
        const when = document.createElement("span");
        when.className = "cm-blame__date";
        when.textContent = ` · ${this.mark.date}`;
        el.append(when);
      }
    }
    return el;
  }
}

const EMPTY_BLAME = new BlameMarker(null);

export const setBlame = StateEffect.define<BlameMark[] | null>();

const blameField = StateField.define<{ on: boolean; set: RangeSet<GutterMarker> }>({
  create: () => ({ on: false, set: RangeSet.empty }),
  update(value, tr) {
    for (const effect of tr.effects) {
      if (!effect.is(setBlame)) continue;
      if (!effect.value) return { on: false, set: RangeSet.empty };
      const builder = new RangeSetBuilder<GutterMarker>();
      const doc = tr.state.doc;
      for (const mark of [...effect.value].sort((a, b) => a.start - b.start)) {
        if (mark.start < 1 || mark.start > doc.lines) continue;
        const from = doc.line(mark.start).from;
        builder.add(from, from, new BlameMarker(mark));
      }
      return { on: true, set: builder.finish() };
    }
    return value;
  },
});

/** The blame gutter shows only while blame data is set (`setBlame.of(null)` hides it). */
export function blameGutter(): Extension {
  return [
    blameField,
    // The column is hidden (by the theme) while blame is off.
    EditorView.editorAttributes.compute([blameField], (state) => ({ class: state.field(blameField).on ? "cm-blame-on" : "" })),
    gutter({
      class: "cm-blameGutter",
      markers: (view) => view.state.field(blameField).set,
      // While on, every line keeps the column's width (empty rows included).
      lineMarker: (view) => (view.state.field(blameField).on ? EMPTY_BLAME : null),
      initialSpacer: () => EMPTY_BLAME,
      lineMarkerChange: (update) => update.transactions.some((tr) => tr.effects.some((e) => e.is(setBlame))),
    }),
  ];
}

// ------------------------------------------------------------------ the target line

export interface TargetLine {
  line: number;
  /** Search hits on that line: UTF-16 offsets, end exclusive. */
  ranges?: { start: number; end: number }[];
}

export const setTarget = StateEffect.define<TargetLine | null>();

const targetLine = Decoration.line({ class: "cm-targetLine" });
const searchHit = Decoration.mark({ class: "cm-searchHit" });

const targetField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(value, tr) {
    for (const effect of tr.effects) {
      if (!effect.is(setTarget)) continue;
      const target = effect.value;
      const doc = tr.state.doc;
      if (!target || target.line < 1 || target.line > doc.lines) return Decoration.none;
      const line = doc.line(target.line);
      const builder = new RangeSetBuilder<Decoration>();
      builder.add(line.from, line.from, targetLine);
      for (const range of [...(target.ranges ?? [])].sort((a, b) => a.start - b.start)) {
        const from = line.from + Math.min(range.start, line.length);
        const to = line.from + Math.min(range.end, line.length);
        if (to > from) builder.add(from, to, searchHit);
      }
      return builder.finish();
    }
    return value.map(tr.changes);
  },
  provide: (field) => EditorView.decorations.from(field),
});

export function targetLineHighlight(): Extension {
  return targetField;
}
