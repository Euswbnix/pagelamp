// The reading column (spec §3.0): 760 pt measure, centred, 40 pt gutters (24 below a 760 pt
// detail column). LampBand uses the same layout so the band's text lines up with the page.

import SwiftUI

/// The reading column's geometry: at most `PLLayout.measure` wide, centred, with the gutter the
/// detail column's width calls for. Views use it through `readingMeasure()`.
nonisolated enum ReadingMeasure {
    /// The gutter for a detail column `width` wide.
    static func gutter(for width: CGFloat) -> CGFloat {
        width < PLLayout.measure ? PLLayout.gutterNarrow : PLLayout.gutter
    }

    /// The text column inside a detail column `width` wide.
    static func column(for width: CGFloat) -> CGFloat {
        max(0, min(PLLayout.measure, width - 2 * gutter(for: width)))
    }
}

extension View {
    /// Lays the view out in the reading column: at most `PLLayout.measure` wide, centred, with
    /// the gutter the detail column's width calls for. Plain frames and padding, which SwiftUI
    /// sizes without measuring the content twice. (A custom `Layout` that asked its content for
    /// an ideal size and then for the column size cost about 30% of the main thread on every
    /// page switch; `containerRelativeFrame` in the page's scroll view looped with the scroller.)
    func readingMeasure() -> some View {
        modifier(ReadingMeasureModifier())
    }
}

private struct ReadingMeasureModifier: ViewModifier {
    @Environment(\.detailColumnWidth) private var detailWidth

    func body(content: Content) -> some View {
        content
            .frame(maxWidth: PLLayout.measure, alignment: .leading)
            .padding(.horizontal, ReadingMeasure.gutter(for: detailWidth ?? .infinity))
            .frame(maxWidth: .infinity)
    }
}

/// Page content in the reading column.
struct ReadingColumn<Content: View>: View {
    var spacing: CGFloat
    @ViewBuilder var content: Content

    init(spacing: CGFloat = PLLayout.sectionGap, @ViewBuilder content: () -> Content) {
        self.spacing = spacing
        self.content = content()
    }

    var body: some View {
        // Lazy: only the sections on screen are built when a page appears (the rest as it scrolls).
        LazyVStack(alignment: .leading, spacing: spacing) { content }
            .readingMeasure()
    }
}

/// A reading page's document: the lamp band first, then the reading column. Put it in a
/// `ScrollView` (the screen) or render it directly (snapshots).
struct ReadingPage<Band: View, Content: View>: View {
    var spacing: CGFloat
    @ViewBuilder var band: Band
    @ViewBuilder var content: Content

    init(
        spacing: CGFloat = PLLayout.sectionGap,
        @ViewBuilder band: () -> Band,
        @ViewBuilder content: () -> Content
    ) {
        self.spacing = spacing
        self.band = band()
        self.content = content()
    }

    var body: some View {
        LazyVStack(alignment: .leading, spacing: spacing) {
            band
            content
        }
        .padding(.bottom, PLSpace.s12)
        .frame(maxWidth: .infinity, alignment: .topLeading)
    }
}

extension EnvironmentValues {
    /// The width of the detail column a page is laid out in: the screen measures it, the
    /// snapshot renderer sets it. Rows that switch to a compact layout in a narrow reading column
    /// (course materials, deadlines) read it, so they need no measuring of their own. nil =
    /// unknown (the regular layout).
    @Entry package var detailColumnWidth: CGFloat? = nil
}

/// The detail column's width as the pages need it: updated only when one of the layout
/// decisions below would change. While the sidebar or the inspector animates, the raw width
/// changes on every frame; publishing it re-evaluated the whole page and the toolbar on every
/// frame. Every reader compares the width with one of these thresholds, so its decisions are
/// the same as with the live width.
nonisolated struct DetailWidth: Equatable, Sendable {
    var width: CGFloat

    /// Everything that reads `detailColumnWidth` (or the course page's width) and what it checks.
    static func decisions(_ width: CGFloat) -> [Bool] {
        [
            width < PLLayout.measure,                                                         // reading-column gutter
            width < WindowToolbar.compactBelow,                                               // course toolbar
            ReadingMeasure.isColumn(of: width, narrowerThan: CourseDeadlineRow.compactBelow), // compact deadline rows
            ReadingMeasure.isColumn(of: width, narrowerThan: CourseMaterialRow.compactBelow), // compact material rows
        ]
    }

    static func == (lhs: DetailWidth, rhs: DetailWidth) -> Bool {
        decisions(lhs.width) == decisions(rhs.width)
    }
}

extension View {
    /// Calls `action` with the view's width when a `DetailWidth` decision changes (and once at first).
    func onDetailWidthChange(_ action: @escaping (CGFloat) -> Void) -> some View {
        onGeometryChange(for: DetailWidth.self) { DetailWidth(width: $0.size.width) } action: { action($0.width) }
    }
}

nonisolated extension ReadingMeasure {
    /// Whether the reading column of a detail column `detailWidth` wide is narrower than `limit`.
    static func isColumn(of detailWidth: CGFloat?, narrowerThan limit: CGFloat) -> Bool {
        guard let detailWidth else { return false }
        return column(for: detailWidth) < limit
    }
}
