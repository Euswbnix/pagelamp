import { describe, expect, it } from "vitest";
import { ApiError, isApiError, toApiError } from "./errors";

describe("toApiError", () => {
  it("keeps the kind and message of a serialised AppError from Tauri", () => {
    const error = toApiError({ kind: "auth", message: "Canvas rejected the token" });
    expect(error).toBeInstanceOf(ApiError);
    expect(error.kind).toBe("auth");
    expect(error.message).toBe("Canvas rejected the token");
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
