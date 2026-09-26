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
    func accessoryBar() -> some View {
        safeAreaBar(edge: .bottom) { AccessoryBar() }
    }
}
