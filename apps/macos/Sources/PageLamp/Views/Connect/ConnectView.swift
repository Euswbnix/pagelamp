// Connect your AI app (spec §3.4, M1): the full static flow. Order: temporary-location warning
// (S15) → intro → the full AI disclosure → How it works → Set up (client picker, numbered steps,
// Good to know) → Try it → the quarantine hint (ad-hoc builds). Read-only: PageLamp never writes
// an AI app's config; the student copies and pastes.

import AppKit
import SwiftUI
import PageLampKit
import PageLampModel

struct ConnectView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n
    @State private var connect = ConnectModel()

    var body: some View {
        ScrollView {
            ConnectPage(connect: connect)
        }
        .scrollEdgeEffectStyle(.soft, for: .bottom)
        .accessoryBar()
        .navigationTitle(l10n("mac.nav.connect"))
        // Reload when the data source changes (mock ↔ live) and after refreshes (the configs
        // carry the data-folder override and the launch location).
        .task(id: ConnectLoadKey(dataMode: model.dataMode, refresh: model.refreshCount)) {
            await connect.load(
                service: model.service,
                binary: model.sidecarPath,
                temporaryLocation: model.temporaryLocation
            )
        }
    }
}

private struct ConnectLoadKey: Equatable {
    var dataMode: DataMode
    var refresh: Int
}

/// The page's document: rendered in the scroll view and by the snapshot harness.
struct ConnectPage: View {
    let connect: ConnectModel
    /// Whether to show the quarantine hint (ad-hoc and unsigned builds; spec §3.4).
    let showsQuarantineHint: Bool

    @Environment(\.l10n) private var l10n

    init(connect: ConnectModel, showsQuarantineHint: Bool? = nil) {
        self.connect = connect
        self.showsQuarantineHint = showsQuarantineHint ?? CodeSigning.isAdHocOrUnsigned
    }

    var body: some View {
        ReadingPage {
            LampBand(lit: false) {
                PageHeader(title: l10n("connect.title"))
            }
        } content: {
            ReadingColumn {
                // Spec §3.4 order: the temporary-location warning (S15) → the intro → the full
                // AI disclosure.
                VStack(alignment: .leading, spacing: PLSpace.s4) {
                    if let location = connect.data?.temporaryLocation {
                        let text = ConnectSetup.temporaryLocationText(location, l10n: l10n)
                        Callout(tone: .warning, title: text.title, message: text.body)
                    }
                    Text(l10n("connect.description"))
                        .font(PLType.body.font)
                        .paragraphLineSpacing()
                        .fixedSize(horizontal: false, vertical: true)
                    DisclosureBlock(title: l10n("mac.connect.disclosureTitle"))
                }
                HowItWorksSection()
                SetUpSection(connect: connect)
                TryItSection()
                if showsQuarantineHint, let command = connect.data?.launchCommand {
                    QuarantineCallout(launchCommand: command)
                }
            }
        }
    }
}

/// Four short facts: nothing to keep open, data stays local, once per app, copy again after
/// moving PageLamp.
private struct HowItWorksSection: View {
    @Environment(\.l10n) private var l10n

    private static let points: [(key: String, symbol: String)] = [
        ("connect.howItWorks.background", "power"),
        ("connect.howItWorks.local", "internaldrive"),
        ("connect.howItWorks.once", "repeat"),
        ("connect.howItWorks.moved", "folder"),
    ]

    var body: some View {
        VStack(alignment: .leading, spacing: PLSpace.s4) {
            SectionHeader(title: l10n("connect.howItWorks.title"))
            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 280), spacing: PLSpace.s6, alignment: .topLeading)],
                alignment: .leading,
                spacing: PLSpace.s3
            ) {
                ForEach(Self.points, id: \.key) { point in
                    Label {
                        Text(l10n(point.key))
                            .paragraphLineSpacing()
                            .fixedSize(horizontal: false, vertical: true)
                    } icon: {
                        Image(systemName: point.symbol)
                            .symbolRenderingMode(.hierarchical)
                    }
                    .font(PLType.body.font)
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
    }
}

/// Set up: the client picker, what kind of setup it is, its steps and notes.
private struct SetUpSection: View {
    let connect: ConnectModel
    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n

    var body: some View {
        VStack(alignment: .leading, spacing: PLSpace.s4) {
            SectionHeader(title: l10n("mac.connect.setUpLabel"))
            switch connect.phase {
            case .loading:
                HStack(spacing: PLSpace.s2) {
                    ProgressView().controlSize(.small)
                    Text(l10n("common.states.loading"))
                        .foregroundStyle(.secondary)
                }
                .accessibilityElement(children: .combine)
            case .failed(let failure):
                Callout(tone: .warning, title: l10n("connect.errorTitle"), message: failure.localizedDescription(in: l10n)) {
                    retryButton
                }
            case .loaded(let data):
                if data.configs.isEmpty {
                    EmptyState(
                        symbol: "cable.connector",
                        title: l10n("connect.empty.title"),
                        message: l10n("connect.empty.description")
                    ) {
                        retryButton
                    }
                } else {
                    loaded(data)
                }
            }
        }
    }

