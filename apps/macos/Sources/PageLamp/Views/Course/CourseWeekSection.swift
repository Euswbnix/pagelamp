// The course page's This Week section (spec §3.2 "This Week section", S9, S11, S12, S14):
// the week's note, modules and materials, then recent announcements. Download Files… is M2.

import SwiftUI
import PageLampKit
import PageLampModel

struct CourseWeekSection: View {
    let summary: CourseSummary
    let detail: CourseDetailModel

    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n

    var body: some View {
        VStack(alignment: .leading, spacing: PLLayout.sectionGapCourse) {
            if summary.aiMaterials == .withheldByPolicy {
                // S9: honest about what No AI means, in full sentences (rule 8).
                Callout(
                    tone: .info,
                    symbol: "hand.raised",
                    title: l10n("common.aiMaterials.withheld_by_policy"),
                    message: l10n("mac.course.noAI.body")
                ) {
                    Button(l10n("mac.actions.aiPolicy")) { detail.showInspector(.aiPolicy, in: model) }
                        .buttonStyle(.bordered)
                }
            }
            weekContent
            CourseAnnouncements(detail: detail)
        }
    }

    @ViewBuilder private var weekContent: some View {
        switch detail.week {
        case .loading:
            CoursePlaceholderRows(rows: 4)
        case .failed(let failure):
            SectionError(title: l10n("course.week.loadError"), message: failure.localizedDescription(in: l10n)) {
                Task { await detail.loadWeek(using: model) }
            }
        case .loaded(let week):
            CourseWeekMaterials(week: week, detail: detail)
                .opacity(detail.isLoadingWeek ? 0.6 : 1)
                .animation(PLMotion.reduced, value: detail.isLoadingWeek)
        }
    }
}

/// One loaded week: its note (S11), modules line, and materials (or the quiet empty, S12).
struct CourseWeekMaterials: View {
    let week: WeekMaterials
    let detail: CourseDetailModel

    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n

    var body: some View {
        VStack(alignment: .leading, spacing: PLSpace.s5) {
            if let note = CourseDetailModel.WeekNote(week) {
                noteView(note)
            }
            if !week.modules.isEmpty {
                modulesLine
            }
            VStack(alignment: .leading, spacing: PLSpace.s2) {
                SectionHeader(
                    title: week.week == nil ? l10n("course.week.recent") : l10n("course.week.materials"),
                    detail: materialsDetail
                )
                if week.materials.isEmpty {
                    QuietState(
                        symbol: "tray",
                        title: week.week.map { l10n("course.week.empty.title", ["week": l10n.number($0)]) }
                            ?? l10n("course.week.empty.titleRecent"),
                        message: l10n("course.week.empty.description")
                    ) {
                        Button(l10n("mac.actions.openSourcesAndSync")) { model.destination = .sources }
                            .buttonStyle(.bordered)
                    }
                    .padding(.top, PLSpace.s2)
                } else {
                    CourseMaterialList(materials: week.materials, aiMaterials: week.aiMaterials)
                }
            }
        }
    }

    @ViewBuilder private func noteView(_ note: CourseDetailModel.WeekNote) -> some View {
        switch note {
        case .known(let key, let offersTermDates):
            CalloutNote(symbol: "info.circle", text: Text(l10n(key))) {
                if offersTermDates {
                    Button(l10n("mac.actions.setTermDates")) { detail.showInspector(.termDates, in: model) }
                        .arbitratedButtonStyle(.setTermDates)
                }
            }
        case .backend(let text):
            CalloutNote(symbol: "info.circle", text: Text.english(text)) { EmptyView() }
        }
    }

