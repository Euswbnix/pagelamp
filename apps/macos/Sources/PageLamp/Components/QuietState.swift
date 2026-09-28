// Quiet, unboxed states inside a section (spec §3.9): S12 empties ("Nothing due in the next 7
// days") and S14 section errors ("Couldn't load your study plan. [Try Again]"). Not boxed, never
// glowing; the rest of the page keeps working.

import SwiftUI
import PageLampModel

/// A glyph, a sentence, an optional hint and optional actions.
struct QuietState<Actions: View>: View {
    var symbol: String
    var title: String
    var message: String?
    @ViewBuilder var actions: Actions

    init(symbol: String, title: String, message: String? = nil, @ViewBuilder actions: () -> Actions) {
        self.symbol = symbol
        self.title = title
        self.message = message
        self.actions = actions()
    }

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: PLSpace.s2) {
            Image(systemName: symbol)
                .symbolRenderingMode(.hierarchical)
                .foregroundStyle(.secondary)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: PLSpace.s1) {
                Text(title)
                    .font(PLType.body.font)
                    .fixedSize(horizontal: false, vertical: true)
                if let message {
                    Text(message)
                        .font(PLType.callout.font)
                        .foregroundStyle(.secondary)
                        .paragraphLineSpacing()
                        .fixedSize(horizontal: false, vertical: true)
                }
                if Actions.self != EmptyView.self {
                    HStack(spacing: PLSpace.s3) { actions }
                        .padding(.top, PLSpace.s1)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .contain)
    }
}

extension QuietState where Actions == EmptyView {
    init(symbol: String, title: String, message: String? = nil) {
        self.init(symbol: symbol, title: title, message: message) { EmptyView() }
    }
}

/// S14: a section that couldn't load, why (when known), and Try Again.
struct SectionError: View {
    var title: String
    var message: String?
    var retry: () -> Void
    @Environment(\.l10n) private var l10n

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: PLSpace.s2) {
            Image(systemName: "exclamationmark.triangle")
                .symbolRenderingMode(.hierarchical)
                .foregroundStyle(PLColor.warning)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: PLSpace.s1) {
                Text(title)
                    .fixedSize(horizontal: false, vertical: true)
                if let message {
                    Text(message)
                        .font(PLType.callout.font)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            Spacer(minLength: PLSpace.s4)
            Button(l10n("mac.actions.tryAgain"), action: retry)
                .buttonStyle(.bordered)
                .controlSize(.small)
        }
        .font(PLType.body.font)
        .accessibilityElement(children: .contain)
    }
}
