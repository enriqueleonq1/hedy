import { useEffect, useState } from "react";

import type { Translate } from "../i18n";

/** The current time in Unix seconds, refreshed every `everyMs`. */
export function useNowSeconds(everyMs = 1000): number {
  const [now, setNow] = useState(() => Date.now() / 1000);
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now() / 1000), everyMs);
    return () => clearInterval(timer);
  }, [everyMs]);
  return now;
}

/**
 * Time left until a limit renews: "2 h 05 min", "12 min", "4 min 05 s",
 * "40 s". `null` once it is due.
 */
export function formatCountdown(seconds: number, t: Translate): string | null {
  const left = Math.ceil(seconds);
  if (left <= 0) return null;
  const pad = (n: number) => String(n).padStart(2, "0");
  if (left >= 3600) {
    const h = Math.floor(left / 3600);
    const m = Math.floor((left % 3600) / 60);
    return t("wait.time.hours", { h, m: pad(m) });
  }
  if (left >= 600) return t("wait.time.minutes", { m: Math.ceil(left / 60) });
  if (left >= 60) return t("wait.time.minutesSeconds", { m: Math.floor(left / 60), s: pad(left % 60) });
  return t("wait.time.seconds", { s: left });
}
