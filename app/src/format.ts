// How numbers, money and times read in the UI.

/** "950", "182k", "2.4M". */
export function formatTokens(tokens: number): string {
  // 999 500 rounds to "1000k": from there on it reads as millions.
  if (tokens >= 999_500) return `${(tokens / 1_000_000).toFixed(1)}M`;
  if (tokens >= 1000) return `${Math.round(tokens / 1000)}k`;
  return `${tokens}`;
}

/** "$3,10"; "—" when the cost is unknown. */
export function formatUsd(cost: number | null): string {
  return cost === null ? "—" : `$${cost.toFixed(2).replace(".", ",")}`;
}

/** "42 s", "3 min 5 s". */
export function formatDuration(ms: number | null): string {
  if (ms === null) return "";
  const s = Math.round(ms / 1000);
  return s < 60 ? `${s} s` : `${Math.floor(s / 60)} min ${s % 60} s`;
}

/** "hace 3 días" from Unix seconds. */
export function timeAgo(seconds: number, now = Date.now() / 1000): string {
  const minutes = Math.max(0, Math.round((now - seconds) / 60));
  if (minutes < 1) return "ahora";
  if (minutes < 60) return `hace ${minutes} min`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `hace ${hours} h`;
  const days = Math.round(hours / 24);
  if (days < 14) return days === 1 ? "ayer" : `hace ${days} días`;
  if (days < 60) return `hace ${Math.round(days / 7)} semanas`;
  const months = Math.round(days / 30);
  return months < 24 ? `hace ${months} meses` : `hace ${Math.round(months / 12)} años`;
}
