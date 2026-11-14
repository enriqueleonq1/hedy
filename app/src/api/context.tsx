import { createContext, useContext, type ReactNode } from "react";
import {
  useMutation,
  useQuery,
  useQueryClient,
  type QueryKey,
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

export interface ActionOptions<TVars, TResult> extends Omit<UseMutationOptions<TResult, unknown, TVars>, "mutationFn"> {
  /** Queries (or key prefixes) refreshed on success; the overview by default. */
  invalidate?: QueryKey[];
}

/**
 * A backend action that refreshes what it changed when it succeeds (the
 * overview unless told otherwise) and shows the (Spanish) error message from
 * the backend when it fails.
 */
export function useAction<TVars, TResult = void>(
  action: (api: HedyApi, vars: TVars) => Promise<TResult>,
  options?: ActionOptions<TVars, TResult>,
) {
  const api = useApi();
  const client = useQueryClient();
  const toast = useToast();
  const { invalidate = [OVERVIEW_KEY], ...mutation } = options ?? {};
  return useMutation<TResult, unknown, TVars>({
    mutationFn: (vars) => action(api, vars),
    ...mutation,
    onSuccess: async (...args) => {
      await Promise.all(invalidate.map((queryKey) => client.invalidateQueries({ queryKey })));
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
