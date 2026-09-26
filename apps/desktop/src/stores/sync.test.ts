import { describe, expect, it } from "vitest";
import { ApiError } from "@/api/errors";
import { useSyncStore } from "./sync";

describe("sync store", () => {
  it("folds SyncEvents into per-source progress", () => {
    const store = useSyncStore.getState();
    store.begin(2);
    store.apply({ type: "source_started", source_id: "a", label: "Course folder" });
    store.apply({ type: "progress", source_id: "a", message: "Indexing", current: 2, total: 5 });
    store.apply({ type: "warning", source_id: "a", message: "Skipped a video" });
    store.apply({ type: "source_started", source_id: "b", label: "Demo Canvas" });
    store.apply({
      type: "source_finished",
      source_id: "b",
      ok: false,
      error: "401",
      error_kind: "auth_expired_or_revoked",
    });

    const s = useSyncStore.getState();
    expect(s.running).toBe(true);
    expect(s.total).toBe(2);
    expect(s.order).toEqual(["a", "b"]);
    expect(s.bySource.a).toMatchObject({
      label: "Course folder",
      current: 2,
      total: 5,
      warnings: ["Skipped a video"],
      result: null,
    });
    expect(s.bySource.b?.result).toEqual({
      ok: false,
      error: "401",
      errorKind: "auth_expired_or_revoked",
    });
  });

  it("marks sources that never finished as stopped when the run ends", () => {
    const store = useSyncStore.getState();
    store.begin(2);
    store.apply({ type: "source_started", source_id: "a", label: "Course folder" });
    store.apply({ type: "source_finished", source_id: "a", ok: true });
    store.apply({ type: "source_started", source_id: "b", label: "Demo Canvas" });
    store.finish(null, new ApiError("internal", "boom"));

    const s = useSyncStore.getState();
    expect(s.running).toBe(false);
    expect(s.bySource.a?.stopped).toBe(false);
    expect(s.bySource.b?.stopped).toBe(true);
  });

  it("keeps a dismissed run error dismissed until the next run", () => {
    const store = useSyncStore.getState();
    store.begin(1);
    store.finish(null, new ApiError("network", "offline"));
    store.dismissRunError();
    expect(useSyncStore.getState().runError).toBeNull();
    store.begin(1);
    store.finish(null, new ApiError("busy", "locked"));
    expect(useSyncStore.getState().runError?.kind).toBe("busy");
  });
});
