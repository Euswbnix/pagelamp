// One light: the lamp band (spec §6.1). Content layer — never glass.

import SwiftUI

/// The warm pool of light that marks *now*. Decorative: hidden from VoiceOver, and always next to
/// a word ("Today", "This week") so it never carries meaning alone.
struct LampWash: View {
    var lit: Bool
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        EllipticalGradient(
            colors: [
                PLColor.lampWash.opacity(scheme == .dark ? PLColor.lampWashOpacityDark : PLColor.lampWashOpacityLight),
                .clear,
            ],
            center: UnitPoint(x: 0.06, y: 0),
            startRadiusFraction: 0,
            endRadiusFraction: lit ? 0.75 : 0.6
        )
        .opacity(lit ? 1 : 0)
    }
}

/// The first view of every reading page: full-bleed across the detail column, its text in the
/// reading column. The wash alone gets `backgroundExtensionEffect`, so the light spills under
/// the glass sidebar and inspector and reaches under the toolbar.
///
/// Under Reduce Transparency or Increase Contrast the pool becomes a 3 pt lamp rule beside the
/// text (spec §7.2). One band per screen, lit only for *now*.
struct LampBand<Content: View>: View {
    var lit: Bool
    @ViewBuilder var content: Content

    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @Environment(\.colorSchemeContrast) private var contrast
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    init(lit: Bool, @ViewBuilder content: () -> Content) {
        self.lit = lit
        self.content = content()
    }

    private var flat: Bool { reduceTransparency || contrast == .increased }

    var body: some View {
        // Text: the reading column. Band: full-bleed.
        ReadingMeasure {
            VStack(alignment: .leading, spacing: PLSpace.s2) { content }
                .padding(.leading, flat && lit ? PLSpace.s3 : 0)
                .overlay(alignment: .leading) {
                    if flat && lit {
                        Rectangle().fill(PLColor.lampRule).frame(width: 3).accessibilityHidden(true)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.vertical, PLSpace.s6)
        .frame(maxWidth: .infinity)
            .background(alignment: .topLeading) {
                LampWash(lit: lit && !flat)
                    .backgroundExtensionEffect(isEnabled: !flat)
                    .ignoresSafeArea(edges: .top)
                    .accessibilityHidden(true)
            }
            .animation(reduceMotion ? nil : PLMotion.calm, value: lit)
    }
}
