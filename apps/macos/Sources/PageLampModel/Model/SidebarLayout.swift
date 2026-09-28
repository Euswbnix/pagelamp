// The sidebar's content and geometry (spec §2.3) without the views: which rows and section
// headers it shows, where each one sits for the Sidebar icon size (small / medium / large), and
// where the glass selection capsule goes. The numbers mirror the system source list as measured
// on macOS 27.2 (NSTableView rowSizeStyle, the SwiftUI sidebar List), so the custom list lines up
// with every other sidebar on the Mac; the one deliberate difference is the small row height
// (see `rowHeight`).

import Foundation

/// The sidebar's items in visual order: rows (selectable, one per destination) and headers.
public struct SidebarOutline: Equatable, Sendable {
    public enum Section: Hashable, Sendable {
        case courses
        case setup
    }

    public enum Item: Equatable, Sendable, Identifiable {
        /// A destination and the text type-select matches (the visible title: "This Week", a course
        /// code or name).
        case row(Destination, title: String)
        case header(Section)

        public enum ID: Hashable, Sendable {
            case row(Destination)
            case header(Section)
        }

        public var id: ID {
            switch self {
            case .row(let destination, _): .row(destination)
            case .header(let section): .header(section)
            }
        }
    }

    public let items: [Item]

    /// Builds the M1 sidebar: This Week; Courses (only with courses); Setup: Sources & Sync, Connect.
    public init(courses: [(id: String, title: String)], thisWeek: String, sources: String, connect: String) {
        var items: [Item] = [.row(.thisWeek, title: thisWeek)]
        if !courses.isEmpty {
            items.append(.header(.courses))
            items += courses.map { .row(.course($0.id), title: $0.title) }
        }
        items += [.header(.setup), .row(.sources, title: sources), .row(.connect, title: connect)]
        self.items = items
    }

    /// The selectable rows, in order.
    public var rows: [(destination: Destination, title: String)] {
        items.compactMap { item in
            if case .row(let destination, let title) = item { (destination, title) } else { nil }
        }
    }

    public func contains(_ destination: Destination) -> Bool {
        items.contains { $0.id == .row(destination) }
    }
}

extension AppModel {
    /// The sidebar's outline: the visible courses by code (or name), in the student's language.
    public func sidebarOutline(l10n: L10n) -> SidebarOutline {
        SidebarOutline(
            courses: visibleCourses.map { (id: $0.course.id, title: $0.course.code ?? $0.course.name) },
            thisWeek: l10n("mac.nav.thisWeek"),
            sources: l10n("mac.nav.sources"),
            connect: l10n("mac.nav.connect")
        )
    }
}

/// Sidebar geometry for one Sidebar icon size (System Settings ▸ Appearance), in points. x is
/// measured from the sidebar's leading edge; W is the column width.
public struct SidebarMetrics: Equatable, Sendable {
    public enum Size: Sendable {
        case small, medium, large
    }

    /// A selectable row and the capsule behind it: 24 / 32 / 40, the source-list table's
    /// `rowHeight` at the small / medium / large size. Medium and large match the system's
    /// SwiftUI sidebar exactly. At small that list uses automatic row heights and sizes each row
    /// to its content, about 5 pt above and below the tallest glyph (25–27 pt with these icons);
    /// this list keeps a uniform 24 on purpose, so the one capsule keeps its height as it slides
    /// from row to row (measured on 27.2).
    public let rowHeight: CGFloat
    /// Row title: 11 / 13 / 15 pt, Regular (Semibold when highlighted).
    public let titleSize: CGFloat
    /// Row icon: 11 / 13 / 15 pt Medium at `imageScale(.large)`.
    public let iconSize: CGFloat
    /// The icon column (icon centred in it) from x = cellInset; the title starts
    /// `iconTitleGap` after it: x 42 / 46 / 48.
    public let iconColumn: CGFloat

    public init(_ size: Size) {
        switch size {
        case .small: (rowHeight, titleSize, iconSize, iconColumn) = (24, 11, 11, 22)
        case .medium: (rowHeight, titleSize, iconSize, iconColumn) = (32, 13, 13, 26)
        case .large: (rowHeight, titleSize, iconSize, iconColumn) = (40, 15, 15, 28)
        }
    }

    /// Row content inset on both sides (cell x 16, width W − 32; trailing text ends at W − 16).
    public static let cellInset: CGFloat = 16
    public static let iconTitleGap: CGFloat = 4
    /// Section header: 19 pt tall, 13 pt below the item above it, text 11 pt Semibold at x 14,
    /// centred (the same at every size).
    public static let headerHeight: CGFloat = 19
    public static let headerGap: CGFloat = 13
    public static let headerInset: CGFloat = 14
    public static let headerFontSize: CGFloat = 11
    /// The selection capsule: x 10, width W − 20, the row's full height, radius h/2 (where the
    /// system draws its selection).
    public static let capsuleInset: CGFloat = 10

    /// Where a row's title starts.
    public var titleX: CGFloat { Self.cellInset + iconColumn + Self.iconTitleGap }
}

/// Where each item of an outline sits: y from the top of the first row (content coordinates).
public struct SidebarLayout: Equatable, Sendable {
    public struct Frame: Equatable, Sendable {
        public let top: CGFloat
        public let height: CGFloat
        public var bottom: CGFloat { top + height }

        public init(top: CGFloat, height: CGFloat) {
            self.top = top
            self.height = height
        }
    }

    public let outline: SidebarOutline
    public let metrics: SidebarMetrics
    /// One per item: the item's own frame (a header's frame excludes the gap above it).
    public let frames: [Frame]
    public let contentHeight: CGFloat

    public init(outline: SidebarOutline, metrics: SidebarMetrics) {
        self.outline = outline
        self.metrics = metrics
        var y: CGFloat = 0
        var frames: [Frame] = []
        for (index, item) in outline.items.enumerated() {
            switch item {
            case .header:
                if index > 0 { y += SidebarMetrics.headerGap }
                frames.append(Frame(top: y, height: SidebarMetrics.headerHeight))
                y += SidebarMetrics.headerHeight
            case .row:
                frames.append(Frame(top: y, height: metrics.rowHeight))
                y += metrics.rowHeight
            }
        }
        self.frames = frames
        contentHeight = y
    }

    /// The space above an item (a header's gap).
    public func gap(before index: Int) -> CGFloat {
        guard index > 0, case .header = outline.items[index] else { return 0 }
        return SidebarMetrics.headerGap
    }

    /// The row of `destination`; nil when the sidebar doesn't list it (a hidden course).
    public func frame(of destination: Destination) -> Frame? {
        outline.items.firstIndex { $0.id == .row(destination) }.map { frames[$0] }
    }

    /// The selection capsule for `destination` in a sidebar `width` wide.
    public func capsuleFrame(for destination: Destination, width: CGFloat) -> CGRect? {
        frame(of: destination).map {
            CGRect(
                x: SidebarMetrics.capsuleInset, y: $0.top,
                width: max(width - 2 * SidebarMetrics.capsuleInset, 0), height: $0.height
            )
        }
    }

    /// The row under the pointer at `y`; nil over a header, a gap or past the last row.
    public func destination(atY y: CGFloat) -> Destination? {
        for (index, frame) in frames.enumerated() where y >= frame.top && y < frame.bottom {
            if case .row(let destination, _) = outline.items[index] { return destination }
            return nil
        }
        return nil
    }
}
