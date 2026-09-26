// The sidebar (spec §2.3, M1): This Week, visible courses, Setup, and the footer status.
// The system draws its glass; nothing here overrides the 27 accent icons or bold selection.

import SwiftUI
import PageLampKit
import PageLampModel

struct SidebarList: View {
    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n

    var body: some View {
        List(selection: selection) {
            Label(l10n("mac.nav.thisWeek"), systemImage: "lamp.desk")
                .tag(Destination.thisWeek)

            let courses = model.visibleCourses
            if !courses.isEmpty {
                Section(l10n("common.nav.courses")) {
                    ForEach(courses, id: \.course.id) { summary in
                        SidebarCourseRow(summary: summary)
                            .tag(Destination.course(summary.course.id))
                    }
                }
            }

            Section(l10n("mac.sidebar.setup")) {
                // The badge counts failing sources only (a count, so `.badge`; zero hides it).
                Label(l10n("mac.nav.sources"), systemImage: "folder.badge.gearshape")
                    .badge(model.failingSources.count)
                    .tag(Destination.sources)
                Label {
                    HStack(spacing: PLSpace.s1) {
                        Text(l10n("mac.nav.connect"))
                        if model.temporaryLocation != nil {
                            Spacer(minLength: PLSpace.s1)
                            Image(systemName: "exclamationmark.triangle")
                                .foregroundStyle(PLColor.warning)
                                .accessibilityHidden(true)
                        }
                    }
                } icon: {
                    Image(systemName: "cable.connector")
                }
                // The warning glyph is hidden from VoiceOver; the value says it in words (S15).
                .accessibilityValue(model.temporaryLocation.map { ConnectSetup.temporaryLocationText($0, l10n: l10n).title } ?? "")
                .tag(Destination.connect)
            }
        }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            SidebarFooterStatus()
        }
    }

    private var selection: Binding<Destination?> {
        Binding(
            get: { model.destination },
            set: { if let destination = $0 { model.destination = destination } }
        )
    }
}

/// A course: code (or name), then a warning glyph if its source fails, then its week.
struct SidebarCourseRow: View {
    let summary: CourseSummary
    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n

    private var week: UInt32? {
        summary.timeline.outsideTerm ? nil : summary.timeline.currentWeek
    }

    var body: some View {
        let course = summary.course
        let failing = model.isSourceFailing(course.sourceId)
        HStack(spacing: PLSpace.s1) {
            Label {
                Text(course.code ?? course.name)
                    .lineLimit(1)
            } icon: {
                // No AI is shown neutrally, never as an error (spec §1.2).
                Image(systemName: course.aiPolicy == .prohibited ? "hand.raised" : "book.closed")
            }
            Spacer(minLength: PLSpace.s1)
            // Not `.badge`, which means a count.
            HStack(spacing: PLSpace.s1) {
                if failing {
                    Image(systemName: "exclamationmark.triangle")
                        .foregroundStyle(PLColor.warning)
                }
                Text(l10n.compactWeek(week))
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
            }
            .font(PLType.callout.font)
        }
        .help(course.name)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(accessibilityLabel(failing: failing))
    }

    private func accessibilityLabel(failing: Bool) -> String {
        let course = summary.course
        let arguments = ["code": course.code ?? course.name, "name": course.name, "week": l10n.week(week)]
        let noAI = course.aiPolicy == .prohibited
        return switch (noAI, failing) {
        case (false, false): l10n("mac.a11y.sidebarCourse", arguments)
        case (true, false): l10n("mac.a11y.sidebarCourseNoAI", arguments)
        case (false, true): l10n("mac.a11y.sidebarCourseAttention", arguments)
        case (true, true): l10n("mac.a11y.sidebarCourseNoAIAttention", arguments)
        }
    }
}

/// The persistent, low-emphasis status (spec §2.3): plain text, Subheadline, never glass, never
/// a count. A click opens Sources & Sync.
struct SidebarFooterStatus: View {
    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n
    @Environment(\.accessibilityShowBorders) private var showBorders

    var body: some View {
        Button {
            model.destination = .sources
        } label: {
            TimelineView(.everyMinute) { _ in
                let status = model.footerStatus
                Label {
                    Text(text(for: status))
                } icon: {
                    glyph(for: status)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .font(PLType.subheadline.font)
        .foregroundStyle(.secondary)
        .padding(.horizontal, PLSpace.s2)
        .padding(.vertical, PLSpace.s1)
        // Show Borders (spec §7.2): the plain footer gets a 1 pt outline.
        .overlay {
            if showBorders {
                RoundedRectangle(cornerRadius: PLRadius.row).strokeBorder(.separator, lineWidth: 1)
            }
        }
        .padding(.horizontal, PLSpace.s2)
        .padding(.vertical, PLSpace.s2)
        .accessibilityHint(l10n("mac.actions.openSourcesAndSync"))
    }

    private func text(for status: FooterStatus) -> String {
        switch status {
        case .syncing: l10n("common.sync.syncing")
        case .needsAttention: l10n("common.sync.needsAttention")
        case .synced(let date): l10n("common.sync.syncedAgo", ["when": l10n.relative(date, to: model.clock())])
        case .neverSynced: l10n("common.sync.never")
        }
    }

    @ViewBuilder private func glyph(for status: FooterStatus) -> some View {
        switch status {
        case .syncing:
            Image(systemName: "arrow.triangle.2.circlepath")
        case .needsAttention:
            Image(systemName: "exclamationmark.triangle")
                .foregroundStyle(PLColor.warning)
        case .synced:
            Image(systemName: "checkmark.circle")
        case .neverSynced:
            Image(systemName: "circle.dashed")
        }
    }
}
