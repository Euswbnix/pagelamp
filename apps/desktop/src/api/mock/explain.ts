// Weekly explanations in the mock (M3; design §5.2): the course's rules first (a course that isn't
// readable is blocked with its reason), the AI gate, the run's events (no text before the end),
// an explanation of the week's readable materials with every paragraph cited, and the last 5 kept
// per course and week, newest first.

import { type GenEvent, materialSharing } from "../ai";
import type { PageLampApi } from "../client";
import { ApiError } from "../errors";
import type { WeeklyExplanation } from "../explain";
import type { LeftOutMaterial } from "../plan";
import { aiMaterialsState, type MaterialView } from "../types";
import type { MockActivity } from "./activity";
import type { MockCourse } from "./fixtures";
import type { MockAiRun } from "./proposals";

type ExplainApi = Pick<
  PageLampApi,
  "explainWeek" | "savedExplanations" | "aiOutputLanguage" | "setAiOutputLanguage"
>;

const KEEP = 5;

export function createExplainMock(deps: {
  now: () => Date;
  respond: <T>(value: T | (() => T), extraLatency?: number) => Promise<T>;
  step: () => Promise<void>;
  activity: MockActivity;
  gate: (courseId: string, week: number | null, overrideBudget: boolean) => Promise<MockAiRun>;
  findCourse: (courseId: string) => MockCourse;
}): ExplainApi & { cancel: (generationId: string) => void } {
  const { now, respond } = deps;
  const saved = new Map<string, WeeklyExplanation[]>();
  const cancelled = new Set<string>();
  const running = new Set<string>();
  const reminded = new Set<string>();
  let language: "ui" | "course" = "ui";

  const key = (courseId: string, week: number | null) => `${courseId}#${week ?? "default"}`;

  async function write(
    courseId: string,
    requestedWeek: number | null,
    generationId: string,
    include: string[],
    overrideBudget: boolean,
    onEvent: (event: GenEvent) => void,
  ): Promise<WeeklyExplanation> {
    const c = deps.findCourse(courseId);
    const state = aiMaterialsState(c.course);
    if (c.course.hidden) {
      throw new ApiError("blocked", "The course is hidden.", { blocked: "course_hidden" });
    }
    if (state === "withheld_by_policy") {
      throw new ApiError("blocked", "The course's AI policy withholds its materials.", {
        blocked: "course_policy_prohibited",
      });
    }
    if (state === "turned_off") {
      throw new ApiError("blocked", "AI access is off for this course.", {
        blocked: "course_ai_turned_off",
      });
    }
    const week = requestedWeek ?? c.timeline.default_week ?? c.timeline.current_week ?? 1;
    const materials = c.materials.filter((m) => m.week_hint === week);
    const readable = materials.filter((m) => m.text_status === "ok");
    if (readable.length === 0) {
      throw new ApiError("blocked", "No readable materials this week.", {
        blocked: "no_readable_materials",
      });
    }
    const run = await deps.gate(c.course.id, week, overrideBudget);
    const stop = () => {
      if (cancelled.has(generationId)) {
        onEvent({ type: "finished", ok: false });
        throw new ApiError("cancelled", "The explanation was stopped.");
      }
    };

    // The first two readable materials are read; the rest are left out for the budget, unless
    // the student included them.
    const read = readable.filter((m, i) => i < 2 || include.includes(m.id));
    const leftOut: LeftOutMaterial[] = [
      ...readable
        .filter((m) => !read.includes(m))
        .map((m) => ({ material_id: m.id, title: m.title, reason: "over_budget" as const })),
      ...materials
        .filter((m) => m.text_status !== "ok")
        .map((m) => ({ material_id: m.id, title: m.title, reason: "no_text" as const })),
    ];
    onEvent({ type: "stage", stage: "building_context" });
    await deps.step();
    stop();
    const summary = {
      courses: [{ course_id: c.course.id, state, text_included: true }],
      left_out: leftOut,
      materials_included: read.length,
      materials_trimmed: 0,
    };
    onEvent({ type: "context", summary, input_tokens: 6_400 * read.length });
    onEvent({ type: "started", generation_id: generationId, ...run });
    onEvent({ type: "stage", stage: "waiting_for_model" });
    await deps.step();
    stop();
    const usage = {
      input_tokens: 6_400 * read.length,
      cached_input_tokens: 0,
      output_tokens: 1_100,
    };
    onEvent({ type: "usage", usage });
    onEvent({ type: "stage", stage: "validating" });
    await deps.step();
    stop();
    onEvent({ type: "finished", ok: true });

    const answer = materialSharing(c.course);
    const reminder =
      !run.on_device &&
      (answer === "unanswered" || answer === "not_sure") &&
      !reminded.has(c.course.id);
    if (reminder) reminded.add(c.course.id);
    const explanation: WeeklyExplanation = {
      meta: {
        backend_label: run.backend_label,
        model: run.model,
        on_device: run.on_device,
        created_at: now().toISOString(),
        generation_id: generationId,
        feature: "weekly_explanation",
        estimated: false,
        prompt_version: 1,
        usage,
        est_cost_micro_usd: run.on_device ? null : 3_100,
        context: summary,
      },
      course_id: c.course.id,
      week,
      sections: read.map((m, i) => ({
        heading: m.title,
        paragraphs: [
          {
            text: `The key idea of **${m.title}** is how this week's topic builds on the last one.`,
            citations: [cite(m, i + 1, "p. 2")],
          },
          {
            text: "Work through the example at the end before the next lecture.",
            citations: [cite(m, i + 1, "p. 5")],
          },
        ],
      })),
      check_questions: read.map((m) => `What is the main point of ${m.title}?`),
      left_out: leftOut,
      stale: false,
      sharing_reminder: reminder,
      dropped_citations: 0,
      cite_ai_use: c.course.ai_policy === "allowed_with_citation",
    };
    const list = saved.get(key(c.course.id, week)) ?? [];
    saved.set(key(c.course.id, week), [explanation, ...list].slice(0, KEEP));
    return explanation;
  }

  return {
    explainWeek: async (courseId, week, generationId, options, onEvent) => {
      if (running.has(generationId))
        throw new ApiError("busy", "This explanation is being written.");
      running.add(generationId);
      try {
        return await deps.activity.during("generation", { generation_id: generationId }, () =>
          write(
            courseId,
            week,
            generationId,
            options.include ?? [],
            options.override_budget ?? false,
            onEvent,
          ),
        );
      } finally {
        running.delete(generationId);
        cancelled.delete(generationId);
      }
    },
    savedExplanations: (courseId, week) =>
      respond(() => {
        const c = deps.findCourse(courseId);
        const resolved = week ?? c.timeline.default_week ?? c.timeline.current_week ?? 1;
        return saved.get(key(c.course.id, resolved)) ?? [];
      }),
    aiOutputLanguage: () => respond(language),
    setAiOutputLanguage: (next) =>
      respond(() => {
        language = next;
      }),
    cancel: (generationId) => {
      if (running.has(generationId)) cancelled.add(generationId);
    },
  };
}

function cite(material: MaterialView, n: number, locator: string) {
  return {
    handle: `M${n}`,
    material_id: material.id,
    title: material.title,
    locator,
    url: material.url ?? null,
  };
}
