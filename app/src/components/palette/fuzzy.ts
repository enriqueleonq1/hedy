// Fuzzy matching for the command palette.
//
// `fuzzyScore` ranks commands: every word of the query must match, as a
// substring (best at the start of a word) or, failing that, as letters in
// order. `fuzzyMatch` and `fuzzyPath` also say which characters matched (to
// highlight them) and rank like VS Code's quick open: a match in the file name
// beats one spread over the path, consecutive letters and starts of words
// (`/`, `_`, `-`, `.`, camelCase) count more, gaps count less. Accents and case
// do not count.

/** Lowercase, without accents ("Sesión" → "sesion"). */
export function fold(text: string): string {
  return text.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();
}

/**
 * `fold`, character by character, so indices keep pointing at the same
 * characters of `text` (highlights are drawn on the original).
 */
export function foldEach(text: string): string {
  // eslint-disable-next-line no-control-regex
  if (!/[^\x00-\x7f]/.test(text)) return text.toLowerCase();
  let out = "";
  for (let i = 0; i < text.length; i++) out += (text[i] ?? "").normalize("NFD").charAt(0).toLowerCase();
  return out;
}

function wordScore(word: string, text: string): number | null {
  const at = text.indexOf(word);
  if (at >= 0) {
    const boundary = at === 0 || !/[a-z0-9]/.test(text[at - 1] ?? "");
    return 100 + (boundary ? 50 : 0) - Math.min(at, 40) * 0.5;
  }
  // Letters in order: fewer gaps and an earlier start score higher.
  let pos = -1;
  let start = -1;
  let gaps = 0;
  for (const ch of word) {
    const next = text.indexOf(ch, pos + 1);
    if (next < 0) return null;
    if (start < 0) start = next;
    else gaps += next - pos - 1;
    pos = next;
  }
  return Math.max(1, 60 - gaps * 2 - Math.min(start, 40) * 0.5);
}

/** Higher is better; `null` when the query does not match. An empty query matches everything with 0. */
export function fuzzyScore(query: string, text: string): number | null {
  const words = fold(query).trim().split(/\s+/).filter(Boolean);
  if (words.length === 0) return 0;
  const haystack = fold(text);
  let total = 0;
  for (const word of words) {
    const score = wordScore(word, haystack);
    if (score === null) return null;
    total += score;
  }
  return total;
}

// ---------------------------------------------------------------- with positions

export interface FuzzyMatch {
  /** Higher is better. */
  score: number;
  /** Indices of the matched characters in the text, ascending. */
  positions: number[];
}

/** A text prepared once (folded), for matching it against many queries. */
export interface Prepared {
  text: string;
  folded: string;
}

export function prepare(text: string): Prepared {
  return { text, folded: foldEach(text) };
}

const SEPARATOR = /[\s/\\_\-.:()[\]{}]/;

/** The start of a word: the first character, after a separator, or a camelCase hump. */
function wordStart(text: string, i: number): boolean {
  if (i === 0) return true;
  const prev = text[i - 1] ?? "";
  if (SEPARATOR.test(prev)) return true;
  const ch = text[i] ?? "";
  return ch !== ch.toLowerCase() && prev === prev.toLowerCase() && /[a-z]/i.test(prev);
}

function scorePositions(text: string, positions: number[]): number {
  let score = 0;
  let prev = -2;
  let run = 0;
  for (const p of positions) {
    score += 1;
    if (wordStart(text, p)) score += 6;
    if (p === prev + 1) {
      // A longer run is worth more: "sion" in "Sesión" beats "s…ion".
      run += 1;
      score += 4 + 2 * run;
    } else {
      run = 0;
      if (prev >= 0) score -= 3 + Math.min(p - prev - 1, 6) * 0.5;
    }
    prev = p;
  }
  const first = positions[0] ?? 0;
  if (first === 0) score += 2;
  else score -= Math.min(first, 20) * 0.1;
  // Shorter texts first among equals.
  return score - text.length * 0.02;
}

function range(start: number, length: number): number[] {
  return Array.from({ length }, (_, i) => start + i);
}

