// Next 7 days (spec §3.1): every deadline and class from today to six days ahead, by day. Rolling,
// so titled so. Day column 120 pt (relative day, then date) · time (monospaced digits, right-
// aligned) · title (Semibold if due today) · course code · kind. Classes are quieter; undated
// events are left out. Rows are plain buttons into the course's Deadlines, with a Deadlines rotor.

import SwiftUI
import PageLampKit
import PageLampModel

struct Next7DaysSection: View {
    let digest: ThisWeekDigest
    let text: ThisWeekText

    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n
    @Namespace private var rotor

    var body: some View {
        VStack(alignment: .leading, spacing: PLLayout.titleToRule) {
            SectionHeader(title: l10n("mac.courses.thisWeek.next7"), detail: text.next7Detail(digest))
            if let failure = model.sectionErrors[.deadlines] {
                SectionError(title: l10n("courses.thisWeek.errorTitle"), message: failure.localizedDescription(in: l10n)) {
                    Task { await model.refresh() }
                }
            } else if digest.groups.isEmpty {
                QuietState(
                    symbol: "calendar",
                    title: l10n("courses.thisWeek.nothingDue"),
                    message: l10n("courses.thisWeek.emptyHint")
                )
            } else {
                days
            }
        }
    }

    private var days: some View {
        let rows = digest.groups.flatMap { $0.deadlines + $0.classes }
        let columns = Columns(
            times: rows.compactMap { ThisWeekDigest.time(of: $0).map(text.time) },
            courses: rows.map { text.course(of: $0) ?? "" },
            kinds: rows.map { text.l10n.eventKind($0.event.kind) }
        )
        return VStack(alignment: .leading, spacing: PLLayout.dayGroupGap) {
            ForEach(digest.groups) { group in
                DayGroup(group: group, text: text, columns: columns, rotor: rotor)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityRotor(Text(l10n("mac.thisWeek.a11y.deadlinesRotor"))) {
            ForEach(rows, id: \.event.id) { deadline in
                AccessibilityRotorEntry(Text(deadline.event.title), id: deadline.event.id, in: rotor)
            }
        }
    }

    /// Every row's column texts, so the time, course and kind columns line up across days.
    struct Columns {
        var times: [String]
        var courses: [String]
        var kinds: [String]
    }
}

/// One day: its heading on the left, its deadlines, then its classes.
private struct DayGroup: View {
    let group: ThisWeekDigest.DayGroup
    let text: ThisWeekText
    let columns: Next7DaysSection.Columns
    let rotor: Namespace.ID

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: PLSpace.s4) {
            VStack(alignment: .leading, spacing: 2) {
                Text(text.dayTitle(group.day))
                    .font(PLType.body.font.weight(.semibold))
                Text(text.dayDate(group.day))
                    .font(PLType.callout.font)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
            }
            .frame(width: PLLayout.deadlineDayColumn, alignment: .leading)
            .accessibilityElement(children: .combine)
            .accessibilityAddTraits(.isHeader)

            VStack(alignment: .leading, spacing: 0) {
                ForEach(group.deadlines, id: \.event.id) { deadline in
                    DeadlineRow(deadline: deadline, isToday: group.dayOffset == 0, text: text, columns: columns)
                        .accessibilityRotorEntry(id: deadline.event.id, in: rotor)
                }
                ForEach(group.classes, id: \.event.id) { deadline in
                    DeadlineRow(deadline: deadline, isToday: group.dayOffset == 0, text: text, columns: columns)
                        .accessibilityRotorEntry(id: deadline.event.id, in: rotor)
                }
            }
        }
    }
}

/// A deadline or class. A plain button into the course's Deadlines; an event without a known
/// course opens its link, and one with neither is plain text.
private struct DeadlineRow: View {
    let deadline: Deadline
    let isToday: Bool
    let text: ThisWeekText
    let columns: Next7DaysSection.Columns

    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n
    @Environment(\.openURL) private var openURL

    private var isClass: Bool { deadline.event.kind == .classEvent }

    var body: some View {
        let course = ThisWeekNavigation.course(of: deadline, in: model)
        let link = deadline.event.url
        let spoken = text.spokenDeadline(deadline)
        Group {
            // A button is one VoiceOver element already: only its label is replaced.
            if let course {
                Button {
                    ThisWeekNavigation.open(courseId: course.course.id, section: .deadlines, model: model)
                } label: {
                    label
                }
                .buttonStyle(RowButtonStyle())
                .accessibilityLabel(spoken)
            } else if Links.web(link) != nil {
                Button {
                    Links.openWeb(link, openURL: openURL)
                } label: {
                    label
                }
                .buttonStyle(RowButtonStyle())
                .accessibilityLabel(spoken)
            } else {
                label
                    .padding(.vertical, PLSpace.s1 + 2)
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel(spoken)
            }
        }
        .contextMenu {
            if Links.web(link) != nil {
                Button(l10n("mac.actions.openInBrowser")) { Links.openWeb(link, openURL: openURL) }
            }
            if let course {
                Button(l10n("mac.actions.goToCourse")) {
                    ThisWeekNavigation.open(courseId: course.course.id, section: .deadlines, model: model)
                }
            }
            Button(l10n("mac.actions.copyTitle")) {
                Pasteboard.copy(deadline.event.title, announce: l10n("common.actions.copied"))
            }
        }
    }

    private var label: some View {
        HStack(alignment: .firstTextBaseline, spacing: PLSpace.s3) {
            ThisWeekColumnCell(samples: columns.times, alignment: .trailing) {
                Text(ThisWeekDigest.time(of: deadline).map(text.time) ?? "")
            }
            .monospacedDigit()
            Text(deadline.event.title)
                .fontWeight(isToday && !isClass ? .semibold : .regular)
                .multilineTextAlignment(.leading)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
            ThisWeekColumnCell(samples: columns.courses) {
                Text(text.course(of: deadline) ?? "")
            }
            ThisWeekColumnCell(samples: columns.kinds) {
                Text(text.l10n.eventKind(deadline.event.kind))
                    .foregroundStyle(.secondary)
            }
        }
        .font(PLType.body.font)
        // Classes are context, not to-dos: the whole row is quieter.
        .foregroundStyle(isClass ? AnyShapeStyle(.secondary) : AnyShapeStyle(.primary))
    }
}
