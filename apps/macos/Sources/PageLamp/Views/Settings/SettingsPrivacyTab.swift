// Settings ▸ Privacy (spec §3.5 W8c; M1 text, the AI-access table is M2): the full AI
// disclosure with when it was confirmed, then the promises behind it. Printed in full; never
// collapsed, truncated or colour-only.

import SwiftUI
import PageLampModel

struct SettingsPrivacyTab: View {
    @Environment(\.l10n) private var l10n

    private static let points: [(key: String, symbol: String)] = [
        ("settings.privacy.keychain", "key"),
        ("settings.privacy.readOnly", "book"),
        ("settings.privacy.aiProvider", "paperplane"),
        ("mac.settings.privacy.perCourse", "switch.2"),
    ]

    var body: some View {
        SettingsForm {
            Section {
                Text(l10n("common.disclosure.full"))
                    .paragraphLineSpacing()
                    .fixedSize(horizontal: false, vertical: true)
                DisclosureAcknowledgement()
            } header: {
                Text(l10n("mac.settings.privacy.disclosureTitle"))
            }

            Section {
                ForEach(Self.points, id: \.key) { point in
                    Label {
                        Text(l10n(point.key))
                            .paragraphLineSpacing()
                            .fixedSize(horizontal: false, vertical: true)
                    } icon: {
                        Image(systemName: point.symbol)
                            .symbolRenderingMode(.hierarchical)
                            .foregroundStyle(.secondary)
                    }
                }
            }
        }
    }
}
