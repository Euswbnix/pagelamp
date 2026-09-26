// The setup screens in their states (Sources & Sync, Connect, Settings) for the snapshot
// catalogue: expired token, mid-sync, results, busy (S17), empty; each Connect client, S15, the
// error; each Settings tab.
//
// Offscreen limits (ImageRenderer): segmented pickers, progress views and AppKit-backed controls
// draw as yellow placeholders; Settings forms render with a stand-in for `.grouped` (blank
// offscreen), see `SettingsForm`.

import SwiftUI
import PageLampKit
import PageLampModel

enum SetupSnapshots {
    static let pages: [SnapshotPage] = sourcesPages + connectPages + settingsPages

    // MARK: Sources & Sync

    static let sourcesPages: [SnapshotPage] = [
        SnapshotPage(name: "sources-expired", width: SnapshotCatalog.detailWidth, setup: SnapshotSetup(scenario: .expired)) { _ in AnyView(SourcesPage()) },
        SnapshotPage(name: "sources-syncing", width: SnapshotCatalog.detailWidth, setup: SnapshotSetup(scenario: .expired, syncStep: .milliseconds(40))) { model in
            // Mid-run: the folder done (with a warning), the feed syncing, Canvas waiting.
            Task { await model.syncAll() }
            _ = await waitUntil {
                guard let progress = model.syncProgress else { return false }
                return progress.sourceIndex == 2 && (progress.current ?? 0) >= 2
            }
            // Rendered now, mid-run; the renderer waits for the sync before the next page.
            return AnyView(SourcesPage())
        },
        SnapshotPage(name: "sources-results", width: SnapshotCatalog.detailWidth, setup: SnapshotSetup(scenario: .error)) { model in
            // After a finished run: the folder is missing, the others synced (one with a warning).
            await model.syncAll()
            return AnyView(SourcesPage())
        },
        SnapshotPage(name: "sources-busy", width: SnapshotCatalog.detailWidth, setup: SnapshotSetup(scenario: .busy)) { _ in AnyView(SourcesPage()) },
        SnapshotPage(name: "sources-empty", width: SnapshotCatalog.detailWidth, minHeight: 560, setup: SnapshotSetup(scenario: .empty)) { _ in AnyView(SourcesPage()) },
    ]

    // MARK: Connect

    static let connectPages: [SnapshotPage] = [
        // Claude Code is already set up in the mock, so it is preselected.
        SnapshotPage(name: "connect-preselected", width: SnapshotCatalog.detailWidth) { model in
            let connect = await loadConnect(model, client: nil)
            return AnyView(ConnectPage(connect: connect, showsQuarantineHint: true))
        },
        SnapshotPage(name: "connect-claude-desktop", width: SnapshotCatalog.detailWidth) { model in
            let connect = await loadConnect(model, client: .claudeDesktop)
            return AnyView(ConnectPage(connect: connect, showsQuarantineHint: false)
                .environment(\.disclosureAcknowledgedAt, model.clock().addingTimeInterval(-86_400)))
        },
        SnapshotPage(name: "connect-codex", width: SnapshotCatalog.detailWidth) { model in
            let connect = await loadConnect(model, client: .codex)
            return AnyView(ConnectPage(connect: connect, showsQuarantineHint: false))
        },
        SnapshotPage(name: "connect-other", width: SnapshotCatalog.detailWidth) { model in
            let connect = await loadConnect(model, client: .generic)
            return AnyView(ConnectPage(connect: connect, showsQuarantineHint: false))
        },
        SnapshotPage(name: "connect-temporary-location", width: SnapshotCatalog.detailWidth) { model in
            let configs = await configs(model, binary: translocatedBinary)
            let data = ConnectData(
                configs: configs,
                presence: McpClientPresence(claudeDesktop: false, claudeCode: false, codex: false),
                temporaryLocation: .translocated
            )
            return AnyView(ConnectPage(connect: ConnectModel(phase: .loaded(data), selectedClient: nil), showsQuarantineHint: true))
        },
        SnapshotPage(name: "connect-error", width: SnapshotCatalog.detailWidth, minHeight: 900) { _ in
            let failure = PageLampFailure(kind: .internal, message: "mock failure")
            return AnyView(ConnectPage(connect: ConnectModel(phase: .failed(failure), selectedClient: nil), showsQuarantineHint: false))
        },
    ]

    /// Where App Translocation runs a quarantined app from (S15).
    static let translocatedBinary =
        "/private/var/folders/xy/T/AppTranslocation/0A1B2C3D/d/PageLamp Preview.app/Contents/MacOS/pagelamp"

    static func configs(_ model: AppModel, binary: String) async -> [McpClientConfig] {
        do throws(PageLampFailure) {
            return try await model.service.mcpClientConfigs(pagelampBinary: binary)
        } catch {
            return []
        }
    }

    static func loadConnect(_ model: AppModel, client: McpClient?) async -> ConnectModel {
        let connect = ConnectModel(phase: .loading, selectedClient: nil)
        await connect.load(service: model.service, binary: model.sidecarPath, temporaryLocation: nil)
        if let client { connect.selectedClient = client }
        return connect
    }

    // MARK: Settings

    static let settingsPages: [SnapshotPage] = [
        SnapshotPage(name: "settings-general", width: PLSize.settingsWidth, minHeight: 240) { model in
            AnyView(SettingsTabPage(tab: .general, title: model.l10n("mac.settings.tabs.general")))
        },
        // The content speaks the other language than the menus: Reopen Now appears.
        SnapshotPage(name: "settings-general-reopen", width: PLSize.settingsWidth, minHeight: 240, setup: SnapshotSetup(otherSystemLanguage: true)) { model in
            AnyView(SettingsTabPage(tab: .general, title: model.l10n("mac.settings.tabs.general")))
        },
        SnapshotPage(name: "settings-data", width: PLSize.settingsWidth, minHeight: 500) { model in
            AnyView(SettingsTabPage(tab: .data, title: model.l10n("mac.settings.tabs.data")))
        },
        SnapshotPage(name: "settings-data-empty", width: PLSize.settingsWidth, minHeight: 500, setup: SnapshotSetup(scenario: .empty)) { model in
            AnyView(SettingsTabPage(tab: .data, title: model.l10n("mac.settings.tabs.data")))
        },
        SnapshotPage(name: "settings-privacy", width: PLSize.settingsWidth, minHeight: 440) { model in
            AnyView(SettingsTabPage(tab: .privacy, title: model.l10n("mac.settings.tabs.privacy")))
        },
        SnapshotPage(name: "settings-privacy-confirmed", width: PLSize.settingsWidth, minHeight: 440) { model in
            AnyView(SettingsTabPage(tab: .privacy, title: model.l10n("mac.settings.tabs.privacy"))
                .environment(\.disclosureAcknowledgedAt, model.clock().addingTimeInterval(-86_400)))
        },
        SnapshotPage(name: "settings-help", width: PLSize.settingsWidth, minHeight: 460) { model in
            AnyView(SettingsTabPage(tab: .help, title: model.l10n("mac.settings.tabs.help")))
        },
    ]

    private static func waitUntil(timeout: Duration = .seconds(10), _ condition: () -> Bool) async -> Bool {
        let clock = ContinuousClock()
        let deadline = clock.now + timeout
        while !condition() {
            if clock.now > deadline { return false }
            try? await Task.sleep(for: .milliseconds(2))
        }
        return true
    }
}
