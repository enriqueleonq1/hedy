// Fuzzy matching for the command palette: every word of the query must match,
// as a substring (best at the start of a word) or, failing that, as letters in
// order. Accents and case do not count.

/** Lowercase, without accents ("Sesión" → "sesion"). */
export function fold(text: string): string {
  return text.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();
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