    private var retryButton: some View {
        Button(l10n("mac.actions.tryAgain")) {
            Task {
                await connect.load(service: model.service, binary: model.sidecarPath, temporaryLocation: model.temporaryLocation)
            }
        }
        .buttonStyle(.bordered)
    }

    @ViewBuilder private func loaded(_ data: ConnectData) -> some View {
        @Bindable var connect = connect
        Picker(l10n("mac.connect.setUpLabel"), selection: $connect.selectedClient) {
            ForEach(data.configs, id: \.client) { config in
                Text(ConnectSetup.pickerTitle(for: config, l10n: l10n))
                    .tag(Optional(config.client))
            }
        }
        .labelsHidden()
        .clientPickerStyle()
        .fixedSize()
        if let config = connect.selectedConfig {
            ClientSummary(config: config, configured: ConnectSetup.isConfigured(config.client, in: data.presence))
            InstallStepsView(config: config, temporaryLocation: data.temporaryLocation != nil)
                .id(config.client)
                .padding(.top, PLSpace.s2)
            ConnectNotesView(config: config)
                .padding(.top, PLSpace.s2)
        }
    }
}

extension View {
    /// `.tabs` on macOS 27, else `.segmented` (spec §3.4).
    @ViewBuilder fileprivate func clientPickerStyle() -> some View {
        if #available(macOS 27, *) {
            pickerStyle(.tabs)
        } else {
            pickerStyle(.segmented)
        }
    }
}

/// "Easiest to start with · Add to a config file", or "✓ Set up · Run in a terminal".
private struct ClientSummary: View {
    let config: McpClientConfig
    let configured: Bool
    @Environment(\.l10n) private var l10n

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: PLSpace.s2) {
            if configured {
                Label(l10n("mac.connect.configured"), systemImage: "checkmark.circle")
                    .labelStyle(SummaryLabelStyle(glyph: AnyShapeStyle(PLColor.success)))
                separator
            } else if config.client == .claudeDesktop {
                Text(l10n("connect.clients.recommended"))
                separator
            }
            Text(ConnectSetup.installKindText(config.installKind, l10n: l10n))
        }
        .font(PLType.callout.font)
        .foregroundStyle(.secondary)
        .accessibilityElement(children: .combine)
    }

    private var separator: some View {
        Text(verbatim: "·")
            .foregroundStyle(.tertiary)
            .accessibilityHidden(true)
    }
}

private struct SummaryLabelStyle: LabelStyle {
    let glyph: AnyShapeStyle

    func makeBody(configuration: Configuration) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: PLSpace.s1) {
            configuration.icon.foregroundStyle(glyph)
            configuration.title
        }
    }
}

/// Example questions for once the AI app is connected.
private struct TryItSection: View {
    @Environment(\.l10n) private var l10n

    var body: some View {
        VStack(alignment: .leading, spacing: PLSpace.s3) {
            SectionHeader(title: l10n("connect.tryIt.title"))
            Text(l10n("connect.tryIt.thenAsk"))
                .foregroundStyle(.secondary)
            PromptRow(prompt: l10n("connect.tryIt.promptWeek"))
            Text(l10n("connect.tryIt.or"))
                .foregroundStyle(.secondary)
            PromptRow(prompt: l10n("connect.tryIt.promptPlan"))
        }
        .font(PLType.body.font)
    }
}

/// A prompt to paste into the AI app, with Copy.
private struct PromptRow: View {
    let prompt: String
    @Environment(\.l10n) private var l10n

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: PLSpace.s3) {
            Text(prompt)
                .fontWeight(.medium)
                .textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
            CopyButton(
                title: l10n("common.actions.copy"),
                text: prompt,
                accessibilityLabel: l10n("mac.actions.copyThePrompt")
            )
            .controlSize(.small)
        }
        .padding(.horizontal, PLSpace.s3)
        .padding(.vertical, PLSpace.s2)
        .background(.fill.quaternary, in: .rect(cornerRadius: PLRadius.row))
    }
}

/// "If macOS blocks PageLamp": Open Anyway first, then the xattr fallback for the app every AI
/// app launches (spec S15; ad-hoc builds only).
private struct QuarantineCallout: View {
    let launchCommand: String
    @Environment(\.l10n) private var l10n

    var body: some View {
        Callout(
            tone: .info,
            symbol: "exclamationmark.shield",
            title: l10n("connect.quarantine.title"),
            message: l10n("connect.quarantine.body")
        ) {
            VStack(alignment: .leading, spacing: PLSpace.s2) {
                Text(l10n("connect.quarantine.fallback"))
                    .fixedSize(horizontal: false, vertical: true)
                CodeBlock(copyable: ConnectCopyable(
                    kind: .command,
                    text: QuarantineHint.command(for: launchCommand),
                    copyTitle: l10n("mac.actions.copyCommand"),
                    copyAccessibilityLabel: l10n("connect.quarantine.copy")
                ))
            }
        }
    }
}
