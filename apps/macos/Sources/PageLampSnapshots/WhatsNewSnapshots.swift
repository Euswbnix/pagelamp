// What's new (the main window's sheet after an update) for the snapshot catalogue: two topics,
// and a list long enough to scroll inside the sheet. The sheet renders on its own at its width
// (no window behind it offscreen).
//
// Layout samples: on main the Mac app shows one topic (course weeks); the update-check topic is
// the Tauri app's and never reaches this sheet. Its strings stand in for the topics that arrive
// with their features (course removal, syllabus reading, …) until those rows exist here.

import SwiftUI
import PageLamp
import PageLampKit
import PageLampModel

enum WhatsNewSnapshots {
    static let pages: [SnapshotPage] = [
        page("whats-new-two-topics", since: "0.3.0-alpha.1", count: 2),
        page("whats-new-long-list", since: nil, count: 6),
    ]

    private static let courseWeeks = WhatsNewItem(
        topic: .courseWeeks,
        titleKey: "updates.whatsNew.topics.course_weeks.title",
        bodyKey: "updates.whatsNew.topics.course_weeks.body",
        symbol: "calendar"
    )
    /// A stand-in with a longer paragraph (see the file's comment).
    private static let longer = WhatsNewItem(
        topic: .updateCheck,
        titleKey: "updates.whatsNew.topics.update_check.title",
        bodyKey: "updates.whatsNew.topics.update_check.body",
        symbol: "arrow.triangle.2.circlepath"
    )

    private static func page(_ name: String, since: String?, count: Int) -> SnapshotPage {
        let items = (0..<count).map { $0.isMultiple(of: 2) ? courseWeeks : longer }
        return SnapshotPage(name: name, width: WhatsNewSheet.width, minHeight: 0) { _ in
            AnyView(WhatsNewSheet(whatsNew: WhatsNewPresentation(since: since, items: items)) {})
        }
    }
}
