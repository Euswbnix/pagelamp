// What the This Week page derives beyond the day grouping (spec §3.1, §3.9, §6.4, §6.5): the
// page's state (S3/S4/first sync), the study plan by day, the Contents order and each course's
// week, and the Next up countdown. Pure values, so the tests and the snapshots share them.

import Foundation
import PageLampKit

/// What the This Week page shows as a whole.
public enum ThisWeekPageState: Equatable, Sendable {
    /// The reading page: callouts, Next 7 days, the study plan and the Contents.
    case page
    /// S3: no sources yet.
    case noSources
    /// S4: sources, but they haven't brought in any courses.
    case noCourses
    /// No courses yet while a sync runs (the M1 part of S5): they are on their way.
    case firstSync

    /// - Parameters:
    ///   - coursesFailed: `list_courses()` failed; the page stays up and its Contents section
    ///     shows the error (S14), since an error is not "no courses".
    public static func resolve(
        courseCount: Int, sourceCount: Int, syncing: Bool, coursesFailed: Bool
    ) -> ThisWeekPageState {
        if coursesFailed || courseCount > 0 { return .page }
        if syncing { return .firstSync }
        return sourceCount == 0 ? .noSources : .noCourses
    }
}

extension AppModel {
    /// This Week's state from the shell data (S3, S4, first sync, or the page).
    public var thisWeekPageState: ThisWeekPageState {
        ThisWeekPageState.resolve(
            courseCount: courses.count,
            sourceCount: sources.count,
            syncing: isSyncing || status?.syncInProgress == true,
            coursesFailed: sectionErrors[.courses] != nil
        )
    }

    /// The fixes This Week offers for failing sources (spec §3.0 (2)): one per rejected token or
    /// feed address, first failing source first.
    public var thisWeekFixCandidates: [PrimaryActionCandidate] {
        failingSources
            .filter { SourceProblem(source: $0)?.fix != nil }
            .map { .fixSource($0.id) }
    }
}

// MARK: - Next up

extension ThisWeekDigest {
    /// The earliest real deadline still ahead of `now`, for the header's "Next: …" line when
    /// nothing is due within 24 hours. (The page re-renders every minute between refreshes, so
    /// a deadline that passed earlier today can still be in the groups.)
    public func nextDeadline(after now: Date) -> Deadline? {
        groups.lazy.flatMap(\.deadlines).first { deadline in
            Self.time(of: deadline).map { $0 >= now } ?? false
        }
    }
}

/// The quiet countdown of Next up (spec §6.5): whole minutes, never seconds.
public struct Countdown: Equatable, Sendable {
    /// Minutes left, rounded up, at least 1 (a deadline 30 s away is "in 1 min", never "in 0").
    public let minutes: Int

    public init(from now: Date, to due: Date) {
        let seconds = max(0, due.timeIntervalSince(now))
        minutes = max(1, Int((seconds / 60).rounded(.up)))
    }

    public var hours: Int { minutes / 60 }
    public var minutesPastHour: Int { minutes % 60 }

    /// Under 2 h only the glyph changes (`clock` → `clock.badge.exclamationmark`); no colour.
    public var isFinalStretch: Bool { minutes < 120 }
}

// MARK: - Contents

/// Where a course is this week, for the Contents list and its VoiceOver label.
public enum CourseWeekState: Equatable, Sendable {
    case week(UInt32)
    /// The core couldn't work out the week (S11): "—" plus "Set the term start to fix this".
    case unknown
    /// Today is outside the course's term dates (S11): "—".
    case outsideTerm

    public init(_ timeline: CourseTimeline) {
        if timeline.outsideTerm {
            self = .outsideTerm
        } else if let week = timeline.currentWeek {
            self = .week(week)
        } else {
            self = .unknown
        }
    }
}

