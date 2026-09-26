import { describe, expect, it } from "vitest";
import { calendarDayDiff, formatIsoDate, formatRelative, todayIso } from "./format";

const now = new Date(2026, 8, 25, 12, 0, 0); // Fri 25 Sep 2026, local time

describe("format helpers", () => {
  it("counts calendar days, not 24h periods", () => {
    expect(calendarDayDiff(new Date(2026, 8, 26, 0, 30).toISOString(), now)).toBe(1);
    expect(calendarDayDiff(new Date(2026, 8, 25, 23, 59).toISOString(), now)).toBe(0);
    expect(calendarDayDiff(new Date(2026, 8, 23, 9, 0).toISOString(), now)).toBe(-2);
  });

  it("formats relative times in the UI language", () => {
    const twoHoursAgo = new Date(now.getTime() - 2 * 3600_000).toISOString();
    expect(formatRelative(twoHoursAgo, "en", now)).toBe("2 hours ago");
    expect(formatRelative(twoHoursAgo, "zh-CN", now)).toBe("2小时前");
    const tomorrow = new Date(2026, 8, 26, 18, 0).toISOString();
    expect(formatRelative(tomorrow, "en", now)).toBe("tomorrow");
  });

  it("formats calendar dates without a timezone shift", () => {
    expect(formatIsoDate("2026-09-08", "en")).toBe("Sep 8, 2026");
    expect(todayIso(now)).toBe("2026-09-25");
  });
});
