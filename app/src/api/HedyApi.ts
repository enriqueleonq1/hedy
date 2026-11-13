import type {
  NewAccountInput,
  Overview,
  SettingKey,
  TerminalEvent,
  TerminalRequest,
} from "./types";

/**
 * Everything the UI needs from the backend. Components depend on this
 * interface only; `tauriApi` implements it for the app and `MockApi` for tests
 * and for running the UI in a plain browser.
 */
export interface HedyApi {
  overview(): Promise<Overview>;

  addAccount(input: NewAccountInput): Promise<string>;
  setApiKey(id: string, key: string): Promise<void>;
  removeAccount(id: string): Promise<void>;
  setDefaultAccount(id: string | null): Promise<void>;
  setAccountColor(id: string, color: string): Promise<void>;

  useAccount(path: string, account: string, protect?: boolean): Promise<void>;
  offerableAccounts(path: string): Promise<string[]>;
  setProjectColor(path: string, color: string | null): Promise<void>;
  removeProject(path: string): Promise<void>;

  setSetting(key: SettingKey, value: string | null): Promise<void>;
  openInEditor(path: string): Promise<void>;
  pickFolder(): Promise<string | null>;

  terminalOpen(
    request: TerminalRequest,
    size: { cols: number; rows: number },
    onEvent: (event: TerminalEvent) => void,
  ): Promise<number>;
  terminalWrite(id: number, data: string): Promise<void>;
  terminalResize(id: number, size: { cols: number; rows: number }): Promise<void>;
  terminalClose(id: number): Promise<void>;
}
