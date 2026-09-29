import { describe, expect, it } from "vitest";
import { ApiError, isApiError, toApiError } from "./errors";

describe("toApiError", () => {
  it("keeps the kind and message of a serialised AppError from Tauri", () => {
    const error = toApiError({ kind: "auth", message: "Canvas rejected the token" });
    expect(error).toBeInstanceOf(ApiError);
    expect(error.kind).toBe("auth");
    expect(error.message).toBe("Canvas rejected the token");
  });

  it("keeps every kind the facade sends, including the schema screens' kinds", () => {
    for (const kind of ["schema_too_new", "schema_too_old", "busy", "not_found"] as const) {
      expect(toApiError({ kind, message: "x" }).kind).toBe(kind);
    }
  });

  it("keeps the M1 details of a blocked or model error", () => {
    const blocked = toApiError({
      kind: "blocked",
      message: "Over this month's budget",
      blocked: "budget_reached",
      model_error: null,
      retry_after_secs: null,
    });
    expect(blocked).toMatchObject({
      kind: "blocked",
      blocked: "budget_reached",
      model_error: null,
    });
    const limited = toApiError({
      kind: "model",
      message: "Rate limited",
      model_error: "rate_limited",
      retry_after_secs: 20,
    });
    expect(limited).toMatchObject({ model_error: "rate_limited", retry_after_secs: 20 });
    expect(toApiError({ kind: "cancelled", message: "Cancelled" }).blocked).toBeNull();
  });

  it("treats unknown kinds, strings and Errors as internal", () => {
    expect(toApiError({ kind: "weird", message: "x" }).kind).toBe("internal");
    expect(toApiError("command not found").kind).toBe("internal");
    expect(toApiError(new Error("boom")).message).toBe("boom");
    expect(toApiError(undefined).kind).toBe("internal");
  });

  it("returns ApiError instances unchanged", () => {
    const original = new ApiError("busy", "syncing");
    expect(toApiError(original)).toBe(original);
    expect(isApiError(original, "busy")).toBe(true);
    expect(isApiError(original, "auth")).toBe(false);
  });
});
