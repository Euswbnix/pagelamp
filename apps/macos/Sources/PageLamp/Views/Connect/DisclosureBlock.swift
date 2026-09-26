// The AI disclosure (ARCHITECTURE rule 8; spec §3.4, §3.5 Privacy): the full sentence, never
// collapsed or truncated, with when the student confirmed it. A privacy callout; never glass.

import SwiftUI
import PageLampModel

extension EnvironmentValues {
    /// When the student confirmed the AI disclosure. Nothing on the Mac records it before M2's
    /// welcome and Add Source gate (spec §13 #1: shared preferences), so it is nil in M1 and the
    /// block says "You haven't confirmed this yet…". M2 sets it in `pageLampEnvironment(_:)`.
    @Entry package var disclosureAcknowledgedAt: Date? = nil
}

struct DisclosureBlock: View {
    /// "What your AI app can read" (Connect).
    var title: String

    @Environment(\.l10n) private var l10n

    var body: some View {
        Callout(tone: .privacy, title: title, message: l10n("common.disclosure.full")) {
            DisclosureAcknowledgement()
        }
    }
}

/// "✓ You confirmed this on Sep 26, 2026." or "You haven't confirmed this yet…" (glyph + words).
struct DisclosureAcknowledgement: View {
    @Environment(\.l10n) private var l10n
    @Environment(\.disclosureAcknowledgedAt) private var acknowledgedAt

    var body: some View {
        Label {
            Text(text)
                .fixedSize(horizontal: false, vertical: true)
        } icon: {
            Image(systemName: acknowledgedAt == nil ? "circle.dashed" : "checkmark.circle")
                .foregroundStyle(acknowledgedAt == nil ? AnyShapeStyle(.secondary) : AnyShapeStyle(PLColor.success))
        }
        .font(PLType.callout.font)
        .foregroundStyle(.secondary)
    }

    private var text: String {
        guard let acknowledgedAt else { return l10n("settings.privacy.notAcknowledged") }
        return l10n("common.disclosure.acknowledgedOn", [
            "date": acknowledgedAt.formatted(.dateTime.year().month(.abbreviated).day().locale(l10n.locale)),
        ])
    }
}
