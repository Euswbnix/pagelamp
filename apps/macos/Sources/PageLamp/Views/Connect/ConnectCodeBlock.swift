// Code on the Connect page (spec §3.4, W7): a snippet, command or path with its titled copy
// button ("Copy Configuration", "Copy Path", …) — the shared CodeBlock, fed by the setup steps.

import SwiftUI
import PageLampModel

extension CodeBlock {
    /// A step's code with its copy button. While PageLamp runs from a temporary location,
    /// copying anything that contains PageLamp's own path first asks "Copy anyway?" (S15).
    init(copyable: ConnectCopyable, temporaryLocation: Bool = false, @ViewBuilder trailing: () -> Trailing) {
        self.init(
            code: copyable.text,
            copyTitle: copyable.copyTitle,
            copyAccessibilityLabel: copyable.copyAccessibilityLabel,
            confirmBeforeCopy: temporaryLocation && copyable.dependsOnAppLocation,
            trailing: trailing
        )
    }
}

extension CodeBlock where Trailing == EmptyView {
    init(copyable: ConnectCopyable, temporaryLocation: Bool = false) {
        self.init(copyable: copyable, temporaryLocation: temporaryLocation) { EmptyView() }
    }
}
