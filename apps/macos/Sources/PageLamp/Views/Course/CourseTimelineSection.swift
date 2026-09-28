// The course page's Timeline section (spec §3.2 W3b; the term strip is M2): where the course is
// now, how sure PageLamp is (in words), the evidence verbatim (English, tagged), and
// "Wrong week? Set Term Dates…" (tinted only via the arbiter).

import SwiftUI
import PageLampKit
import PageLampModel

struct CourseTimelineSection: View {
    let timeline: CourseTimeline
    let detail: CourseDetailModel

    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n

    var body: some View {
        VStack(alignment: .leading, spacing: PLLayout.sectionGapCourse) {
            VStack(alignment: .leading, spacing: PLSpace.s3) {
                SectionHeader(title: l10n("course.timeline.title"))
                InlineText.joined(weekParts)
                    .font(PLType.title3.font.weight(.regular))
                    .monospacedDigit()
                Text(l10n(CourseCodes.confidenceSentenceKey(timeline.confidence)))
                    .font(PLType.body.font)
                    .foregroundStyle(.secondary)
                    .paragraphLineSpacing()
                    .fixedSize(horizontal: false, vertical: true)
                if timeline.outsideTerm {
                    CalloutNote(symbol: "calendar", text: Text(l10n("course.timeline.outsideTerm"))) { EmptyView() }
                        .padding(.top, PLSpace.s1)
                }
            }
            VStack(alignment: .leading, spacing: PLSpace.s2) {
                Text(l10n("course.timeline.evidenceTitle"))
                    .font(PLType.headline.font)
                    .accessibilityAddTraits(.isHeader)
                if evidence.isEmpty {
                    Text(l10n("course.timeline.evidenceEmpty"))
                        .font(PLType.body.font)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                } else {
                    VStack(alignment: .leading, spacing: PLSpace.s1) {
                        ForEach(evidence, id: \.self) { reason in
                            HStack(alignment: .firstTextBaseline, spacing: PLSpace.s2) {
                                Text(verbatim: "•")
                                    .foregroundStyle(.tertiary)
                                    .accessibilityHidden(true)
                                Text.english(reason)
                                    .font(PLType.body.font)
                                    .foregroundStyle(.secondary)
                                    .textSelection(.enabled)
                                    .fixedSize(horizontal: false, vertical: true)
                            }
                        }
                    }
                }
            }
            HStack(alignment: .firstTextBaseline, spacing: PLSpace.s3) {
                Text(l10n("mac.course.timeline.wrongWeek"))
                    .font(PLType.body.font)
                Button(l10n("mac.actions.setTermDates")) { detail.showInspector(.termDates, in: model) }
                    .arbitratedButtonStyle(.setTermDates)
            }
        }
    }

    /// "Week 4 · high confidence", or "Week unknown" / "Outside term".
    private var weekParts: [Text] {
        let week = CourseWeekLine(section: .timeline, selectedWeek: nil, currentWeek: timeline.currentWeek, outsideTerm: timeline.outsideTerm)
        var parts = [Text(week.title(l10n)).fontWeight(.semibold)]
        if timeline.currentWeek != nil {
            parts.append(Text(verbatim: " · "))
            parts.append(Text(l10n(CourseCodes.confidenceKey(timeline.confidence))).foregroundStyle(.secondary))
        }
        if timeline.outsideTerm, timeline.currentWeek != nil {
            parts.append(Text(verbatim: " · "))
            parts.append(Text(l10n("common.week.outsideTerm")).foregroundStyle(.secondary))
        }
        return parts
    }

    /// The core's reasons exactly as given, de-duplicated (like Tauri).
    private var evidence: [String] {
        var seen = Set<String>()
        return timeline.evidence.filter { seen.insert($0).inserted }
    }
}
