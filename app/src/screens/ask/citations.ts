// The answer's text in parts: plain text, and the citations the core read
// (docs/decisions.md §36). A bracket becomes a citation only as the core
// reads one: it starts a word, it is outside code, and every number in it
// is a note the answer cites. `[2024]` or `args[0]` stay text.

export type AnswerPart = { text: string } | { cite: number[] };

const STARTS = /[\s\](("«.,;:]/;

export function answerParts(text: string, cited: number[]): AnswerPart[] {
  const parts: AnswerPart[] = [];
  let plain = "";
  let code = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i] ?? "";
    if (c === "`") {
      code = !code && text.indexOf("`", i + 1) >= 0;
      plain += c;
      continue;
    }
    if (c === "[" && !code && (i === 0 || STARTS.test(text[i - 1] ?? ""))) {
      const close = text.indexOf("]", i);
      const numbers = close > i ? text.slice(i + 1, close).split(", ") : [];
      if (numbers.length > 0 && numbers.every((n) => /^\d+$/.test(n) && cited.includes(Number(n)))) {
        if (plain) parts.push({ text: plain });
        plain = "";
        parts.push({ cite: numbers.map(Number) });
        i = close;
        continue;
      }
    }
    plain += c;
  }
  if (plain) parts.push({ text: plain });
  return parts;
}

/** Hidden characters (text direction, zero width) shown, as the core shows them. */
export function revealHidden(text: string): string {
  return text.replace(/[\u200b-\u200f\u202a-\u202e\u2060-\u2069\ufeff]/g, (c) => `\u27e6U+${c.charCodeAt(0).toString(16).toUpperCase().padStart(4, "0")}\u27e7`);
}
