// "This Week" grouping, ported from apps/desktop/src/features/courses/lib/thisWeek.ts: the M1
// fallback until the facade has `this_week(…)` (spec §2.8, §13 #2). The menu bar extra and
// reminders will read the same digest.

import Foundation
import PageLampKit

/// Deadlines and classes grouped by local calendar day, plus what the This Week page derives
/// from them (counts, the "Next up" deadline, today's plan).
public struct ThisWeekDigest: Equatable, Sendable {
    /// One calendar day.
    public struct DayGroup: Equatable, Sendable, Identifiable {
        /// Calendar days from today: 0 = today, 1 = tomorrow.
        public var dayOffset: Int
        /// Midnight of that day (for the heading).
        public var day: Date
        /// Things that are due (assignments, quizzes, exams, to-dos), soonest first.
        public var deadlines: [Deadline]
        /// Class meetings: shown, but quieter than deadlines.
        public var classes: [Deadline]

        public var id: Int { dayOffset }
    }

    /// Calendar days covered: today and the following `days - 1` days.
    public static let days = 7
    /// What to ask the core for: one extra day, trimmed back to whole calendar days here (the
    /// core's window is "now + N × 24 h", which reaches into an eighth day).
    public static let fetchDaysAhead: UInt32 = UInt32(days + 1)

    public var groups: [DayGroup]
    /// Real deadlines in the window (class meetings don't count).
    public var dueCount: Int
    public var classCount: Int
    /// The earliest real deadline due within the next 24 hours (spec §6.5), if any.
    public var nextUp: Deadline?
    /// Plan items for today, in plan order.
    public var planToday: [StudyPlanItem]
    /// Plan items for today and the next two days (the page shows these; the rest folds away).
    public var planSoon: [StudyPlanItem]

    /// When a deadline is due, or when an event starts.
    public static func time(of deadline: Deadline) -> Date? {
        deadline.event.dueAt ?? deadline.event.startsAt
    }

    /// Whole calendar days from `now` to `date` in `calendar` (tomorrow = 1).
    public static func dayOffset(of date: Date, from now: Date, calendar: Calendar) -> Int {
        let from = calendar.startOfDay(for: now)
        let to = calendar.startOfDay(for: date)
        return calendar.dateComponents([.day], from: from, to: to).day ?? 0
    }

    /// Groups events by local calendar day, soonest first. Events without a time are dropped;
    /// with `days` set only today and the following `days - 1` calendar days are kept.
    public static func groupByDay(
        _ deadlines: [Deadline], now: Date, calendar: Calendar, days: Int? = nil
    ) -> [DayGroup] {
        let timed = deadlines
            .compactMap { deadline in time(of: deadline).map { (deadline: deadline, time: $0) } }
            .enumerated()
            // Stable: equal times keep their input order (like Array.prototype.sort).
            .sorted { $0.element.time == $1.element.time ? $0.offset < $1.offset : $0.element.time < $1.element.time }
            .map(\.element)
        var groups: [Int: DayGroup] = [:]
        for (deadline, time) in timed {
            let offset = dayOffset(of: time, from: now, calendar: calendar)
            if let days, offset < 0 || offset >= days { continue }
            var group = groups[offset] ?? DayGroup(
                dayOffset: offset, day: calendar.startOfDay(for: time), deadlines: [], classes: []
            )
            if deadline.event.kind == .classEvent {
                group.classes.append(deadline)
            } else {
                group.deadlines.append(deadline)
            }
            groups[offset] = group
        }
        return groups.values.sorted { $0.dayOffset < $1.dayOffset }
    }

    /// Number of real deadlines in the groups (class meetings don't count).
    public static func countDue(_ groups: [DayGroup]) -> Int {
        groups.reduce(0) { $0 + $1.deadlines.count }
    }

    public init(
        deadlines: [Deadline],
        plan: StoredStudyPlan?,
        now: Date,
        calendar: Calendar,
        days: Int = ThisWeekDigest.days
    ) {
        groups = Self.groupByDay(deadlines, now: now, calendar: calendar, days: days)
        dueCount = Self.countDue(groups)
        classCount = groups.reduce(0) { $0 + $1.classes.count }
        let horizon = now.addingTimeInterval(24 * 60 * 60)
        nextUp = groups
            .flatMap(\.deadlines)
            .first { deadline in
                guard let time = Self.time(of: deadline) else { return false }
                return time >= now && time <= horizon
            }
        let today = IsoDate.string(from: now, calendar: calendar)
        let soon = Set((0 ... 2).compactMap { offset in
            calendar.date(byAdding: .day, value: offset, to: calendar.startOfDay(for: now))
                .map { IsoDate.string(from: $0, calendar: calendar) }
        })
        let items = plan?.plan.items ?? []
        planToday = items.filter { $0.date == today }
        planSoon = items.filter { soon.contains($0.date) }
    }
}
