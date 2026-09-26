// What the course detail page says, decided without any view (spec §3.2, §3.9 S7/S9/S11):
// the week line and its lamp, each material's status, the notes, the tinted action. Views turn
// the keys into text with L10n; policies are never colour-coded (spec §1.2 rule 6).

import Foundation
import PageLampKit

// MARK: - Week line

/// The header's week line: "Week 4" + a word tag ("This week", "In 2 weeks", …), and whether
/// the lamp is lit (spec §3.2 "Lit").
public struct CourseWeekLine: Equatable, Sendable {
    public enum Tag: Equatable, Sendable {
        case thisWeek
        case lastWeek
        case nextWeek
        case inWeeks(Int)
        case weeksAgo(Int)
        /// The current week, but today is outside the course's term dates.
        case outsideTerm
    }

    /// The week the line shows (nil = unknown).
    public let week: UInt32?
    public let tag: Tag?
    /// The pool of light: the line shows the current week and today is inside the term.
    public let lit: Bool
    public let outsideTerm: Bool

    /// - Parameters:
    ///   - section: the This Week section shows the selected week; the others the current week.
    ///   - selectedWeek: the week the student stepped to (nil = now).
    ///   - currentWeek: `timeline.current_week`.
    public init(section: CourseSection, selectedWeek: UInt32?, currentWeek: UInt32?, outsideTerm: Bool) {
        let shown = section == .week ? (selectedWeek ?? currentWeek) : currentWeek
        week = shown
        self.outsideTerm = outsideTerm
        if let shown, let currentWeek {
            let delta = Int(shown) - Int(currentWeek)
            tag = switch delta {
            case 0: outsideTerm ? .outsideTerm : .thisWeek
            case 1: .nextWeek
            case -1: .lastWeek
            case 2...: .inWeeks(delta)
            default: .weeksAgo(-delta)
            }
        } else {
            tag = nil
        }
        lit = shown != nil && shown == currentWeek && !outsideTerm
    }

    /// "Week 4" / "第 4 周"; "Outside term" or "Week unknown" without a week.
    public func title(_ l10n: L10n) -> String {
        if let week { return l10n("common.week.current", ["week": week.formatted(.number.locale(l10n.locale))]) }
        return outsideTerm ? l10n("common.week.outsideTerm") : l10n("common.week.unknown")
    }

    /// "This week" / "本周", "In 2 weeks" / "2 周后", …
    public func tagText(_ l10n: L10n) -> String? {
        switch tag {
        case nil: nil
        case .thisWeek: l10n("mac.course.week.relative.thisWeek")
        case .lastWeek: l10n("mac.course.week.relative.lastWeek")
        case .nextWeek: l10n("mac.course.week.relative.nextWeek")
        case .inWeeks(let count): l10n.plural("mac.course.week.relative.inWeeks", count: count)
        case .weeksAgo(let count): l10n.plural("mac.course.week.relative.weeksAgo", count: count)
        case .outsideTerm: l10n("common.week.outsideTerm")
        }
    }

    /// What VoiceOver reads for the line: "Week 4, this week".
    public func spoken(_ l10n: L10n) -> String {
        guard let tag = tagText(l10n) else { return title(l10n) }
        return l10n("mac.course.a11y.weekLine", ["week": title(l10n), "relative": tag])
    }
}

// MARK: - Materials

/// A material's status as the row shows it (spec §3.2 MaterialRow): a glyph and a short word,
/// with a longer phrase for VoiceOver. A course whose materials the AI app may not read says so
/// on every row (rule 8), whatever the extraction status.
public enum CourseMaterialStatus: Equatable, Sendable {
    case readable
    case waiting
    case notDownloaded(DownloadBlock?)
    case unreadable
    case extractFailed
    /// The course is marked No AI.
    case notShared
    /// The student turned the course's AI access off.
    case offForAI

    /// How the glyph is coloured (words are never coloured).
    public enum Tone: Equatable, Sendable {
        case neutral
        case success
        case warning
    }

    public init(material: MaterialView, aiMaterials: AiMaterialsState) {
        switch aiMaterials {
        case .withheldByPolicy:
            self = .notShared
        case .turnedOff:
            self = .offForAI
        case .readable:
            self = switch material.textStatus {
            case .ok: .readable
            case .pending: .waiting
            case .notDownloaded: .notDownloaded(material.downloadBlocked)
            case .unsupported: .unreadable
            case .error: .extractFailed
            }
        }
    }

    public var symbol: String {
        switch self {
        case .readable: "checkmark.circle"
        case .waiting: "hourglass"
        case .notDownloaded: "arrow.down.circle"
        case .unreadable: "minus.circle"
        case .extractFailed: "exclamationmark.triangle"
        case .notShared, .offForAI: "hand.raised"
        }
    }

    public var tone: Tone {
        switch self {
        case .readable: .success
        case .extractFailed: .warning
        default: .neutral
        }
    }

    /// "Readable" / "可读取", "Not shared" / "不共享", …
    public var shortKey: String {
        switch self {
        case .readable: "mac.materialStatus.ok"
        case .waiting: "mac.materialStatus.pending"
        case .notDownloaded: "mac.materialStatus.not_downloaded"
        case .unreadable: "mac.materialStatus.unsupported"
        case .extractFailed: "mac.materialStatus.error"
        case .notShared: "mac.materialStatus.withheld_by_policy"
        case .offForAI: "mac.materialStatus.turned_off"
        }
    }

