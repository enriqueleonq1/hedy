import { useQueryClient } from "@tanstack/react-query";
import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from "react";

import { useApi } from "../../api/context";
import { useToast } from "../../components/ui/Toast";
import { useT } from "../../i18n";
import { SKILLS_OVERVIEW_KEY, verdictKey, voidKey } from "./library";

/** How often a trial left running in the background is looked at. */
const BACKGROUND_POLL_MS = 5000;

type BackgroundTrial = { trial: string; name: string };

const BackgroundTrials = createContext<((trial: BackgroundTrial) => void) | null>(null);

/**
 * Trials left running with "Seguir en segundo plano": watched from here, above
 * every screen, so their outcome is shown (once) even after leaving Skills.
 */
export function BackgroundTrialsProvider({ children }: { children: ReactNode }) {
  const t = useT();
  const api = useApi();
  const toast = useToast();
  const client = useQueryClient();
  const [trials, setTrials] = useState<BackgroundTrial[]>([]);
  useEffect(() => {
    if (trials.length === 0) return;
    const timer = setInterval(async () => {
      for (const running of trials) {
        const ended = await api.takeTrialOutcome(running.trial).catch(() => null);
        if (!ended) continue;
        setTrials((all) => all.filter((x) => x.trial !== running.trial));
        await client.invalidateQueries({ queryKey: SKILLS_OVERVIEW_KEY });
        if ("judged" in ended) {
          toast.info(t("skillLib.try.doneIn", { name: running.name, verdict: t(verdictKey[ended.judged.verdict]) }));
        } else {
          toast.error(`${running.name}: ${t(voidKey[ended.void])}`);
        }
      }
    }, BACKGROUND_POLL_MS);
    return () => clearInterval(timer);
  }, [trials, api, client, toast, t]);
  const add = useMemo(() => (trial: BackgroundTrial) => setTrials((all) => [...all, trial]), []);
  return <BackgroundTrials.Provider value={add}>{children}</BackgroundTrials.Provider>;
}

/** Hands a running trial to [`BackgroundTrialsProvider`]. */
export function useBackgroundTrials() {
  const add = useContext(BackgroundTrials);
  if (!add) throw new Error("useBackgroundTrials must be used inside <BackgroundTrialsProvider>");
  return { add };
}
