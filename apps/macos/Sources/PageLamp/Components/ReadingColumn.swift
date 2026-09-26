// The reading column (spec §3.0): 760 pt measure, centred, 40 pt gutters (24 below a 760 pt
// detail column). LampBand uses the same layout so the band's text lines up with the page.

import SwiftUI

/// Lays its content out in the reading column of whatever width it is offered: at most
/// `PLLayout.measure` wide, centred, with the gutter that width calls for. A layout (not a
/// measured width in state), so the column is right in the first pass: offscreen renders and
/// the first frame never show a column laid out for another width.
struct ReadingMeasure: Layout {
    /// The gutter for a detail column `width` wide.
    static func gutter(for width: CGFloat) -> CGFloat {
        width < PLLayout.measure ? PLLayout.gutterNarrow : PLLayout.gutter
    }

    /// The text column inside a detail column `width` wide.
    static func column(for width: CGFloat) -> CGFloat {
        max(0, min(PLLayout.measure, width - 2 * gutter(for: width)))
    }

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        guard let content = subviews.first else { return .zero }
        if let width = proposal.width {
            let size = content.sizeThatFits(ProposedViewSize(width: Self.column(for: width), height: nil))
            return CGSize(width: width, height: size.height)
        }
        // No width offered (ideal size): the content's own width, capped at the measure.
        let ideal = content.sizeThatFits(.unspecified)
        let column = min(ideal.width, PLLayout.measure)
        let size = content.sizeThatFits(ProposedViewSize(width: column, height: nil))
        return CGSize(width: column + 2 * PLLayout.gutter, height: size.height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        guard let content = subviews.first else { return }
        let column = Self.column(for: bounds.width)
        content.place(
            at: CGPoint(x: bounds.minX + (bounds.width - column) / 2, y: bounds.minY),
            anchor: .topLeading,
            proposal: ProposedViewSize(width: column, height: bounds.height)
        )
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
        ReadingMeasure {
            VStack(alignment: .leading, spacing: spacing) { content }
                .frame(maxWidth: .infinity, alignment: .leading)
        }
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
        VStack(alignment: .leading, spacing: spacing) {
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

extension ReadingMeasure {
    /// Whether the reading column of a detail column `detailWidth` wide is narrower than `limit`.
    static func isColumn(of detailWidth: CGFloat?, narrowerThan limit: CGFloat) -> Bool {
        guard let detailWidth else { return false }
        return column(for: detailWidth) < limit
    }
}
