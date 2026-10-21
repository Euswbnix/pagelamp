import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { CODE, SentenceWithCode } from "@/components/common/SentenceWithCode";
import { SentenceWithTime, WHEN, WHEN_2 } from "@/components/common/SentenceWithTime";
import i18n from "@/i18n";

// Instructor-controlled text (course, material and deadline titles) as an attacker would write it.
const HOSTILE = "Quiz {{when}} {{product}} $t(common:errors.internal) {{- when}} <b>x</b>";

describe("interpolated values are data, never template", () => {
  const courses = i18n.getFixedT("en", "courses");

  it("keeps placeholders, nesting and HTML inside a value as literal text", () => {
    expect(courses("card.next", { title: HOSTILE, when: "tomorrow" })).toBe(
      `Next: ${HOSTILE} · tomorrow`,
    );
    const course = i18n.getFixedT("en", "course");
    expect(course("header.freshness", { source: "{{when}}", when: "2 hours ago" })).toBe(
      "Data from {{when}} · synced 2 hours ago",
    );
  });

  it("does so in Chinese too, whatever the word order", () => {
    const zh = i18n.getFixedT("zh-CN", "courses");
    const text = zh("card.next", { title: HOSTILE, when: "明天" });
    expect(text).toContain(HOSTILE);
    expect(text).toContain("明天");
    expect(text).not.toContain("{{title}}");
  });

  it("keeps the AI screens' course and service names literal", () => {
    const ai = i18n.getFixedT("en", "ai");
    expect(ai("sharing.reminder.body", { course: "{{service}}", service: "OpenAI" })).toBe(
      "PageLamp sent text from {{service}}'s materials to OpenAI. If you're not sure it's allowed, check the syllabus or ask your instructor.",
    );
    expect(ai("backend.removeTitle", { name: "$t(ai:settings.title)" })).toBe(
      "Remove $t(ai:settings.title)?",
    );
  });

  it("still fills the product name and plain values", () => {
    expect(i18n.getFixedT("en", "ai")("settings.title")).toBe("AI models");
    expect(courses("card.next", { title: "Essay 1", when: "tomorrow" })).toBe(
      "Next: Essay 1 · tomorrow",
    );
    expect(i18n.getFixedT("en", "ai")("disclosure.title", { name: "OpenAI" })).toBe(
      "Before PageLamp uses OpenAI",
    );
  });

  it("puts the time where the sentence says, not inside a hostile title", () => {
    const text = courses("card.next", { title: "Quiz {{when}} \u0000 end", when: WHEN });
    const { container } = render(<SentenceWithTime text={text} iso="2026-10-01T12:00:00Z" />);
    expect(container.textContent?.startsWith("Next: Quiz {{when}} � end · ")).toBe(true);
    expect(container.querySelectorAll("time")).toHaveLength(1);
    expect(container.lastChild?.nodeName).toBe("TIME");
  });

  it("never lets a value that is only a marker's character stand for its slot", () => {
    // A course code from the LMS can be exactly the character a slot uses.
    for (const title of ["\u0000", "\u0001"]) {
      const text = courses("card.next", { title, when: WHEN });
      const { container, unmount } = render(
        <SentenceWithTime text={text} iso="2026-10-01T12:00:00Z" iso2="2026-10-02T12:00:00Z" />,
      );
      expect(container.textContent?.startsWith("Next: \uFFFD · ")).toBe(true);
      expect(container.querySelectorAll("time")).toHaveLength(1);
      expect(container.lastChild?.nodeName).toBe("TIME");
      unmount();
    }
    // Two times: each where the sentence puts it, none where the source's name is.
    const course = i18n.getFixedT("en", "course");
    const text = course("header.freshnessWithDeadlines", {
      source: "\u0001",
      when: WHEN,
      deadlines: WHEN_2,
    });
    const { container } = render(
      <SentenceWithTime text={text} iso="2026-10-01T12:00:00Z" iso2="2026-10-02T12:00:00Z" />,
    );
    expect(container.textContent?.startsWith("Data from \uFFFD · synced ")).toBe(true);
    expect([...container.querySelectorAll("time")].map((t) => t.dateTime)).toEqual([
      "2026-10-01T12:00:00Z",
      "2026-10-02T12:00:00Z",
    ]);
  });

  it("puts a command in code where the sentence says, whatever a course is called", () => {
    const calendar = i18n.getFixedT("en", "calendar");
    const text = calendar("sameSession.skipped.own_calendar", {
      count: 1,
      courses: "\u0000",
      command: CODE,
    });
    const { container } = render(<SentenceWithCode text={text} code="pagelamp course calendar" />);
    expect(container.textContent).toBe(
      "Not used in \uFFFD: it counts its weeks from a calendar of its own. Change that calendar with the pagelamp course calendar command.",
    );
    expect(container.querySelector("code")?.textContent).toBe("pagelamp course calendar");
  });
});
