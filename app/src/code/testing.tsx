// Test helpers for the code views: the providers they need around them.

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "@testing-library/react";
import type { ReactElement, ReactNode } from "react";

import { ApiProvider } from "../api/context";
import { MockApi } from "../api/MockApi";
import { ToastProvider } from "../components/ui/Toast";
import { FileNavigatorProvider } from "../files/navigator";
import { UnsavedChangesProvider } from "./editor/unsaved";

export function sampleApi(): MockApi {
  const api = MockApi.withSampleData().withSampleActivity();
  api.delayMs = 0;
  return api;
}

export function renderCode(ui: ReactElement, api: MockApi = sampleApi()) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>
      <ApiProvider api={api}>
        <ToastProvider>
          <FileNavigatorProvider>
            <UnsavedChangesProvider>{children}</UnsavedChangesProvider>
          </FileNavigatorProvider>
        </ToastProvider>
      </ApiProvider>
    </QueryClientProvider>
  );
  return { api, client, ...render(ui, { wrapper }) };
}
