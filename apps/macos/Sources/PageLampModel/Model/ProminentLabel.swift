// The label colour on a surface tinted with the student's accent colour (the capsule's fix
// bubble, spec §6.2). White is the platform's label on a tinted control, and it stays white on
// every accent where it still reaches 3:1 (blue, purple, pink, red, graphite); on the light
// accents (yellow, orange, green) white falls below that and the label turns black.
//
// Why not "whichever contrasts more": by WCAG 2's formula black out-contrasts white on any colour
// brighter than a luminance of about 0.18, which includes the system blue (#007AFF, 0.21). Black
// on the default blue would look wrong on a Mac, where every tinted control is white on blue.

import Foundation

public enum ProminentLabel {
    /// The contrast white must reach on the accent to stay the label: WCAG 2's minimum for
    /// bold text and user-interface components.
    public static let minimumWhiteContrast = 3.0

    /// Whether the label on a surface of this sRGB colour (gamma-encoded components 0…1) is
    /// black: white would fall below `minimumWhiteContrast` there.
    public static func prefersDarkText(red: Double, green: Double, blue: Double) -> Bool {
        contrastOfWhite(red: red, green: green, blue: blue) < minimumWhiteContrast
    }

    /// WCAG 2 contrast ratio of white text on this sRGB colour.
    public static func contrastOfWhite(red: Double, green: Double, blue: Double) -> Double {
        1.05 / (relativeLuminance(red: red, green: green, blue: blue) + 0.05)
    }

    /// WCAG 2 relative luminance of an sRGB colour.
    public static func relativeLuminance(red: Double, green: Double, blue: Double) -> Double {
        func linear(_ component: Double) -> Double {
            let c = min(max(component, 0), 1)
            return c <= 0.04045 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4)
        }
        return 0.2126 * linear(red) + 0.7152 * linear(green) + 0.0722 * linear(blue)
    }
}