    /// "▪ Modules  ▪ Week 4: Sampling  ▪ Lab 3" (Callout, secondary).
    private var modulesLine: some View {
        var parts: [Text] = [Text(l10n("course.week.modules")).fontWeight(.semibold)]
        for module in week.modules {
            parts.append(Text(verbatim: "   "))
            parts.append(InlineText.glyph("square.stack.3d.up"))
            parts.append(Text(verbatim: " "))
            parts.append(Text(module.name))
        }
        return InlineText.joined(parts)
            .font(PLType.callout.font)
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(
                ([l10n("course.week.modules")] + week.modules.map(\.name)).joined(separator: ", ")
            )
    }

    /// "12 of 14 readable" for the week, or why the AI app reads none of it.
    private var materialsDetail: String? {
        switch week.aiMaterials {
        case .readable:
            guard !week.materials.isEmpty else { return nil }
            let readable = week.materials.filter { $0.textStatus == .ok }.count
            return l10n("mac.course.readableShort", [
                "indexed": l10n.number(readable),
                "count": l10n.number(week.materials.count),
            ])
        case .turnedOff:
            return l10n("common.aiMaterials.turned_off")
        case .withheldByPolicy:
            // The No AI callout above already says so.
            return nil
        }
    }
}

/// Recent announcements (titles and dates, newest first) from `course_overview`.
struct CourseAnnouncements: View {
    let detail: CourseDetailModel

    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n
    @Environment(\.openURL) private var openURL

    var body: some View {
        VStack(alignment: .leading, spacing: PLSpace.s2) {
            SectionHeader(title: l10n("course.week.announcements.title"))
            switch detail.overview {
            case .loading:
                CoursePlaceholderRows(rows: 2)
            case .failed(let failure):
                SectionError(title: l10n("course.loadError"), message: failure.localizedDescription(in: l10n)) {
                    Task { await detail.loadOverview(using: model) }
                }
            case .loaded(let overview):
                if overview.recentAnnouncements.isEmpty {
                    Text(l10n("course.week.announcements.empty"))
                        .font(PLType.callout.font)
                        .foregroundStyle(.secondary)
                        .padding(.top, PLSpace.s1)
                } else {
                    VStack(alignment: .leading, spacing: 0) {
                        ForEach(Array(overview.recentAnnouncements.enumerated()), id: \.element.id) { index, item in
                            row(item)
                            if index < overview.recentAnnouncements.count - 1 {
                                Divider().padding(.leading, PLLayout.rowPadding)
                            }
                        }
                    }
                }
            }
        }
    }

    private func row(_ item: MaterialView) -> some View {
        let link = CourseLink(item.url)
        let date = item.publishedAt.map {
            CourseDates.published($0, now: model.clock(), calendar: model.calendar, l10n: l10n)
        }
        return HStack(alignment: .firstTextBaseline, spacing: PLSpace.s3) {
            Image(systemName: "megaphone")
                .symbolRenderingMode(.hierarchical)
                .foregroundStyle(.secondary)
                .frame(width: PLSpace.s5)
                .accessibilityHidden(true)
            Text(item.title)
                .font(PLType.body.font)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
            if let date {
                Text(date)
                    .font(PLType.callout.font)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
                    .fixedSize()
            }
            if case .web = link {
                Button {
                    if let link { Links.open(link, openURL: openURL) }
                } label: {
                    Label(l10n("mac.actions.openInBrowser"), systemImage: "arrow.up.forward.app")
                        .labelStyle(.iconOnly)
                }
                .quietButtonStyle()
                .help(l10n("mac.actions.openInBrowser"))
            }
        }
        .padding(PLLayout.rowPadding)
        .rowBorder()
        .accessibilityElement(children: .combine)
        .contextMenu {
            if case .web(let url) = link {
                Button(l10n("mac.actions.openInBrowser")) { openURL(url) }
                Button(l10n("mac.actions.copyLink")) { Pasteboard.copy(url.absoluteString, announce: l10n("common.actions.copied")) }
            }
            Button(l10n("mac.actions.copyTitle")) { Pasteboard.copy(item.title, announce: l10n("common.actions.copied")) }
        }
    }
}