    /// VoiceOver's phrase: "Readable by your AI app", "Can't be read (e.g. video)", …
    public var spokenKey: String {
        switch self {
        case .readable: "common.textStatus.ok"
        case .waiting: "common.textStatus.pending"
        case .notDownloaded: "common.textStatus.not_downloaded"
        case .unreadable: "common.textStatus.unsupported"
        case .extractFailed: "common.textStatus.error"
        case .notShared: "common.aiMaterials.withheld_by_policy"
        case .offForAI: "common.aiMaterials.turned_off"
        }
    }

    /// Why a not-downloaded Canvas file can't be downloaded on request ("Locked in Canvas").
    public var blockKey: String? {
        guard case .notDownloaded(let block?) = self else { return nil }
        return switch block {
        case .locked: "common.downloadBlock.locked"
        case .tooLarge: "common.downloadBlock.too_large"
        }
    }
}

/// How a material (or deadline) opens: a file from a course folder opens in its app, a web
/// address in the browser, anything else not at all.
public enum CourseLink: Equatable, Sendable {
    case file(URL)
    case web(URL)

    public init?(_ string: String?) {
        guard let string, let url = URL(string: string), let scheme = url.scheme?.lowercased() else { return nil }
        switch scheme {
        case "file": self = .file(url)
        case "http", "https": self = .web(url)
        default: return nil
        }
    }

    public var url: URL {
        switch self {
        case .file(let url), .web(let url): url
        }
    }
}

// MARK: - Notes, callouts and the tinted action

extension CourseDetailModel {
    /// The note above a week's materials (S11). `noMaterialsThisWeek` is left to the empty
    /// state, which says the same.
    public enum WeekNote: Equatable, Sendable {
        /// A localised note by code, with "Set Term Dates…" when the week is unknown or outside
        /// the term.
        case known(key: String, offersTermDates: Bool)
        /// A note without a known code: the backend's English, shown verbatim and tagged English.
        case backend(String)

        public init?(_ week: WeekMaterials) {
            switch week.noteKind {
            case .currentWeekUnknown:
                self = .known(key: "course.week.note.current_week_unknown", offersTermDates: true)
            case .outsideTerm:
                self = .known(key: "course.week.note.outside_term", offersTermDates: true)
            case .noMaterialsThisWeek:
                return nil
            case nil:
                guard let note = week.note, !note.isEmpty else { return nil }
                self = .backend(note)
            }
        }
    }

    /// Whether "Set Term Dates…" is a candidate for the page's tinted action (spec §3.0 (3)):
    /// the week is unknown, today is outside the term, or the guess has low confidence.
    public static func needsTermDates(_ timeline: CourseTimeline) -> Bool {
        timeline.currentWeek == nil || timeline.outsideTerm || timeline.confidence == .low
    }

    /// The page's candidates for its one tinted action, in page order (spec §3.0): the fix of
    /// the course's source (S7), then Set Term Dates…. Course detail has no page primary; Save AI
    /// Policy (1) is M2.
    public static func primaryActionCandidates(source: SourceRecord?, timeline: CourseTimeline) -> [PrimaryActionCandidate] {
        var candidates: [PrimaryActionCandidate] = []
        if let source, SourceProblem(source: source)?.fix != nil { candidates.append(.fixSource(source.id)) }
        if needsTermDates(timeline) { candidates.append(.setTermDates) }
        return candidates
    }
}

// MARK: - Dates

/// Course dates in the student's language and calendar (spec §7.3).
public enum CourseDates {
    /// A deadline's day: "Today" / "Tomorrow" / "Yesterday", else "Thu, Oct 9" ("10月9日 周四").
    public static func day(_ date: Date, now: Date, calendar: Calendar, l10n: L10n) -> String {
        let start = calendar.startOfDay(for: now)
        let days = calendar.dateComponents([.day], from: start, to: calendar.startOfDay(for: date)).day ?? 0
        switch days {
        case 0: return l10n("common.time.today")
        case 1: return l10n("common.time.tomorrow")
        case -1: return l10n("common.time.yesterday")
        default:
            return date.formatted(style(l10n, calendar).weekday(.abbreviated).month(.abbreviated).day())
        }
    }

    /// "11:59 PM" / "23:59" (the locale's clock).
    public static func time(_ date: Date, calendar: Calendar, l10n: L10n) -> String {
        date.formatted(style(l10n, calendar).hour().minute())
    }

    /// The full date and time for VoiceOver: "Friday, September 25, 11:59 PM".
    public static func spoken(_ date: Date, calendar: Calendar, l10n: L10n) -> String {
        date.formatted(style(l10n, calendar).weekday(.wide).month(.wide).day().hour().minute())
    }

    /// A material's publication day: "Sep 22" ("9月22日"); with the year when it isn't this year.
    public static func published(_ date: Date, now: Date, calendar: Calendar, l10n: L10n) -> String {
        let base = style(l10n, calendar).month(.abbreviated).day()
        let sameYear = calendar.component(.year, from: date) == calendar.component(.year, from: now)
        return date.formatted(sameYear ? base : base.year())
    }

    /// A term date ("2026-09-02") as "Sep 2, 2026" ("2026年9月2日"); nil for a malformed date.
    public static func term(_ isoDate: String?, calendar: Calendar, l10n: L10n) -> String? {
        guard let isoDate, let date = IsoDate.date(from: isoDate, calendar: calendar) else { return nil }
        return date.formatted(style(l10n, calendar).year().month(.abbreviated).day())
    }

    /// Whether `date` is on the same calendar day as `now` (due today → Semibold title).
    public static func isToday(_ date: Date, now: Date, calendar: Calendar) -> Bool {
        calendar.isDate(date, inSameDayAs: now)
    }

    private static func style(_ l10n: L10n, _ calendar: Calendar) -> Date.FormatStyle {
        Date.FormatStyle(locale: l10n.locale, calendar: calendar, timeZone: calendar.timeZone)
    }
}
