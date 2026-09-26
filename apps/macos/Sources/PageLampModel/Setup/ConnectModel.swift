// The Connect page's data (spec §2.8: `mcp_client_configs(url)`, `mcp_launch(url)`,
// `doctor().mcp_clients`). Read-only: PageLamp never writes an AI app's config.

import Foundation
import Observation
import PageLampKit

/// What the Connect page shows once loaded.
public struct ConnectData: Equatable, Sendable {
    /// In picker order (`ConnectSnippets.sorted`).
    public var configs: [McpClientConfig]
    /// Which clients already have a PageLamp entry (nil if `doctor()` failed; nothing is marked).
    public var presence: McpClientPresence?
    /// Set when PageLamp runs from a place that won't last (S15).
    public var temporaryLocation: TemporaryLocation?

    public init(configs: [McpClientConfig], presence: McpClientPresence?, temporaryLocation: TemporaryLocation?) {
        self.configs = ConnectSnippets.sorted(configs)
        self.presence = presence
        self.temporaryLocation = temporaryLocation ?? ConnectSnippets.temporaryLocation(configs)
    }

    /// The command every AI app launches (the bundled CLI).
    public var launchCommand: String? {
        configs.first?.launch.command
    }
}

@Observable @MainActor
public final class ConnectModel {
    public enum Phase: Equatable, Sendable {
        case loading
        case loaded(ConnectData)
        case failed(PageLampFailure)
    }

    public private(set) var phase: Phase
    /// The client whose steps are shown (preselected on the first load).
    public var selectedClient: McpClient?
    /// A reload failed while the page had content: the content stays, with this inline error.
    public private(set) var reloadFailure: PageLampFailure?
    /// Explicit reloads asked for (Try Again): part of the page's load key, with the data mode
    /// and the temporary location. Nothing else reloads the page (not every app refresh: the
    /// configs only change with those, and `doctor()` reads the keychain).
    public private(set) var attempt = 0
    /// The data source the page's content came from.
    @ObservationIgnored private var loadedDataMode: DataMode?
    /// How many loads started (only the latest applies its result).
    @ObservationIgnored private var loads = 0

    public init(phase: Phase = .loading, selectedClient: McpClient? = nil) {
        self.phase = phase
        self.selectedClient = selectedClient
        if selectedClient == nil, case .loaded(let data) = phase {
            self.selectedClient = ConnectSetup.preselected(data.configs, presence: data.presence)
        }
    }

    public var data: ConnectData? {
        if case .loaded(let data) = phase { return data }
        return nil
    }

    /// The selected client's config (the first one if the selection is gone).
    public var selectedConfig: McpClientConfig? {
        guard let data else { return nil }
        return data.configs.first { $0.client == selectedClient } ?? data.configs.first
    }

    /// Try Again: the page reloads (`attempt` is part of its load key).
    public func retry() {
        attempt += 1
    }

    /// Loads the configs for the bundled CLI at `binary`, then which clients are already set up
    /// (a failing `doctor()` only loses the "Set up" marks). Keeps the student's selection. A
    /// failure keeps what the page already shows and reports it in `reloadFailure`, unless the
    /// data source changed (`dataMode`): the other source's steps would be wrong there, so the
    /// page starts over. When loads overlap, only the latest to start applies its result.
    public func load(
        service: any PageLampService,
        binary: String,
        temporaryLocation: TemporaryLocation? = nil,
        dataMode: DataMode? = nil
    ) async {
        loads += 1
        let load = loads
        if let dataMode, dataMode != loadedDataMode {
            if loadedDataMode != nil {
                phase = .loading
                reloadFailure = nil
            }
            loadedDataMode = dataMode
        }
        if data == nil { phase = .loading }
        let configs: [McpClientConfig]
        do throws(PageLampFailure) {
            configs = try await service.mcpClientConfigs(pagelampBinary: binary)
        } catch {
            guard load == loads else { return }
            if data != nil {
                reloadFailure = error
            } else {
                phase = .failed(error)
            }
            return
        }
        var presence: McpClientPresence?
        do throws(PageLampFailure) {
            presence = try await service.doctor().mcpClients
        } catch {
            presence = nil
        }
        guard load == loads else { return }
        let data = ConnectData(configs: configs, presence: presence, temporaryLocation: temporaryLocation)
        phase = .loaded(data)
        reloadFailure = nil
        if selectedClient == nil || !data.configs.contains(where: { $0.client == selectedClient }) {
            selectedClient = ConnectSetup.preselected(data.configs, presence: presence)
        }
    }
}
