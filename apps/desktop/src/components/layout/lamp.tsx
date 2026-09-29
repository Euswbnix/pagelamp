import { createContext, useContext, useEffect } from "react";

/** AppShell's switch for the lamp band at the top of the content column. */
export const LampContext = createContext<((lit: boolean) => void) | null>(null);

/**
 * One light (docs/design/macos-shell.md §6.1): while a screen shows "now", the lamp band's warm
 * pool lights the top of the content column, under the toolbar row. Outside the app shell it
 * does nothing. Use through PageHeader's `lit`, so the light always sits next to a word.
 */
export function useLampBand(lit: boolean) {
  const setLit = useContext(LampContext);
  useEffect(() => {
    if (!setLit || !lit) return;
    setLit(true);
    return () => setLit(false);
  }, [setLit, lit]);
}
