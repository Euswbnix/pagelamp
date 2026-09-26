// The course detail page in its states (spec §3.2, §3.9: S7, S9, S11, S12, S14) for the snapshot
// catalogue, beside an open inspector at the default window size and at the minimum window; plus
// the read-only inspector's contents. States the demo data lacks come from a `FixtureService`.

import SwiftUI
import PageLamp
import PageLampKit
import PageLampModel

enum CourseSnapshots {
    /// The detail column beside an open inspector at the main window's default size.
    static let pageWidth = WindowMetrics.mainWidth - WindowMetrics.sidebarIdeal - WindowMetrics.inspectorIdeal
    /// The detail column of the 760 pt minimum window with the sidebar and the inspector open.
    static let narrowWidth = WindowMetrics.mainMinWidth - WindowMetrics.sidebarMin - WindowMetrics.inspectorMin

    /// One state of the page.
    struct Case {
        var name: String
        var code: String
        var section: CourseSection = .week
        var selectedWeek: UInt32?
        var setup = SnapshotSetup()
        /// Also render the read-only inspector.
        var inspector = false
        var width = pageWidth
    }

    static let cases: [Case] = [
        // The current week, lit: every material status, modules, announcements.
        Case(name: "DEMO101-week", code: "DEMO101", inspector: true),
        // Stepped back two weeks: the lamp is out, "2 weeks ago".
        Case(name: "DEMO101-week2", code: "DEMO101", selectedWeek: 2),
        Case(name: "DEMO101-deadlines", code: "DEMO101", section: .deadlines),
        Case(name: "DEMO101-timeline", code: "DEMO101", section: .timeline),
        // S7: the Canvas token expired (the header's Replace Token… is bordered: the capsule's fix
        // bubble holds the tint while it shows); policy not set; blocked file.
        Case(name: "DEMO205-week-expired", code: "DEMO205", setup: SnapshotSetup(scenario: .expired), inspector: true),
        // S9 + S11: No AI, week unknown (Set Term Dates… wins the arbiter).
        Case(name: "DEMO310-week-noai", code: "DEMO310", inspector: true),
        Case(name: "DEMO310-timeline", code: "DEMO310", section: .timeline),
        // The student turned AI access off.
        Case(name: "DEMO101-week-off", code: "DEMO101", setup: fixture { FixtureService(base: $0, aiAccessOff: true) }, inspector: true),
        // S11: outside the term dates.
        Case(name: "DEMO205-week-outside", code: "DEMO205", setup: fixture { FixtureService(base: $0, outsideTerm: true) }),
        // Hidden past course with no deadlines (S12 empties).
        Case(name: "DEMO099-deadlines-empty", code: "DEMO099", section: .deadlines, inspector: true),
        // S12: a week without materials.
        Case(name: "DEMO205-week-empty", code: "DEMO205", selectedWeek: 1, setup: fixture { FixtureService(base: $0, emptyWeek: true) }),
        // S14: sections that couldn't load; a folder source that failed.
        Case(
            name: "DEMO101-week-errors", code: "DEMO101",
            setup: fixture(scenario: .error) { FixtureService(base: $0, failing: [.week, .overview]) }
        ),
        Case(
            name: "DEMO101-deadlines-error", code: "DEMO101", section: .deadlines,
            setup: fixture { FixtureService(base: $0, failing: [.courseDeadlines]) }
        ),
        // The minimum window with the inspector open: rows switch to their compact layout.
        Case(name: "DEMO101-week-narrow", code: "DEMO101", width: narrowWidth),
        Case(name: "DEMO101-deadlines-narrow", code: "DEMO101", section: .deadlines, width: narrowWidth),
        Case(name: "DEMO099-timeline-narrow", code: "DEMO099", section: .timeline, width: narrowWidth),
    ]

    /// The states as `course-<case>` pages, and `course-<case>-inspector` where asked.
    static var pages: [SnapshotPage] {
        cases.flatMap { item in
            var pages = [
                SnapshotPage(name: "course-\(item.name)", width: item.width, setup: item.setup) { model in
                    guard let (summary, detail) = await load(item, model) else { return AnyView(EmptyView()) }
                    return AnyView(CourseDetailPage(summary: summary, detail: detail))
                },
            ]
            if item.inspector {
                pages.append(SnapshotPage(name: "course-\(item.name)-inspector", width: WindowMetrics.inspectorIdeal, setup: item.setup) { model in
                    guard let (summary, _) = await load(item, model) else { return AnyView(EmptyView()) }
                    // ImageRenderer can't draw a Form: the same sections in a plain stack.
                    return AnyView(CourseInspectorForm(summary: summary, layout: .stack))
                })
            }
            return pages
        }
    }

    /// A mock scenario with some answers replaced.
    private static func fixture(
        scenario: MockScenario = .demo, _ wrap: @escaping @Sendable (MockService) -> FixtureService
    ) -> SnapshotSetup {
        SnapshotSetup(scenario: scenario, service: { mock, _, _ in wrap(mock) })
    }

    /// Opens the case's course like the page does: learn its weeks, step like the toolbar, then
    /// load every part.
    private static func load(_ item: Case, _ model: AppModel) async -> (CourseSummary, CourseDetailModel)? {
        guard let summary = model.courses.first(where: { $0.course.code == item.code }) else { return nil }
        let ui = model.ui(for: summary.course.id)
        _ = try? await model.weekMaterials(for: summary.course.id)
        ui.section = item.section
        ui.selectedWeek = item.selectedWeek
        let detail = CourseDetailModel(courseId: summary.course.id)
        await detail.loadAll(using: model)
        return (summary, detail)
    }
}
