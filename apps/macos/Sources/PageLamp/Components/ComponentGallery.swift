// The shared content components on one page, for snapshot review (never shown in the app; the
// snapshot catalogue in PageLampSnapshots lists it). Real strings only: every text comes from
// the string table.

import SwiftUI
import PageLampModel

package struct ComponentGallery: View {
    @Environment(\.l10n) private var l10n

    package init() {}

    package var body: some View {
        ReadingPage {
            LampBand(lit: true) {
                PageHeader(
                    eyebrow: l10n("mac.sidebar.setup"),
                    title: l10n("mac.nav.thisWeek"),
                    subtitle: l10n.plural("courses.thisWeek.summary", count: 3)
                )
            }
        } content: {
            ReadingColumn {
                SectionHeader(
                    title: l10n("mac.courses.thisWeek.next7"),
                    detail: l10n.plural("mac.thisWeek.deadlineCount", count: 3)
                )
                // The arbiter: the source fix outranks the page's own primary action.
                Callout(
                    tone: .danger,
                    symbol: "key",
                    title: l10n("mac.capsule.needsToken", ["source": "Demo Canvas"]),
                    message: l10n("common.errors.auth")
                ) {
                    Button(l10n("mac.actions.replaceToken")) {}
                        .arbitratedButtonStyle(.fixSource("canvas:canvas.demo.test"))
                    Button(l10n("mac.actions.syncAll")) {}
                        .arbitratedButtonStyle(.pagePrimary)
                }
                Callout(tone: .privacy, title: l10n("common.disclosure.title"), message: l10n("common.disclosure.short"))
                Callout(tone: .info, symbol: "hand.raised", title: l10n("common.aiMaterials.withheld_by_policy"), message: l10n("mac.course.noAI.body"))
                CodeBlock(code: "xattr -dr com.apple.quarantine \"/Applications/PageLamp Preview.app\"")
                EmptyState(
                    symbol: "tray",
                    title: l10n("courses.thisWeek.nothingDue"),
                    message: l10n("courses.thisWeek.emptyHint")
                )
            }
        }
        .primaryActionCandidates([.fixSource("canvas:canvas.demo.test"), .pagePrimary])
    }
}
