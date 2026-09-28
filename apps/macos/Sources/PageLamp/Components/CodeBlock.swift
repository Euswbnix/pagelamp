// CodeBlock and CopyButton (spec §3.0, §3.4): selectable monospaced text with a titled Copy
// button below it. Content layer.

import SwiftUI
import PageLampModel

/// Code, paths and config snippets: Callout-size SF Mono, selectable, on `.fill.quaternary` in a
/// concentric shape; then its Copy button ("Copy Configuration", "Copy Path", …) and any extra
/// buttons (Show in Finder).
struct CodeBlock<Trailing: View>: View {
    var code: String
    /// The Copy button's title; "Copy" by default.
    var copyTitle: String?
    /// VoiceOver label of the Copy button when its title alone is ambiguous on the page.
    var copyAccessibilityLabel: String?
    /// Ask "Copy anyway?" first (S15: the text points to a place that won't last).
    var confirmBeforeCopy = false
    @ViewBuilder var trailing: Trailing

    @Environment(\.l10n) private var l10n

    init(
        code: String,
        copyTitle: String? = nil,
        copyAccessibilityLabel: String? = nil,
        confirmBeforeCopy: Bool = false,
        @ViewBuilder trailing: () -> Trailing
    ) {
        self.code = code
        self.copyTitle = copyTitle
        self.copyAccessibilityLabel = copyAccessibilityLabel
        self.confirmBeforeCopy = confirmBeforeCopy
        self.trailing = trailing()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: PLSpace.s2) {
            Text(verbatim: code.trimmingTrailingNewlines)
                .font(PLType.mono.font)
                .textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(PLSpace.s3)
                .background(
                    .fill.quaternary,
                    in: ConcentricRectangle(corners: .concentric(minimum: .fixed(PLRadius.innerMin)), isUniform: true)
                )
            HStack(spacing: PLSpace.s2) {
                CopyButton(
                    title: copyTitle ?? l10n("common.actions.copy"),
                    text: code,
                    accessibilityLabel: copyAccessibilityLabel,
                    confirmBeforeCopy: confirmBeforeCopy
                )
                trailing
            }
            .controlSize(.small)
        }
    }
}

extension CodeBlock where Trailing == EmptyView {
    init(code: String, copyTitle: String? = nil, copyAccessibilityLabel: String? = nil, confirmBeforeCopy: Bool = false) {
        self.init(
            code: code,
            copyTitle: copyTitle,
            copyAccessibilityLabel: copyAccessibilityLabel,
            confirmBeforeCopy: confirmBeforeCopy
        ) { EmptyView() }
    }
}

/// A bordered Copy button that shows "Copied" for 2 s (`.symbolEffect(.replace)`, none under
/// Reduce Motion) and announces it. With `confirmBeforeCopy` it first asks "Copy anyway?"
/// (Cancel is the default, spec S15).
struct CopyButton: View {
    var title: String
    var text: String
    var accessibilityLabel: String?
    var confirmBeforeCopy = false

    @Environment(\.l10n) private var l10n
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var copied = false
    @State private var confirming = false

    var body: some View {
        Button {
            if confirmBeforeCopy {
                confirming = true
            } else {
                copy()
            }
        } label: {
            Label(copied ? l10n("common.actions.copied") : title, systemImage: copied ? "checkmark" : "doc.on.doc")
                .contentTransition(reduceMotion ? .identity : .symbolEffect(.replace))
        }
        .buttonStyle(.bordered)
        .accessibilityLabel(accessibilityLabel ?? title)
        .alert(l10n("mac.connect.copyAnyway.title"), isPresented: $confirming) {
            Button(l10n("common.actions.cancel"), role: .cancel) {}
                .keyboardShortcut(.defaultAction)
            Button(l10n("mac.connect.copyAnyway.confirm")) { copy() }
        } message: {
            Text(l10n("mac.connect.copyAnyway.message"))
        }
        .task(id: copied) {
            guard copied else { return }
            try? await Task.sleep(for: .seconds(2))
            copied = false
        }
    }

    private func copy() {
        Pasteboard.copy(text, announce: l10n("common.actions.copied"))
        copied = true
    }
}

private extension String {
    /// Snippets often end in a newline; the block shouldn't show an empty last line (the copy
    /// keeps it).
    var trimmingTrailingNewlines: String {
        var text = Substring(self)
        while let last = text.last, last.isNewline { text = text.dropLast() }
        return String(text)
    }
}
