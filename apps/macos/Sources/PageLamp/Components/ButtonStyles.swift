// Content-layer buttons and rows (spec §3.0, §6.4, §7.2): plain row buttons with a hover fill,
// link-style and borderless secondary actions, and row outlines. Every one of them turns into a
// bordered control (or gets a 1 pt outline) under Show Borders. Never glass.

import SwiftUI

/// A plain row button (spec §6.4 Contents rows, §3.1 Next 7 days rows): no chrome at rest, a
/// radius-8 fill on hover and press, a 1 pt outline under Show Borders. The fill bleeds
/// `rowPadding` past the text on both sides, so the text stays on the reading column's edge.
struct RowButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        Row(configuration: configuration)
    }

    private struct Row: View {
        let configuration: Configuration
        @State private var hovering = false
        @Environment(\.accessibilityShowBorders) private var showBorders
        @Environment(\.accessibilityReduceMotion) private var reduceMotion
        @Environment(\.isEnabled) private var isEnabled

        var body: some View {
            let shape = RoundedRectangle(cornerRadius: PLRadius.row)
            configuration.label
                .padding(.horizontal, PLLayout.rowPadding)
                .padding(.vertical, PLSpace.s1 + 2)
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(shape)
                .background { shape.fill(fill) }
                .overlay {
                    if showBorders { shape.strokeBorder(.separator, lineWidth: 1) }
                }
                .padding(.horizontal, -PLLayout.rowPadding)
                .onHover { hovering = $0 && isEnabled }
                .animation(reduceMotion ? nil : PLMotion.hover, value: hovering)
        }

        private var fill: AnyShapeStyle {
            if configuration.isPressed { return AnyShapeStyle(.fill.tertiary) }
            if hovering { return AnyShapeStyle(.fill.quaternary) }
            return AnyShapeStyle(.clear)
        }
    }
}

extension View {
    /// A secondary, borderless action that reads as a link ("Open Sources & Sync", a website):
    /// accent-coloured text with the link pointer, `.bordered` under Show Borders (spec §7.2).
    /// Drawn by SwiftUI (not the AppKit-backed `.link` style), so it also renders offscreen.
    func linkButtonStyle() -> some View {
        modifier(LinkButtonStyle())
    }

    /// A quiet content button: `.borderless` (or `.plain` for text that is itself the control,
    /// like the AI status line), `.bordered` under Show Borders (spec §7.2).
    func quietButtonStyle(plain: Bool = false) -> some View {
        modifier(QuietButtonStyle(plain: plain))
    }

    /// A plain row's 1 pt `.separator` outline under Show Borders (spec §7.2).
    func rowBorder() -> some View {
        modifier(RowBorder())
    }
}

private struct LinkButtonStyle: ViewModifier {
    @Environment(\.accessibilityShowBorders) private var showBorders

    func body(content: Content) -> some View {
        if showBorders {
            content.buttonStyle(.bordered)
        } else {
            content
                .buttonStyle(.plain)
                .foregroundStyle(.tint)
                .pointerStyle(.link)
                .accessibilityAddTraits(.isLink)
        }
    }
}

private struct QuietButtonStyle: ViewModifier {
    var plain: Bool
    @Environment(\.accessibilityShowBorders) private var showBorders

    func body(content: Content) -> some View {
        if showBorders {
            content.buttonStyle(.bordered)
        } else if plain {
            content.buttonStyle(.plain)
        } else {
            content.buttonStyle(.borderless)
        }
    }
}

private struct RowBorder: ViewModifier {
    @Environment(\.accessibilityShowBorders) private var showBorders

    func body(content: Content) -> some View {
        content.overlay {
            if showBorders {
                RoundedRectangle(cornerRadius: PLRadius.row).strokeBorder(.separator, lineWidth: 1)
            }
        }
    }
}
