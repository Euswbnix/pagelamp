// A page's heading (spec §3.0, §4.3): type before boxes.

import SwiftUI

/// A page's heading: optional eyebrow (Subheadline Semibold), Large Title, optional subtitle
/// (Callout, secondary). Usually the content of the page's `LampBand`.
struct PageHeader: View {
    var eyebrow: String?
    var title: String
    var subtitle: String?
    var titleWeight: Font.Weight = .regular

    var body: some View {
        VStack(alignment: .leading, spacing: PLSpace.s1) {
            if let eyebrow {
                Text(eyebrow)
                    .font(PLType.subheadline.font.weight(.semibold))
                    .foregroundStyle(.secondary)
            }
            Text(title)
                .font(PLType.largeTitle.font.weight(titleWeight))
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityAddTraits(.isHeader)
            if let subtitle {
                Text(subtitle)
                    .font(PLType.callout.font)
                    .foregroundStyle(.secondary)
                    .paragraphLineSpacing()
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }
}
