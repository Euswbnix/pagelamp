import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { describe, expect, it } from "vitest";
import { INCLUDABLE_REASON } from "./explain";
import { ASSESSMENT_WORDS, looksLikeAssessment, STUDY_WORDS } from "./mock/explain";

// The facade's rules, in pagelamp-core: found from this test file, wherever the tests run from.
const testFile = expect.getState().testPath ?? "";
const builders = readFileSync(
  join(dirname(testFile), "../../../../crates/pagelamp-core/src/ai_gate/builders.rs"),
  "utf8",
);
const snake = (name: string) => name.replace(/(?<!^)([A-Z])/g, "_$1").toLowerCase();
const words = (constant: string) => {
  const list = builders.match(new RegExp(`const ${constant}: \\[&str; \\d+\\] = \\[([^\\]]*)\\]`));
  if (!list?.[1]) throw new Error(`${constant} not found in builders.rs`);
  return [...list[1].matchAll(/"([^"]+)"/g)].map((m) => m[1]);
};

describe("Explain's include, against the facade", () => {
  it("offers include for exactly the reason week_context_including brings back", () => {
    const lifted = [...builders.matchAll(/LeftOutReason::(\w+) && include\.contains/g)].map((m) =>
      snake(m[1] ?? ""),
    );
    expect(lifted).toEqual([INCLUDABLE_REASON]);
  });

  it("marks graded-looking titles with the facade's words", () => {
    expect(ASSESSMENT_WORDS).toEqual(words("ASSESSMENT_WORDS"));
    expect(STUDY_WORDS).toEqual(words("STUDY_WORDS"));
    expect(looksLikeAssessment("Assignment 4 — Survey Simulation")).toBe(true);
    expect(looksLikeAssessment("HW3 solutions")).toBe(false);
    expect(looksLikeAssessment("hw3")).toBe(true);
    expect(looksLikeAssessment("Midterm review")).toBe(false);
  });
});
