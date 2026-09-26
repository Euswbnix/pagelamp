// Your courses — the Contents page (spec §6.4, M1: no dotted leaders, no rolling numerals). Each
// course reads like a line in a book's contents: code (Subheadline Semibold), name (Body, one
// line, original language), and its week right-aligned (Title 3, monospaced digits); below, the
// next deadline and the AI policy with what the AI app may read. Policies are never
// colour-coded. Rows are plain buttons into the course, one VoiceOver element each.

import SwiftUI
import PageLampKit
import PageLampModel

struct ContentsSection: View {
    let text: ThisWeekText

    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n

    var body: some View {
        VStack(alignment: .leading, spacing: PLLayout.titleToRule) {
            SectionHeader(title: l10n("courses.list.title"))
            let courses = ThisWeekContents.courses(model.courses)
            if model.sectionErrors[.courses] != nil {
                SectionError(title: l10n("courses.list.errorTitle")) { Task { await model.refresh() } }
            } else if ThisWeekContents.allHidden(model.courses) {
                Text(l10n("mac.thisWeek.allHidden"))
                    .foregroundStyle(.secondary)
            } else {
                let codes = courses.map(ThisWeekContents.label)
                VStack(alignment: .leading, spacing: PLSpace.s1) {
                    ForEach(courses, id: \.course.id) { summary in
                        ContentsRow(summary: summary, codes: codes, text: text)
                    }
                }
            }
        }
    }
}

private struct ContentsRow: View {
    let summary: CourseSummary
    let codes: [String]
    let text: ThisWeekText

    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n
    @Environment(\.openURL) private var openURL
    @Environment(\.detailColumnWidth) private var detailWidth

    /// Below this reading-column width the next deadline and the policy take a line each.
    static let stackBelow: CGFloat = 600

    private var course: Course { summary.course }
    private var past: Bool { !course.enrollmentActive }

    var body: some View {
        let week = CourseWeekState(summary.timeline)
        Button {
            ThisWeekNavigation.open(courseId: course.id, model: model)
        } label: {
            HStack(alignment: .firstTextBaseline, spacing: PLSpace.s3) {
                ThisWeekColumnCell(samples: codes) {
                    Text(ThisWeekContents.label(summary))
                }
                .font(PLType.subheadline.font.weight(.semibold))
                VStack(alignment: .leading, spacing: PLSpace.s1) {
                    HStack(alignment: .firstTextBaseline, spacing: PLSpace.s3) {
                        Text(course.name)
                            .font(PLType.body.font)
                            .lineLimit(1)
                            .help(course.name)
                        Spacer(minLength: PLSpace.s3)
                        Text(text.week(week))
                            .font(PLType.title3.font.weight(.regular))
                            .monospacedDigit()
                            .fixedSize()
                    }
                    details(week: week)
                }
            }
            .foregroundStyle(past ? AnyShapeStyle(.secondary) : AnyShapeStyle(.primary))
        }
        .buttonStyle(RowButtonStyle())
        .accessibilityLabel(text.spokenContentsRow(summary))
        .accessibilityValue(past ? l10n("common.pastCourse.badge") : "")
        .contextMenu {
            Button(l10n("mac.actions.openCourse")) {
                ThisWeekNavigation.open(courseId: course.id, model: model)
            }
            // Only a web address opens as the course website (never a file or script URL).
            if let url = Links.web(course.url) {
                Button(l10n("mac.actions.openCourseWebsite")) { openURL(url) }
            }
        }
    }

    /// "Next: Quiz 3 · Sat 9:00 AM   ▪ Learning aid only · 12 of 14 readable", wrapping onto two
    /// lines when narrow; then the term hint (S11) and the past-course note.
    private func details(week: CourseWeekState) -> some View {
        let next = summary.nextDeadline.flatMap(text.next)
        // One text that wraps where it must, on two lines in a narrow column; the glyph stays
        // with its words (no-break space).
        let stacked = ReadingMeasure.isColumn(of: detailWidth, narrowerThan: Self.stackBelow)
        var parts: [Text] = []
        if let next { parts += [Text(next), Text(verbatim: stacked ? "\n" : "   ")] }
        parts += [
            InlineText.glyph(summary.course.aiPolicy.symbol),
            Text(verbatim: "\u{00A0}"),
            Text(text.policyLine(summary)),
        ]
        return Group {
            InlineText.joined(parts)
                .monospacedDigit()
                .fixedSize(horizontal: false, vertical: true)
            switch week {
            case .unknown:
                Text(l10n("courses.card.setTerm"))
                    .fixedSize(horizontal: false, vertical: true)
            case .outsideTerm:
                Text(text.spokenWeek(week))
            case .week:
                EmptyView()
            }
            if past {
                Label(l10n("common.pastCourse.badge"), systemImage: "clock.arrow.circlepath")
            }
        }
        .font(PLType.callout.font)
        .foregroundStyle(.secondary)
    }
}
