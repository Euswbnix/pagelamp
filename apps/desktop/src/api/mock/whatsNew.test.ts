import { describe, expect, it } from "vitest";
import { compareVersions, topicsSince } from "./whatsNew";

describe("mock What's new", () => {
  it("orders versions like semver, pre-releases first", () => {
    const sorted = ["0.3.0", "0.3.0-beta.1", "0.3.0-alpha.10", "0.3.0-alpha.2", "0.2.9"].sort(
      compareVersions,
    );
    expect(sorted).toEqual(["0.2.9", "0.3.0-alpha.2", "0.3.0-alpha.10", "0.3.0-beta.1", "0.3.0"]);
  });

  it("gives an upgrader from alpha.1 only what came after it, and one from 0.1 everything", () => {
    expect(topicsSince("0.3.0-alpha.1")).toEqual([
      "course_removal",
      "syllabus_reading",
      "ai_writing",
      "reminders",
    ]);
    expect(topicsSince(null)).toHaveLength(6);
    expect(topicsSince("0.3.0-alpha.3")).toEqual(["ai_writing", "reminders"]);
    expect(topicsSince("0.3.0-beta.1")).toEqual([]);
  });
});
