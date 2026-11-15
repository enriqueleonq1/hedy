// The code views' way into the review (the Revisión tab): "Ver cambios" selects
// a file there, "Comentar en la revisión" adds a line comment. The review's
// comments store (`app/src/review/comments.tsx`, `useReviewComments()`) is the
// real implementation; until it is wired here, comments go to the composer as a
// line reference so nothing the user writes is lost.
//
// To wire it (one line): `export const useReviewBridge = useReviewComments;`
// (its `add` and `selectFile` have these signatures).

import type { CodeCopy } from "../api/codeTypes";
import { insertInComposer } from "../screens/workspace/composerBridge";
import { lineReference } from "./copies";

export interface NewReviewComment {
  copy: CodeCopy;
  path: string;
  line: number;
  endLine?: number;
  /** Which side of the diff the line belongs to; the current file is the new side. */
  side: "old" | "new";
  text: string;
}

export interface ReviewBridge {
  add(comment: NewReviewComment): void;
  selectFile(path: string): void;
}

const fallback: ReviewBridge = {
  add: ({ path, line, endLine, text }) => {
    insertInComposer(`${lineReference(path, line, endLine)} ${text}`);
  },
  selectFile: () => {},
};

export function useReviewBridge(): ReviewBridge {
  return fallback;
}
