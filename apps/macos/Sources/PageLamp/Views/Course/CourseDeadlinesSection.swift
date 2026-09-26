// The course page's Deadlines section (spec §3.2 W3a): the rule-4 note first, then Coming up
// (14 days) and Recently past (7 days, secondary). Rows end with a labelled Open in Browser.

import SwiftUI
import PageLampKit
import PageLampModel

struct CourseDeadlinesSection: View {
    let detail: CourseDetailModel

    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n

    var body: some View {
        VStack(alignment: .leading, spacing: PLLayout.sectionGapCourse) {
            // Rule 4, always first and never truncated.
            HStack(alignment: .firstTextBaseline, spacing: PLSpace.s3) {
                Image(systemName: "info.circle")
                    .symbolRenderingMode(.hierarchical)
                    .foregroundStyle(.secondary)
                    .accessibilityHidden(true)
                Text(l10n("course.deadlines.note"))
                    .font(PLType.body.font)
                    .foregroundStyle(.secondary)
                    .paragraphLineSpacing()
                    .fixedSize(horizontal: false, vertical: true)
            }
            switch detail.deadlines {
            case .loading:
                CoursePlaceholderRows(rows: 3)
            case .failed(let failure):
                SectionError(title: l10n("course.deadlines.loadError"), message: failure.localizedDescription(in: l10n)) {
                    Task { await detail.loadDeadlines(using: model) }
                }
            case .loaded(let deadlines):
                upcoming(deadlines.upcoming)
                past(deadlines.past)
            }
        }
    }

    private func upcoming(_ deadlines: [Deadline]) -> some View {
        let days = l10n.number(CourseDeadlines.daysAhead)
        return VStack(alignment: .leading, spacing: PLSpace.s2) {
            SectionHeader(
                title: l10n("course.deadlines.upcomingTitle"),
                detail: l10n("course.deadlines.upcomingDescription", ["days": days])
            )
            if deadlines.isEmpty {
                QuietState(
                    symbol: "calendar",
                    title: l10n("course.deadlines.empty.title", ["days": days]),
                    message: l10n("course.deadlines.empty.description")
                ) {
                    Button(l10n("mac.actions.openSourcesAndSync")) { model.destination = .sources }
                        .buttonStyle(.bordered)
                }
                .padding(.top, PLSpace.s2)
            } else {
                CourseDeadlineList(deadlines: deadlines, isPast: false)
            }
        }
    }

    private func past(_ deadlines: [Deadline]) -> some View {
        let days = l10n.number(CourseDeadlines.daysBack)
        return VStack(alignment: .leading, spacing: PLSpace.s2) {
            SectionHeader(
                title: l10n("course.deadlines.pastTitle"),
                detail: l10n("course.deadlines.pastDescription", ["days": days])
            )
            if deadlines.isEmpty {
                Text(l10n("course.deadlines.pastEmpty", ["days": days]))
                    .font(PLType.callout.font)
                    .foregroundStyle(.secondary)
                    .padding(.top, PLSpace.s1)
            } else {
                CourseDeadlineList(deadlines: deadlines, isPast: true)
            }
        }
    }
}

/// Rows of deadlines with hairlines between them, and a Deadlines rotor.
struct CourseDeadlineList: View {
    let deadlines: [Deadline]
    let isPast: Bool
    @Environment(\.l10n) private var l10n
    @Environment(\.detailColumnWidth) private var detailWidth

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(deadlines.enumerated()), id: \.element.event.id) { index, deadline in
                CourseDeadlineRow(deadline: deadline, isPast: isPast, compact: ReadingMeasure.isColumn(of: detailWidth, narrowerThan: CourseDeadlineRow.compactBelow))
                if index < deadlines.count - 1 {
                    Divider().padding(.leading, PLLayout.rowPadding)
                }
            }
        }
        .accessibilityRotor(
            Text(l10n("course.tabs.deadlines")),
            entries: deadlines,
            entryID: \.event.id,
            entryLabel: \.event.title
        )
    }
}

