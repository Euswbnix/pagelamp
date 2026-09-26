// The window sizes the snapshot tool (PageLampSnapshots) lays pages out at, from the design
// tokens (PLSize is internal to this module, like every generated token).

import CoreGraphics

package enum WindowMetrics {
    package static let mainWidth = PLSize.windowMainWidth
    package static let mainHeight = PLSize.windowMainHeight
    package static let mainMinWidth = PLSize.windowMainMinWidth
    package static let sidebarIdeal = PLSize.sidebarIdeal
    package static let sidebarMin = PLSize.sidebarMin
    package static let inspectorIdeal = PLSize.inspectorIdeal
    package static let inspectorMin = PLSize.inspectorMin
    package static let settingsWidth = PLSize.settingsWidth
}
