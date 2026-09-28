// A section heading (spec §3.0, §4.3): type before boxes.

import SwiftUI

/// A section title (Title 3 Semibold, `.isHeader`) with an optional trailing detail, then a
/// hairline 12 pt below.
struct SectionHeader: View {
    var title: String
    var detail: String?

    var body: some View {
        VStack(alignment: .leading, spacing: PLLayout.titleToRule) {
            HStack(alignment: .firstTextBaseline) {
                Text(title)
                    .font(PLType.title3.font)
                    .accessibilityAddTraits(.isHeader)
                Spacer(minLength: PLSpace.s4)
                if let detail {
                    Text(detail)
                        .font(PLType.callout.font)
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                }
            }
            Divider()
        }
    }
}
