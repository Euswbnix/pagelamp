// The course page's header band (spec §3.2 "Header band"): eyebrow, course name, the AI status
// line, the section picker and the week line. It sits in the LampBand, lit only for *now*.

import SwiftUI
import PageLampKit
import PageLampModel

struct CourseHeader: View {
    let summary: CourseSummary
    let detail: CourseDetailModel
    let weekLine: CourseWeekLine
    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n

    var body: some View {
        let ui = model.ui(for: summary.course.id)
        PageHeader(eyebrow: eyebrow, title: summary.course.name, titleWeight: .semibold)
        CourseAIStatusLine(summary: summary) {
            detail.showInspector(.aiPolicy, in: model)
        }
        CourseSectionPicker(ui: ui)
            .padding(.top, PLSpace.s2)
        CourseWeekLineView(line: weekLine) { delta in
            ui.step(by: delta)
        }
    }

    /// "DEMO205 · Demo Canvas · synced 2 hr. ago" (+ "Syncing now…" while its source syncs).
    private var eyebrow: String {
        let course = summary.course
        let arguments = ["code": course.code ?? course.name, "source": summary.sourceLabel]
        if isSourceSyncing {
            return l10n("mac.course.eyebrowSyncing", arguments)
        }
        guard let synced = summary.lastSyncedAt else {
            return l10n("mac.course.eyebrowNeverSynced", arguments)
        }
        return l10n("mac.course.eyebrow", arguments.merging(["when": l10n.relative(synced, to: model.clock(), calendar: model.calendar)]) { $1 })
    }

    /// The course's own source is being synced right now (not just any source).
    private var isSourceSyncing: Bool {
        guard model.isSyncing, let progress = model.syncProgress else { return false }
        let sourceId = summary.course.sourceId
        return progress.sourceId == sourceId && progress.outcomes[sourceId] == nil
    }
}

/// "▪ Learning aid only · ▪ 12 of 14 materials readable by your AI app": one plain button that
/// opens the inspector at AI Policy. Neutral glyphs, ink text, never colour-coded (spec §1.2).
struct CourseAIStatusLine: View {
    let summary: CourseSummary
    var showPolicy: () -> Void
    @Environment(\.l10n) private var l10n

    var body: some View {
        Button(action: showPolicy) {
            InlineText.joined(parts)
                .font(PLType.body.font)
                .foregroundStyle(.primary)
                .multilineTextAlignment(.leading)
                .fixedSize(horizontal: false, vertical: true)
                .contentShape(.rect)
        }
        .quietButtonStyle(plain: true)
        .accessibilityLabel(Self.items(summary, l10n).map(\.text).joined(separator: ", "))
        .accessibilityHint(l10n("mac.course.a11y.aiStatusHint"))
        .help(l10n("mac.actions.aiPolicy"))
    }

    private var parts: [Text] {
        var parts: [Text] = []
        for (index, item) in Self.items(summary, l10n).enumerated() {
            if index > 0 { parts.append(Text(verbatim: " · ")) }
            if let symbol = item.symbol {
                parts.append(InlineText.glyph(symbol))
                parts.append(Text(verbatim: " "))
            }
            parts.append(Text(item.text))
        }
        return parts
    }

    /// The line's items in order: policy, AI access to materials, then Past course and Hidden.
    static func items(_ summary: CourseSummary, _ l10n: L10n) -> [(symbol: String?, text: String)] {
        let course = summary.course
        var items: [(symbol: String?, text: String)] = []
        if course.aiPolicy == .unknown {
            items.append((course.aiPolicy.symbol, l10n("mac.course.aiStatus.notSet")))
        } else {
            items.append((course.aiPolicy.symbol, l10n.policy(course.aiPolicy)))
        }
        switch summary.aiMaterials {
        case .readable:
            let total = Int(summary.counts.materials)
            items.append((
                "book.pages",
                total == 0
                    ? l10n("common.aiMaterials.readableNone")
                    : l10n.plural("common.aiMaterials.readable", count: total, ["indexed": l10n.number(summary.counts.indexedMaterials)])
            ))
        case .turnedOff:
            items.append(("hand.raised", l10n("common.aiMaterials.turned_off")))
        case .withheldByPolicy:
            // The policy's own glyph (hand.raised) already leads the line (S9).
            items.append((nil, l10n("common.aiMaterials.withheld_by_policy")))
        }
        if !course.enrollmentActive {
            items.append(("clock.arrow.circlepath", l10n("common.pastCourse.badge")))
        }
        if course.hidden {
            items.append(("eye.slash", l10n("course.header.hidden")))
        }
        return items
    }
}

/// "Week 4" + "This week": Title 1, monospaced digits, Semibold when lit. The one adjustable
/// VoiceOver element for stepping weeks (spec §6.3).
struct CourseWeekLineView: View {
    let line: CourseWeekLine
    var step: (Int) -> Void
    @Environment(\.l10n) private var l10n
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: PLSpace.s3) {
            Text(line.title(l10n))
                .font(PLType.title1.font.weight(line.lit ? .semibold : .regular))
                .monospacedDigit()
                .contentTransition(reduceMotion ? .identity : .numericText(value: Double(line.week ?? 0)))
            if let tag = line.tagText(l10n) {
                Text(tag)
                    .font(PLType.body.font)
                    .foregroundStyle(.secondary)
            }
        }
        .animation(reduceMotion ? nil : PLMotion.week, value: line)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(line.spoken(l10n))
        .modifier(WeekStepping(enabled: line.isAdjustable, step: step))
    }
}

/// The adjustable action, only where stepping weeks is what the page does (the This Week section).
private struct WeekStepping: ViewModifier {
    let enabled: Bool
    let step: (Int) -> Void

    func body(content: Content) -> some View {
        if enabled {
            content.accessibilityAdjustableAction { direction in
                switch direction {
                case .increment: step(1)
                case .decrement: step(-1)
                @unknown default: break
                }
            }
        } else {
            content
        }
    }
}
