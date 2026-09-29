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
            // The topics at their height, up to the sheet's limit; past it, they scroll.
            if standIns {
                // Offscreen (snapshots) a scroll view draws nothing: its top part stands in.
                topics
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxHeight: Self.topicsMaxHeight, alignment: .top)
                    .clipped()
            } else {
                ScrollView {
                    topics.onGeometryChange(for: CGFloat.self) { $0.size.height } action: {
                        topicsHeight = $0
                    }
                }
                .frame(height: min(topicsHeight, Self.topicsMaxHeight))
                .scrollBounceBehavior(.basedOnSize)
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

    /// The sheet's width, and the topics' height before they scroll (the diagnostic preview's
    /// sizes, for a list of short paragraphs).
    package static let width: CGFloat = 480
    static let topicsMaxHeight: CGFloat = 360
}
