import { type RefObject, useEffect, useRef } from "react";

/**
 * Moves focus to the element once it mounts: where a run's progress, result or outcome replaces
 * the button that started it, so keyboard and screen-reader users aren't left on the page body.
 */
export function useFocusOnMount<T extends HTMLElement>(): RefObject<T | null> {
  const ref = useRef<T>(null);
  useEffect(() => {
    ref.current?.focus();
  }, []);
  return ref;
}
