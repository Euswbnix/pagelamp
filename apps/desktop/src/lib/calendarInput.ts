// A course calendar as the dates form's input, and the student's choices applied to it. Used
// when a proposal is accepted with choices or edited (calendar design §7.10); the facade still
// validates whatever is sent.

import type {
  AlternativeDate,
  CourseCalendar,
  CourseDatesInput,
  DateKind,
  TermResolution,
} from "@/api/types";

export function calendarToInput(calendar: CourseCalendar): CourseDatesInput {
  const [first, second] = calendar.segments;
  return {
    first_class: first?.first_class ?? null,
    last_class: first?.last_class ?? null,
    exams_end: calendar.exam_period?.end ?? null,
    breaks: calendar.breaks.map((b) => ({
      kind: b.kind,
      start: b.span.start,
      end: b.span.end,
      numbered: b.numbered,
      label: b.label || null,
    })),
    second_segment: second
      ? {
          first_class: second.first_class,
          last_class: second.last_class ?? null,
          restart_numbering: second.first_week_number <= 1,
        }
      : null,
  };
}

/** The dates form's starting point for a proposal (the same shape the resolver gives it). */
export function calendarAsTerm(calendar: CourseCalendar): TermResolution {
  return {
    week_one_monday: null,
    teaching: calendar.segments,
    breaks: calendar.breaks,
    exams_end: calendar.exam_period?.end ?? null,
    anchor: "none",
    anchor_confidence: "low",
    anchor_origin: null,
    ai_label: null,
    outer_frame: null,
    not_used: [],
    student_start: null,
    student_end: null,
  };
}

/** `input` with one date replaced by the student's choice. */
export function applyChoice(
  input: CourseDatesInput,
  kind: DateKind,
  segment: number,
  choice: AlternativeDate,
): CourseDatesInput {
  const second = input.second_segment;
  switch (kind) {
    case "first_class":
      return segment === 1 && second
        ? { ...input, second_segment: { ...second, first_class: choice.date } }
        : { ...input, first_class: choice.date };
    case "last_class":
      return segment === 1 && second
        ? { ...input, second_segment: { ...second, last_class: choice.date } }
        : { ...input, last_class: choice.date };
    case "exam_period":
      return { ...input, exams_end: choice.end ?? choice.date };
    default:
      return input;
  }
}
