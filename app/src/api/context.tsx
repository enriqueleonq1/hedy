import { createContext, useContext, type ReactNode } from "react";
import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationOptions,
} from "@tanstack/react-query";

import type { HedyApi } from "./HedyApi";
import { useToast } from "../components/ui/Toast";

const ApiContext = createContext<HedyApi | null>(null);

export function ApiProvider({ api, children }: { api: HedyApi; children: ReactNode }) {
  return <ApiContext.Provider value={api}>{children}</ApiContext.Provider>;
}

export function useApi(): HedyApi {
  const api = useContext(ApiContext);
  if (!api) throw new Error("useApi must be used inside <ApiProvider>");
  return api;
}

export const OVERVIEW_KEY = ["overview"] as const;

export function useOverview() {
  const api = useApi();
  return useQuery({ queryKey: OVERVIEW_KEY, queryFn: () => api.overview() });
}

/**
 * A backend action that refreshes the overview when it succeeds and shows the
 * (Spanish) error message from the backend when it fails.
 */
export function useAction<TVars, TResult = void>(
  action: (api: HedyApi, vars: TVars) => Promise<TResult>,
  options?: Omit<UseMutationOptions<TResult, unknown, TVars>, "mutationFn">,
) {
  const api = useApi();
  const client = useQueryClient();
  const toast = useToast();
  return useMutation<TResult, unknown, TVars>({
    mutationFn: (vars) => action(api, vars),
    ...options,
    onSuccess: async (...args) => {
      await client.invalidateQueries({ queryKey: OVERVIEW_KEY });
      await options?.onSuccess?.(...args);
    },
    onError: (error, ...rest) => {
      toast.error(errorMessage(error));
      options?.onError?.(error, ...rest);
    },
  });
}

export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "Algo salió mal.";
}
