// One floating accessory bar (spec §6.2): the main window's only custom glass. One
// GlassEffectContainer holds the status capsule (and in M2 its bubbles and the week scrubber).

import SwiftUI
import PageLampModel

struct AccessoryBar: View {
    @Environment(AppModel.self) private var model
    @Namespace private var glass

    var body: some View {
        GlassEffectContainer(spacing: PLLayout.accessoryContainerSpacing) {
            StatusCapsule(state: model.capsule, ns: glass)
        }
        .padding(.bottom, PLSpace.s3)
    }
}

extension View {
    /// Pins the accessory bar to the bottom of a page's scroll view (`safeAreaBar`, so content
    /// scrolls under it with the soft edge effect). Every main-window page applies it once.
    ///
    /// The bar's inset layout asks the page for its minimum height on every layout pass, and a
    /// scroll view answers by laying out all of its content; the shield answers it directly.
    func accessoryBar() -> some View {
        minimumSizeShield()
            .safeAreaBar(edge: .bottom) { AccessoryBar() }
    }
}
