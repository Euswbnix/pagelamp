// The This Week grouping, ported with the Tauri tests (thisWeek.test.ts) plus the Mac additions.

import Foundation
import PageLampKit
import PageLampModel
import Testing

@Suite("This Week grouping")
struct ThisWeekDigestTests {
    let now = TestClock.now
    let calendar = TestClock.calendar

    @Test("groups by calendar day, soonest first, with classes kept apart")
    func groupsByDay() {
        let groups = ThisWeekDigest.groupByDay(
            [
                deadline("Quiz", .quizDue, day: 3, hour: 9),
                deadline("Lecture", .classEvent, day: 1, hour: 10),
                deadline("Essay", .assignmentDue, day: 1, hour: 23),
                deadline("Lab", .assignmentDue, day: 1, hour: 8),
                deadline("Late tonight", .assignmentDue, day: 0, hour: 23),
            ],
            now: now,
            calendar: calendar
        )
        #expect(groups.map(\.dayOffset) == [0, 1, 3])
        #expect(groups[1].deadlines.map(\.event.title) == ["Lab", "Essay"])
        #expect(groups[1].classes.map(\.event.title) == ["Lecture"])
        #expect(groups[0].deadlines.map(\.event.title) == ["Late tonight"])
        #expect(groups[1].day == calendar.startOfDay(for: TestClock.at(1, 0)))
    }

    @Test("drops events without any time")
    func dropsUntimed() {
        let timed = deadline("No date", .other, day: 1, hour: 9)
        let untimed = Deadline(
            event: Event(
                id: "untimed", sourceId: "ical:test", courseId: nil, kind: .other, title: "No date",
                startsAt: nil, endsAt: nil, dueAt: nil, url: nil, updatedAt: now, courseHint: nil
            ),
            courseCode: nil,
            courseName: nil
        )
        #expect(ThisWeekDigest.groupByDay([untimed], now: now, calendar: calendar).isEmpty)
        #expect(ThisWeekDigest.groupByDay([timed, untimed], now: now, calendar: calendar).count == 1)
    }

    @Test("counts deadlines and exams but not class meetings")
    func countsDue() {
        let groups = ThisWeekDigest.groupByDay(
            [
                deadline("Lecture", .classEvent, day: 1, hour: 10),
                deadline("Essay", .assignmentDue, day: 1, hour: 23),
                deadline("Midterm", .exam, day: 2, hour: 18),
            ],
            now: now,
            calendar: calendar
        )
        #expect(ThisWeekDigest.countDue(groups) == 2)
    }

    @Test("keeps today and the next 6 calendar days, nothing on the 8th")
    func dayWindow() {
        let groups = ThisWeekDigest.groupByDay(
            [
                deadline("Due today", .assignmentDue, day: 0, hour: 23),
                deadline("Due in 6 days", .assignmentDue, day: 6, hour: 23),
                // 7 × 24 h from 10:00 today reaches 09:00 on day 7 — outside "this week".
                deadline("Early on day 7", .assignmentDue, day: 7, hour: 9),
                deadline("Yesterday", .assignmentDue, day: -1, hour: 23),
            ],
            now: now,
            calendar: calendar,
            days: 7
        )
        #expect(groups.map(\.dayOffset) == [0, 6])
    }

    @Test("next up: the earliest real deadline within 24 hours, never a class")
    func nextUp() {
        let soon = ThisWeekDigest(
            deadlines: [
                deadline("Lecture", .classEvent, day: 0, hour: 12),
                deadline("Tomorrow morning", .assignmentDue, day: 1, hour: 9),
                deadline("Later", .assignmentDue, day: 2, hour: 9),
            ],
            plan: nil,
            now: now,
            calendar: calendar
        )
        #expect(soon.nextUp?.event.title == "Tomorrow morning")
        #expect(soon.dueCount == 2)
        #expect(soon.classCount == 1)

        let none = ThisWeekDigest(
            deadlines: [deadline("In two days", .assignmentDue, day: 2, hour: 9)],
            plan: nil,
            now: now,
            calendar: calendar
        )
        #expect(none.nextUp == nil)
    }

    @Test("the mock's demo data: 3 deadlines, 1 class, 2 plan tasks today")
    func mockDigest() async throws {
        let mock = MockService(scenario: .demo, timing: .instant, calendar: calendar, now: { TestClock.now })
        let deadlines = try await mock.listDeadlines(course: nil, daysAhead: ThisWeekDigest.fetchDaysAhead, daysBack: 0)
        let plan = try await mock.latestStudyPlan()
        let digest = ThisWeekDigest(deadlines: deadlines, plan: plan, now: now, calendar: calendar)
        // Problem Set 2 (day 2), Quiz 3 (day 5), Exercise set 3 (day 6); Lecture 9 is a class;
        // Seminar presentation (day 9) and hidden DEMO099 are outside.
        #expect(digest.dueCount == 3)
        #expect(digest.classCount == 1)
        #expect(digest.groups.flatMap(\.deadlines).map(\.event.title) == ["Problem Set 2", "Quiz 3 — Sampling", "Exercise set 3"])
        #expect(digest.nextUp == nil)
        #expect(digest.planToday.map(\.title) == ["Read Chapter 4 and summarise sampling frames", "Work through Unit C examples"])
        #expect(digest.planSoon.count == 4)
    }
}
