import { useEffect, useState } from "react";

import { useT } from "../../i18n";
import { ACTIVITY_LABEL, ACTIVITY_TIMING, ACTIVITY_WORDS, type WorkingActivity } from "./activity";
import "./Mascot.css";

/**
 * The word next to a working mascot ("Leyendo…"), rotating among the words
 * of its activity. Decorative and hidden from screen readers: it changes
 * every few seconds, often inside live regions.
 */
export function WorkingWord({ activity, className }: { activity: WorkingActivity | undefined; className?: string }) {
  const t = useT();
  const [turn, setTurn] = useState({ activity, n: 0 });
  // A new activity starts again from its first word.
  if (turn.activity !== activity) setTurn({ activity, n: 0 });

  useEffect(() => {
    if (!activity) return;
    const timer = setInterval(
      () => setTurn((current) => ({ activity: current.activity, n: current.n + 1 })),
      ACTIVITY_TIMING.wordEveryMs,
    );
    return () => clearInterval(timer);
  }, [activity]);

  if (!activity) return null;
  const words = ACTIVITY_WORDS[activity];
  const word = t(words[turn.n % words.length] ?? ACTIVITY_LABEL[activity]);
  return (
    <span className={["working-word", className].filter(Boolean).join(" ")} aria-hidden data-activity={activity}>
      <span key={word} className="working-word__text">
        {word}
      </span>
    </span>
  );
}
