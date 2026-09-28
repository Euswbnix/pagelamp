// Callout (spec §3.0): a boxed note in the content layer. Never glass.

import SwiftUI

/// Glyph + Headline title + body + actions on `.fill.tertiary`, radius 12. The tone shows only in
/// the glyph; the text stays `.primary` (spec §4.2). A 1 pt border appears only under Increase
/// Contrast or Show Borders.
struct Callout<Actions: View>: View {
    enum Tone {
        case info
        case warning
        case danger
        case privacy
        case success

        var symbol: String {
            switch self {
            case .info: "info.circle"
            case .warning: "exclamationmark.triangle"
            case .danger: "xmark.octagon"
            case .privacy: "lock.shield"
            case .success: "checkmark.circle"
            }
        }

        var glyphStyle: AnyShapeStyle {
            switch self {
            case .info, .privacy: AnyShapeStyle(.secondary)
            case .warning: AnyShapeStyle(PLColor.warning)
            case .danger: AnyShapeStyle(PLColor.danger)
            case .success: AnyShapeStyle(PLColor.success)
            }
        }
    }

    var tone: Tone
    /// Overrides the tone's glyph (e.g. `key` for an expired token).
    var symbol: String?
    var title: String
    var message: String?
    @ViewBuilder var actions: Actions

    init(
        tone: Tone,
        symbol: String? = nil,
        title: String,
        message: String? = nil,
        @ViewBuilder actions: () -> Actions
    ) {
        self.tone = tone
        self.symbol = symbol
        self.title = title
        self.message = message
        self.actions = actions()
    }

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: PLSpace.s3) {
            Image(systemName: symbol ?? tone.symbol)
                .symbolRenderingMode(.hierarchical)
                .foregroundStyle(tone.glyphStyle)
                .font(PLType.headline.font)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: PLSpace.s1) {
                Text(title)
                    .font(PLType.headline.font)
                    .fixedSize(horizontal: false, vertical: true)
                if let message {
                    Text(message)
                        .font(PLType.body.font)
                        .paragraphLineSpacing()
                        .fixedSize(horizontal: false, vertical: true)
                }
                if Actions.self != EmptyView.self {
                    HStack(spacing: PLSpace.s2) { actions }
                        .padding(.top, PLSpace.s2)
                }
            }
        }
        .padding(PLSpace.s4)
        .frame(maxWidth: .infinity, alignment: .leading)
        .calloutSurface()
        .accessibilityElement(children: .contain)
    }
}

extension Callout where Actions == EmptyView {
    init(tone: Tone, symbol: String? = nil, title: String, message: String? = nil) {
        self.init(tone: tone, symbol: symbol, title: title, message: message) { EmptyView() }
    }
}

/// A one-sentence note in a callout box (S11 week notes, "Today is outside the term"): like
/// `Callout`, but the sentence is body text rather than a bold title.
struct CalloutNote<Actions: View>: View {
    var symbol: String
    var text: Text
    @ViewBuilder var actions: Actions

    init(symbol: String, text: Text, @ViewBuilder actions: () -> Actions) {
        self.symbol = symbol
        self.text = text
        self.actions = actions()
    }

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: PLSpace.s3) {
            Image(systemName: symbol)
                .symbolRenderingMode(.hierarchical)
                .foregroundStyle(.secondary)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: PLSpace.s2) {
                text
                    .font(PLType.body.font)
                    .paragraphLineSpacing()
                    .fixedSize(horizontal: false, vertical: true)
                if Actions.self != EmptyView.self {
                    HStack(spacing: PLSpace.s2) { actions }
                }
            }
        }
        .padding(PLSpace.s4)
        .frame(maxWidth: .infinity, alignment: .leading)
        .calloutSurface()
        .accessibilityElement(children: .contain)
    }
}

extension CalloutNote where Actions == EmptyView {
    init(symbol: String, text: Text) {
        self.init(symbol: symbol, text: text) { EmptyView() }
    }
}

extension View {
    /// The callout surface (spec §3.0, §7.2): a system fill in `.rect(cornerRadius: 12)` that is
    /// also the container shape for concentric insides; a 1 pt `.separator` border only under
    /// Increase Contrast or Show Borders. Callouts, notes and source sections share it.
    func calloutSurface(_ fill: CalloutFill = .tertiary) -> some View {
        modifier(CalloutSurface(fill: fill))
    }
}

enum CalloutFill {
    case tertiary
    case quaternary
}

private struct CalloutSurface: ViewModifier {
    var fill: CalloutFill
    @Environment(\.colorSchemeContrast) private var contrast
    @Environment(\.accessibilityShowBorders) private var showBorders

    func body(content: Content) -> some View {
        let shape = RoundedRectangle(cornerRadius: PLRadius.callout)
        content
            .background {
                switch fill {
                case .tertiary: shape.fill(.fill.tertiary)
                case .quaternary: shape.fill(.fill.quaternary)
                }
            }
            .overlay {
                if contrast == .increased || showBorders {
                    shape.strokeBorder(.separator, lineWidth: 1)
                }
            }
            .containerShape(.rect(cornerRadius: PLRadius.callout))
    }
}