/** Candidate alignments of `query` (folded, no spaces) in `p`; the best scored one wins. */
function align(query: string, p: Prepared): number[] | null {
  const { text, folded } = p;
  if (query.length > folded.length) return null;
  const candidates: number[][] = [];

  // A run of consecutive letters, preferably at the start of a word.
  let at = folded.indexOf(query);
  if (at >= 0) {
    let best = at;
    while (at >= 0) {
      if (wordStart(text, at)) {
        best = at;
        break;
      }
      at = folded.indexOf(query, at + 1);
    }
    candidates.push(range(best, query.length));
  }

  // Letters in order, then tightened from the end (the shortest window).
  let pos = -1;
  for (const ch of query) {
    pos = folded.indexOf(ch, pos + 1);
    if (pos < 0) return candidates[0] ?? null;
  }
  const tight: number[] = [];
  let back = pos + 1;
  for (let q = query.length - 1; q >= 0; q--) {
    back = folded.lastIndexOf(query.charAt(q), back - 1);
    tight.unshift(back);
  }
  candidates.push(tight);

  // Starts of words first ("fip" → facturas/impuestos.py), the next letter otherwise.
  const starts: number[] = [];
  pos = -1;
  for (let q = 0; q < query.length; q++) {
    const ch = query.charAt(q);
    let next = folded.indexOf(ch, pos + 1);
    if (next < 0) {
      starts.length = 0;
      break;
    }
    // Continue a run if the previous letter matched right before.
    if (!(starts.length && next === pos + 1)) {
      let probe = next;
      while (probe >= 0 && !wordStart(text, probe)) probe = folded.indexOf(ch, probe + 1);
      // Only jump to a word start if the rest of the query still fits after it.
      if (probe >= 0 && fits(query, q + 1, folded, probe)) next = probe;
    }
    starts.push(next);
    pos = next;
  }
  if (starts.length === query.length) candidates.push(starts);

  let best: number[] | null = null;
  let bestScore = -Infinity;
  for (const c of candidates) {
    const s = scorePositions(text, c);
    if (s > bestScore) {
      bestScore = s;
      best = c;
    }
  }
  return best;
}

function fits(query: string, from: number, folded: string, after: number): boolean {
  let pos = after;
  for (let q = from; q < query.length; q++) {
    pos = folded.indexOf(query.charAt(q), pos + 1);
    if (pos < 0) return false;
  }
  return true;
}

function pieces(query: string): string[] {
  return foldEach(query).trim().split(/\s+/).filter(Boolean);
}

function merge(a: number[], b: number[]): number[] {
  return [...new Set([...a, ...b])].sort((x, y) => x - y);
}

/**
 * Matches `query` against a label: every space-separated piece must match, its
 * letters in order. `null` when it does not; an empty query matches with 0.
 */
export function fuzzyMatch(query: string, text: string | Prepared): FuzzyMatch | null {
  const p = typeof text === "string" ? prepare(text) : text;
  const words = pieces(query);
  if (words.length === 0) return { score: 0, positions: [] };
  let score = 0;
  let positions: number[] = [];
  for (const word of words) {
    const found = align(word, p);
    if (!found) return null;
    score += scorePositions(p.text, found);
    positions = merge(positions, found);
  }
  return { score, positions };
}

/**
 * Matches `query` against a relative path, the file name first: a piece that
 * fits in the name scores well above one spread over the folders. A piece with
 * a `/` is matched against the whole path. Positions index the whole path.
 */
export function fuzzyPath(query: string, path: string | Prepared): FuzzyMatch | null {
  const p = typeof path === "string" ? prepare(path) : path;
  const words = pieces(query);
  if (words.length === 0) return { score: 0, positions: [] };
  const slash = p.text.lastIndexOf("/") + 1;
  const name: Prepared = { text: p.text.slice(slash), folded: p.folded.slice(slash) };
  let score = 0;
  let positions: number[] = [];
  for (const word of words) {
    const inName = word.includes("/") ? null : align(word, name);
    if (inName) {
      let s = scorePositions(name.text, inName) + 30;
      if (name.folded === word) s += 40;
      else if (name.folded.startsWith(word)) s += 20;
      score += s;
      positions = merge(
        positions,
        inName.map((i) => i + slash),
      );
      continue;
    }
    const inPath = align(word, p);
    if (!inPath) return null;
    score += scorePositions(p.text, inPath);
    positions = merge(positions, inPath);
  }
  return { score, positions };
}
