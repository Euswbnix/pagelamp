import { useEffect } from "react";
import { isRouteErrorResponse } from "react-router";
import { useApi } from "@/api/context";
import { isApiError } from "@/api/errors";

// The Rust side redacts and caps too; these just keep one bad error from flooding the log.
const MAX_MESSAGE = 2_000;
const MAX_STACK = 8_000;

// Errors already written. React's StrictMode runs effects twice in development, and a screen
// that crashes again after "Try again" throws a new object, which is logged again (as it should).
const logged = new WeakSet<object>();

/** What a crashed screen threw, as message + stack only: never props, state or course data. */
export function describeUiError(error: unknown): { message: string; stack: string | null } {
  if (isRouteErrorResponse(error)) {
    return { message: `${error.status} ${error.statusText}`.trim(), stack: null };
  }
  if (isApiError(error)) {
    return { message: `ApiError(${error.kind}): ${error.message}`, stack: error.stack ?? null };
  }
  if (error instanceof Error) {
    return { message: `${error.name}: ${error.message}`, stack: error.stack ?? null };
  }
  return { message: String(error), stack: null };
}

function cap(text: string, max: number) {
  return text.length > max ? `${text.slice(0, max)}…` : text;
}

/** Writes an error-boundary error to PageLamp's log (Settings → Help & feedback → logs). */
export function useLogUiError(error: unknown) {
  const api = useApi();
  useEffect(() => {
    if (typeof error === "object" && error !== null) {
      if (logged.has(error)) return;
      logged.add(error);
    }
    const { message, stack } = describeUiError(error);
    void api.logUiError(cap(message, MAX_MESSAGE), stack === null ? null : cap(stack, MAX_STACK));
  }, [api, error]);
}
