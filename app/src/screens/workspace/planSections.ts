// A plan's «Lo que supongo» section, shown first in its card (docs/decisions.md §60):
// it is what the user corrects.

const HEADING = /^#{1,4}\s/;
const ASSUMPTIONS = /^#{1,4}\s*\**\s*(lo que supongo|what i assume)\b/i;

/**
 * Splits a plan into what the AI assumes and the rest. A plan without that
 * section (an agent that did not follow the format) is all `rest`.
 */
export function splitAssumptions(text: string): { assumptions: string | null; rest: string } {
  const lines = text.split("\n");
  const start = lines.findIndex((line) => ASSUMPTIONS.test(line.trim()));
  if (start < 0) return { assumptions: null, rest: text };
  const after = lines.findIndex((line, i) => i > start && HEADING.test(line.trim()));
  const end = after < 0 ? lines.length : after;
  const assumptions = lines.slice(start + 1, end).join("\n").trim();
  const rest = [...lines.slice(0, start), ...lines.slice(end)].join("\n").trim();
  return { assumptions: assumptions || null, rest };
}