/// "Today   11:59 PM   Problem Set 2 — q. 1–3   Assignment   ↗": day (120 pt), time (mono,
/// right-aligned), title (Semibold when due today), kind (secondary), Open in Browser.
struct CourseDeadlineRow: View {
    /// Below this list width the day and time move under the title (the 760 pt minimum window
    /// with the inspector open leaves a ~250 pt reading column).
    static let compactBelow: CGFloat = 440
    /// The time column's minimum, so "11:59 PM" and "9:00 AM" right-align.
    private static let timeColumn: CGFloat = 64

    let deadline: Deadline
    let isPast: Bool
    var compact = false

    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n
    @Environment(\.openURL) private var openURL

    var body: some View {
        let when = CourseDeadlines.when(deadline)
        let now = model.clock()
        let dueToday = when.map { CourseDates.isToday($0, now: now, calendar: model.calendar) } ?? false
        let link = CourseLink(deadline.event.url)
        let day = when.map { CourseDates.day($0, now: now, calendar: model.calendar, l10n: l10n) } ?? ""
        let time = when.map { CourseDates.time($0, calendar: model.calendar, l10n: l10n) } ?? ""
        let kind = l10n.eventKind(deadline.event.kind)
        let title = Text(deadline.event.title)
            .font(PLType.body.font.weight(dueToday && !isPast ? .semibold : .regular))
        HStack(alignment: .firstTextBaseline, spacing: PLSpace.s3) {
            if compact {
                VStack(alignment: .leading, spacing: 2) {
                    title
                        .fixedSize(horizontal: false, vertical: true)
                    Text([day, time, kind].filter { !$0.isEmpty }.joined(separator: " · "))
                        .font(PLType.callout.font)
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            } else {
                Text(day)
                    .font(PLType.body.font)
                    .monospacedDigit()
                    .frame(width: PLLayout.deadlineDayColumn, alignment: .leading)
                Text(time)
                    .font(PLType.body.font)
                    .monospacedDigit()
                    .frame(minWidth: Self.timeColumn, alignment: .trailing)
                title
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
                Text(kind)
                    .font(PLType.callout.font)
                    .foregroundStyle(.secondary)
                    .fixedSize()
            }
            Button {
                if let link { Links.open(link, openURL: openURL) }
            } label: {
                Label(l10n("mac.actions.openInBrowser"), systemImage: "arrow.up.forward.app")
                    .labelStyle(.iconOnly)
            }
            .quietButtonStyle()
            .help(l10n("mac.actions.openInBrowser"))
            .opacity(link == nil ? 0 : 1)
            .disabled(link == nil)
            .accessibilityHidden(link == nil)
        }
        .foregroundStyle(isPast ? AnyShapeStyle(.secondary) : AnyShapeStyle(.primary))
        .padding(PLLayout.rowPadding)
        .rowBorder()
        .contentShape(.rect)
        .contextMenu {
            if let link {
                Button(l10n("mac.actions.openInBrowser")) { Links.open(link, openURL: openURL) }
            }
            Button(l10n("mac.actions.copyTitle")) { Pasteboard.copy(deadline.event.title, announce: l10n("common.actions.copied")) }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(spoken(when))
        .accessibilityAction(named: Text(l10n("mac.actions.openInBrowser"))) {
            if let link { Links.open(link, openURL: openURL) }
        }
    }

    /// "Problem Set 2. DEMO205, Assignment. Due Friday, September 25, 11:59 PM."
    private func spoken(_ when: Date?) -> String {
        l10n("mac.a11y.deadlineRow", [
            "title": deadline.event.title,
            "course": deadline.courseCode ?? deadline.courseName ?? "",
            "kind": l10n.eventKind(deadline.event.kind),
            "when": when.map { CourseDates.spoken($0, calendar: model.calendar, l10n: l10n) } ?? "",
        ])
    }
}
