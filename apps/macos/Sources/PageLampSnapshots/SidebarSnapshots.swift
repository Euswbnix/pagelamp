// The sidebar (spec §2.3) for the snapshot catalogue: rows and headers where the system sidebar
// puts them, at each Sidebar icon size, the narrowest column, a selected course, and without
// courses. The glass capsule draws as a flat stand-in (glass and the NSView behind it don't render
// offscreen); the window's sidebar glass isn't drawn either.

import SwiftUI
import PageLamp
import PageLampKit
import PageLampModel

enum SidebarSnapshots {
    static let pages: [SnapshotPage] = [
        // Expired Canvas token: Sources & Sync counts one failing source, its courses show the
        // warning glyph.
        page("sidebar", size: .medium) { _ in .thisWeek },
        page("sidebar-course", size: .medium) { model in
            model.visibleCourses.first { $0.course.code == "DEMO205" }.map { .course($0.course.id) } ?? .thisWeek
        },
        page("sidebar-small", size: .small) { _ in .thisWeek },
        // The large glyphs against the title at x 48.
        page("sidebar-large", size: .large) { _ in .thisWeek },
        // 200 pt fits "数据来源与同步" and its count, "DEMO205 ⚠︎ 第 4 周" on one line (spec §7.3).
        page("sidebar-narrow", width: WindowMetrics.sidebarMin, size: .medium) { _ in .sources },
        page("sidebar-no-courses", scenario: .empty, size: .medium) { _ in .thisWeek },
    ]

    private static func page(
        _ name: String, width: CGFloat = WindowMetrics.sidebarIdeal, scenario: MockScenario = .expired,
        size: SidebarRowSize, destination: @escaping @MainActor (AppModel) -> Destination
    ) -> SnapshotPage {
        SnapshotPage(name: name, width: width, setup: SnapshotSetup(scenario: scenario)) { model in
            model.destination = destination(model)
            return AnyView(SidebarSnapshot().environment(\.sidebarRowSize, size))
        }
    }
}
