// Weekly explanations (v0.3 M3; design §5.2): the facade's types (generated.ts).

import type { LeftOutReason } from "./generated";

export type {
  Citation,
  ExplainOptions,
  ExplanationParagraph,
  ExplanationSection,
  OutputLanguage,
  WeeklyExplanation,
} from "./generated";

/**
 * The one left-out reason `ExplainOptions.include` brings back (pagelamp-core's
 * week_context_including): a material that looks like graded work, once the student says it
 * isn't. Materials over the length limit stay out whatever `include` says.
 */
export const INCLUDABLE_REASON: LeftOutReason = "looks_like_assessment";