public enum ThisWeekContents {
    /// The Contents order (spec §6.4): courses in the course list by code, past courses last.
    /// Hidden courses wait for View ▸ Show Hidden Courses [M2].
    public static func courses(_ all: [CourseSummary]) -> [CourseSummary] {
        all
            .filter { !$0.course.hidden }
            .sorted { lhs, rhs in
                if lhs.course.enrollmentActive != rhs.course.enrollmentActive {
                    return lhs.course.enrollmentActive
                }
                return label(lhs).localizedStandardCompare(label(rhs)) == .orderedAscending
            }
    }

    /// Whether every course is hidden (the Contents then says so instead of listing nothing).
    public static func allHidden(_ all: [CourseSummary]) -> Bool {
        !all.isEmpty && all.allSatisfy(\.course.hidden)
    }

    /// The course's short label: its code, else its name.
    public static func label(_ summary: CourseSummary) -> String {
        summary.course.code ?? summary.course.name
    }

    /// The short label for a study-plan item's course (ported from Tauri's `courseLabelFor`).
    /// Plan items name a course by id or sometimes by code; an unknown id
    /// ("canvas:host/course/42") isn't worth showing, a bare unknown code is.
    public static func planCourseLabel(_ idOrCode: String?, courses: [CourseSummary]) -> String? {
        guard let idOrCode, !idOrCode.isEmpty else { return nil }
        if let match = courses.first(where: { $0.course.id == idOrCode || $0.course.code == idOrCode }) {
            return label(match)
        }
        return idOrCode.contains(where: { $0 == ":" || $0 == "/" }) ? nil : idOrCode
    }
}

// MARK: - Study plan

/// The latest study plan by day (read-only, spec §3.1): today and the next two days up front,
/// the rest behind Show Full Plan. Ported from Tauri's `lib/plan.ts`.
public struct StudyPlanDigest: Equatable, Sendable {
    /// One task, with its position in the saved plan (plan items have no id; a saved plan never
    /// changes, so the position is a stable identity).
    public struct Entry: Equatable, Sendable, Identifiable {
        public var position: Int
        public var item: StudyPlanItem
        public var id: Int { position }
    }

    /// The tasks of one calendar date, in plan order.
    public struct Day: Equatable, Sendable, Identifiable {
        /// "YYYY-MM-DD".
        public var date: String
        /// Midnight of that date, or nil if the plan's date is malformed.
        public var day: Date?
        public var entries: [Entry]
        public var id: String { date }
    }

    /// A plan older than this is probably out of date, even if its horizon has not ended.
    public static let staleAfterDays = 7
    /// The collapsed plan shows today plus the next 2 days.
    public static let focusDays = 3

    /// Every day of the plan, earliest first.
    public var days: [Day]
    /// Today and the next `focusDays - 1` days.
    public var focus: [Day]
    /// The rest (earlier and later days), earliest first: what Show Full Plan adds.
    public var others: [Day]
    /// The horizon has ended, or the plan was made more than a week ago.
    public var isStale: Bool

    public init(_ stored: StoredStudyPlan, now: Date, calendar: Calendar) {
        var byDate: [String: [Entry]] = [:]
        for (position, item) in stored.plan.items.enumerated() {
            byDate[item.date, default: []].append(Entry(position: position, item: item))
        }
        // "YYYY-MM-DD" sorts by date as a string.
        days = byDate.keys.sorted().map { date in
            Day(date: date, day: IsoDate.date(from: date, calendar: calendar), entries: byDate[date] ?? [])
        }
        let today = IsoDate.string(from: now, calendar: calendar)
        let lastFocus = calendar.date(
            byAdding: .day, value: Self.focusDays - 1, to: calendar.startOfDay(for: now)
        ).map { IsoDate.string(from: $0, calendar: calendar) } ?? today
        focus = days.filter { $0.date >= today && $0.date <= lastFocus }
        others = days.filter { !($0.date >= today && $0.date <= lastFocus) }
        let age = now.timeIntervalSince(stored.createdAt)
        isStale = stored.plan.horizonEnd < today || age > Double(Self.staleAfterDays) * 86_400
    }
}
