// What's new after an update (the Tauri app's sheet, for the Mac app): the facade says which
// topics a new version brings for this shell and when the student has seen them; this file says
// how the Mac app shows each topic.

import PageLampKit

/// One topic as the sheet shows it.
public struct WhatsNewItem: Equatable, Sendable {
    public let topic: WhatsNewTopic
    public let titleKey: String
    public let bodyKey: String
    /// An SF Symbol beside the title.
    public let symbol: String

    public init(topic: WhatsNewTopic, titleKey: String, bodyKey: String, symbol: String) {
        self.topic = topic
        self.titleKey = titleKey
        self.bodyKey = bodyKey
        self.symbol = symbol
    }
}

/// The sheet's content for this launch.
public struct WhatsNewPresentation: Equatable, Sendable {
    /// The version the student updated from (none after 0.1, which recorded no version).
    public let since: String?
    public let items: [WhatsNewItem]

    public init(since: String?, items: [WhatsNewItem]) {
        self.since = since
        self.items = items
    }
}

public enum WhatsNewCatalog {
    /// The topics the Mac app shows. A dictionary, not a `switch`: new topics arrive with their
    /// features, branch by branch, as cases of an enum from another module, and warnings are
    /// errors in this package (a `default:` that can never run, or a missing case, stops the
    /// build). A branch that adds a topic adds its row here, with its strings.
    ///
    /// No update check: the facade sends that topic to the Tauri app only, whose updater it
    /// describes, and only the Tauri app's acknowledgement counts as its update disclosure.
    static let entries: [WhatsNewTopic: WhatsNewItem] = [
        .courseWeeks: WhatsNewItem(
            topic: .courseWeeks,
            titleKey: "updates.whatsNew.topics.course_weeks.title",
            bodyKey: "updates.whatsNew.topics.course_weeks.body",
            symbol: "calendar"
        ),
    ]

    /// The topics this build can show, in the facade's order: those with a row here and both
    /// strings (`has`: the string table knows the key). The rest are left out, never the
    /// acknowledgement (see `AppModel.loadWhatsNew`).
    public static func items(for topics: [WhatsNewTopic], has: (String) -> Bool) -> [WhatsNewItem] {
        topics.compactMap { entries[$0] }.filter { has($0.titleKey) && has($0.bodyKey) }
    }
}
