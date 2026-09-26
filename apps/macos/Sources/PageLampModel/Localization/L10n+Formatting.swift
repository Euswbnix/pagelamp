// Text every screen needs, in the student's language (spec §7.3): numbers, weeks, relative
// times, and the names of backend codes. Swift maps codes (policies, source kinds, errors, event
// and material kinds), never the core's English.

import Foundation
import PageLampKit

extension L10n {
    /// A number in this locale.
    public func number(_ value: some BinaryInteger) -> String {
        Int(value).formatted(.number.locale(locale))
    }

    /// "Week 4" / "第 4 周", or "Week unknown".
    public func week(_ week: UInt32?) -> String {
        guard let week else { return self("common.week.unknown") }
        return self("common.week.current", ["week": number(week)])
    }

    /// The sidebar's and the Contents' compact week: "Wk 4" / "第 4 周", or "—".
    public func compactWeek(_ week: UInt32?) -> String {
        guard let week else { return self("mac.common.week.none") }
        return self("mac.common.week.compact", ["week": number(week)])
    }

    /// How long ago `date` was, in the spec's words (§2.3): "5 min ago", "2 h ago", "3 days ago" /
    /// "5 分钟前", "2 小时前", "3 天前". Numbers and short units, never "now" (under a minute is
    /// "1 min ago", like the Tauri app); minutes and hours are rounded, a day or more counts
    /// calendar days in `calendar`. A date after `now` (clock skew) reads as a minute ago.
    /// Plain string lookups: cheap enough for view bodies that re-render every minute.
    public func relative(_ date: Date, to now: Date, calendar: Calendar = .current) -> String {
        let seconds = max(0, now.timeIntervalSince(date))
        let minutes = max(1, Int((seconds / 60).rounded()))
        if minutes < 60 {
            return plural("mac.relative.minutesAgo", count: minutes)
        }
        if seconds < 86_400 {
            return plural("mac.relative.hoursAgo", count: max(1, Int((seconds / 3_600).rounded())))
        }
        let days = calendar.dateComponents(
            [.day], from: calendar.startOfDay(for: date), to: calendar.startOfDay(for: now)
        ).day ?? 1
        return plural("mac.relative.daysAgo", count: max(1, days))
    }

    /// The policy's name (policies are never colour-coded, spec §1.2).
    public func policy(_ policy: AiPolicy) -> String {
        switch policy {
        case .unknown: self("common.policy.unknown")
        case .prohibited: self("common.policy.prohibited")
        case .learningAid: self("common.policy.learning_aid")
        case .allowedWithCitation: self("common.policy.allowed_with_citation")
        case .unrestricted: self("common.policy.unrestricted")
        }
    }

    /// The source kind's name ("Canvas", "Course folder", "Calendar feed").
    public func sourceKind(_ kind: SourceKind) -> String {
        switch kind {
        case .canvas: self("common.sourceKind.canvas")
        case .folder: self("common.sourceKind.folder")
        case .ical: self("common.sourceKind.ical")
        }
    }

    /// Why a source failed, briefly: "Access expired", "Can't connect", … (`common.sourceError.*`).
    public func sourceError(_ kind: SourceErrorKind) -> String {
        switch kind {
        case .authExpiredOrRevoked: self("common.sourceError.auth_expired_or_revoked")
        case .network: self("common.sourceError.network")
        case .notFound: self("common.sourceError.not_found")
        case .rateLimited: self("common.sourceError.rate_limited")
        case .other: self("common.sourceError.other")
        }
    }

    /// "Assignment", "Quiz", "Class", …
    public func eventKind(_ kind: EventKind) -> String {
        switch kind {
        case .assignmentDue: self("common.eventKind.assignment_due")
        case .quizDue: self("common.eventKind.quiz_due")
        case .exam: self("common.eventKind.exam")
        case .classEvent: self("common.eventKind.class_event")
        case .plannerItem: self("common.eventKind.planner_item")
        case .other: self("common.eventKind.other")
        }
    }

    /// "File", "Page", "Syllabus", "Announcement", "Link".
    public func materialKind(_ kind: MaterialKind) -> String {
        switch kind {
        case .file: self("common.materialKind.file")
        case .page: self("common.materialKind.page")
        case .syllabus: self("common.materialKind.syllabus")
        case .announcement: self("common.materialKind.announcement")
        case .externalLink: self("common.materialKind.external_link")
        }
    }

    /// The fix's button title: "Replace Token…" / "Replace Feed Address…".
    public func fix(_ fix: CapsuleState.Attention.Fix) -> String {
        switch fix {
        case .replaceToken: self("mac.actions.replaceToken")
        case .replaceFeed: self("mac.actions.replaceFeed")
        }
    }
}
