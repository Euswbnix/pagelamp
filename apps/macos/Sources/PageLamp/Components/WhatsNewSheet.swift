// What's new after an update (the Tauri app's sheet, for the Mac app's topics): a standard sheet
// on the main window. It is read from the top (the title takes VoiceOver's focus), each topic is
// a heading, and a long list scrolls inside the sheet. Closing it any way counts as read. A
// sheet: fills and standard buttons, never glass.

import SwiftUI
import PageLampModel

extension View {
    /// Presents What's new on this window while `model.whatsNew` has something to show.
    func whatsNewSheet() -> some View {
        modifier(WhatsNewSheetModifier())
    }
}

private struct WhatsNewSheetModifier: ViewModifier {
    @Environment(AppModel.self) private var model

    func body(content: Content) -> some View {
        content.sheet(isPresented: presented) {
            if let whatsNew = model.whatsNew {
                WhatsNewSheet(whatsNew: whatsNew) {
                    Task { await model.acknowledgeWhatsNew() }
                }
                .pageLampEnvironment(model)
            }
        }
    }

    private var presented: Binding<Bool> {
        Binding(
            get: { model.whatsNew != nil },
            // Closed by the system (Esc): read, like Got It.
            set: { if !$0 { Task { await model.acknowledgeWhatsNew() } } }
        )
    }
}

package struct WhatsNewSheet: View {
    let whatsNew: WhatsNewPresentation
    let done: () -> Void

    @Environment(\.l10n) private var l10n
    @Environment(\.drawsControlStandIns) private var standIns
    @AccessibilityFocusState private var titleFocused: Bool
    /// The topics' height as laid out (the scroll view is as tall, up to the limit).
    @State private var topicsHeight = WhatsNewSheet.topicsMaxHeight
    /// Scrolled to the last topic: nothing more below, so no fade.
    @State private var scrolledToEnd = false

    package init(whatsNew: WhatsNewPresentation, done: @escaping () -> Void) {
        self.whatsNew = whatsNew
        self.done = done
    }

    package var body: some View {
        VStack(alignment: .leading, spacing: PLSpace.s4) {
            VStack(alignment: .leading, spacing: PLSpace.s1) {
                Text(l10n("updates.whatsNew.title"))
                    .font(PLType.title2.font.weight(.semibold))
                    .accessibilityAddTraits(.isHeader)
                    .accessibilityFocused($titleFocused)
                if let since = whatsNew.since {
                    Text(l10n("updates.whatsNew.since", ["version": since]))
                        .font(PLType.callout.font)
                        .foregroundStyle(.secondary)
                }
            }
            // The topics at their height, up to the sheet's limit; past it, they scroll, and the
            // bottom edge fades while there's more below (macOS hides scroll bars until you
            // scroll, so the fade says the list goes on).
            if standIns {
                // Offscreen (snapshots) a scroll view draws nothing, and one render pass can't
                // follow a measurement: a layout shows the topics as they are, or their top part
                // with the fade when they're taller than the limit.
                StandInFit(maxHeight: Self.topicsMaxHeight) {
                    topics
                    topics
                        .fixedSize(horizontal: false, vertical: true)
                        .frame(height: Self.topicsMaxHeight, alignment: .top)
                        .mask { Self.fade(active: true) }
                }
                .clipped()
            } else {
                ScrollView {
                    topics.onGeometryChange(for: CGFloat.self) { $0.size.height } action: {
                        topicsHeight = $0
                    }
                }
                .onScrollGeometryChange(for: Bool.self) { geometry in
                    geometry.contentOffset.y + geometry.containerSize.height >= geometry.contentSize.height - 1
                } action: { _, atEnd in
                    scrolledToEnd = atEnd
                }
                .frame(height: min(topicsHeight, Self.topicsMaxHeight))
                .scrollBounceBehavior(.basedOnSize)
                .mask { Self.fade(active: topicsHeight > Self.topicsMaxHeight && !scrolledToEnd) }
            }
            HStack {
                Spacer()
                Button(l10n("updates.whatsNew.done"), action: done)
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding(PLLayout.sheetInset)
        .frame(width: Self.width)
        .onExitCommand(perform: done)
        .onAppear { titleFocused = true }
    }

    private var topics: some View {
        VStack(alignment: .leading, spacing: PLSpace.s4) {
            ForEach(Array(whatsNew.items.enumerated()), id: \.offset) { _, item in
                HStack(alignment: .firstTextBaseline, spacing: PLSpace.s3) {
                    Image(systemName: item.symbol)
                        .foregroundStyle(.secondary)
                        .accessibilityHidden(true)
                    VStack(alignment: .leading, spacing: PLSpace.s1) {
                        Text(l10n(item.titleKey))
                            .font(PLType.body.font.weight(.semibold))
                            .accessibilityAddTraits(.isHeader)
                        Text(l10n(item.bodyKey))
                            .font(PLType.body.font)
                            .foregroundStyle(.secondary)
                            .paragraphLineSpacing()
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// Opaque down to the last `fadeHeight`, then clear: a mask, so it fades the text into
    /// whatever the sheet's background is, in either appearance.
    private static func fade(active: Bool) -> some View {
        GeometryReader { proxy in
            let start = max(0, 1 - fadeHeight / max(proxy.size.height, 1))
            LinearGradient(
                stops: [
                    .init(color: .black, location: 0),
                    .init(color: .black, location: active ? start : 1),
                    .init(color: active ? .clear : .black, location: 1),
                ],
                startPoint: .top,
                endPoint: .bottom
            )
        }
    }

    static let fadeHeight: CGFloat = PLSpace.s8

    /// The sheet's width, and the topics' height before they scroll (the diagnostic preview's
    /// sizes, for a list of short paragraphs).
    package static let width: CGFloat = 480
    static let topicsMaxHeight: CGFloat = 360
}

/// Snapshots only: of two subviews (the content, then its clipped stand-in), shows the first
/// when it fits in `maxHeight` and the second otherwise, and is as tall as what it shows. The
/// other is placed out of sight (the caller clips).
private struct StandInFit: Layout {
    let maxHeight: CGFloat

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        guard let content = subviews.first else { return .zero }
        let natural = content.sizeThatFits(ProposedViewSize(width: proposal.width, height: nil))
        return CGSize(width: proposal.width ?? natural.width, height: min(natural.height, maxHeight))
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        guard subviews.count == 2 else { return }
        let natural = subviews[0].sizeThatFits(ProposedViewSize(width: bounds.width, height: nil))
        let (shown, hidden) = natural.height > maxHeight ? (subviews[1], subviews[0]) : (subviews[0], subviews[1])
        shown.place(at: bounds.origin, proposal: ProposedViewSize(width: bounds.width, height: bounds.height))
        hidden.place(
            at: CGPoint(x: bounds.minX, y: bounds.maxY + natural.height + maxHeight),
            proposal: ProposedViewSize(width: bounds.width, height: nil)
        )
    }
}
