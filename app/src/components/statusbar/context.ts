import { createContext, useContext } from "react";

/**
 * Whether the window has a status bar (the shell's frame). Views that show
 * something the bar already shows (the folder's branch) leave it to the bar,
 * so it is said once; rendered alone (tests of one screen), they still show it.
 */
export const StatusBarContext = createContext(false);

export function useStatusBarShown(): boolean {
  return useContext(StatusBarContext);
}
