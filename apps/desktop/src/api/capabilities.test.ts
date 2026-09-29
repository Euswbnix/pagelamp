// The webview may never install or redirect updates itself: the updater is driven from Rust
// commands only (M0.4). Guard the capability file against an `updater:*` permission creeping in.

import { expect, it } from "vitest";
import capabilities from "../../src-tauri/capabilities/default.json";

it("grants the webview no updater permission", () => {
  const ids = capabilities.permissions.map((p) => (typeof p === "string" ? p : p.identifier));
  expect(ids.length).toBeGreaterThan(0);
  expect(ids.filter((id) => id.startsWith("updater"))).toEqual([]);
});
