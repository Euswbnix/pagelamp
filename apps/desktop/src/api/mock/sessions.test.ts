import { expect, it } from "vitest";
import { sessionKey } from "./sessions";

it("groups a Fall or Winter session's F, S and unlettered sections, and keeps Y apart", () => {
  const fall = sessionKey("DEM332H5 F LEC0101 20269");
  expect(fall).toEqual({ id: "20269/FS/5", session: "20269", campus: 5 });
  expect(sessionKey("DEM240H5 S LEC0101 20269")?.id).toBe(fall?.id);
  expect(sessionKey("DEM180Y5 Y LEC0101 20269")?.id).not.toBe(fall?.id);
  // Another campus or another session is another group.
  expect(sessionKey("DEM332H1 F LEC0101 20269")?.id).not.toBe(fall?.id);
  expect(sessionKey("DEM332H5 F LEC0101 20271")?.id).not.toBe(fall?.id);
});

it("keeps a summer session's F, S and Y sections apart", () => {
  const ids = ["F", "S", "Y"].map((s) => sessionKey(`DEM101H5 ${s} LEC0101 20265`)?.id);
  expect(new Set(ids).size).toBe(3);
});

it("finds no session in a code that doesn't say one", () => {
  expect(sessionKey("PHS110")).toBeNull();
  expect(sessionKey(null)).toBeNull();
});
