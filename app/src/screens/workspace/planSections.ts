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
  // It ends at the next heading of its level or above, outside code blocks.
  const level = (line: string) => line.trim().match(/^#+/)?.[0].length ?? 0;
  const own = level(lines[start] ?? "");
  let fenced = false;
  const after = lines.findIndex((line, i) => {
    if (line.trim().startsWith("```")) fenced = !fenced;
    return i > start && !fenced && HEADING.test(line.trim()) && level(line) <= own;
  });
  const end = after < 0 ? lines.length : after;
  const assumptions = lines.slice(start + 1, end).join("\n").trim();
  const rest = [...lines.slice(0, start), ...lines.slice(end)].join("\n").trim();
  return { assumptions: assumptions || null, rest };
}
