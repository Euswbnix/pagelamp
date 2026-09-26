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

    /// Loads the configs for the bundled CLI at `binary`, then which clients are already set up
    /// (a failing `doctor()` only loses the "Set up" marks). Keeps the student's selection.
    public func load(service: any PageLampService, binary: String, temporaryLocation: TemporaryLocation? = nil) async {
        if data == nil { phase = .loading }
        let configs: [McpClientConfig]
        do throws(PageLampFailure) {
            configs = try await service.mcpClientConfigs(pagelampBinary: binary)
        } catch {
            phase = .failed(error)
            return
        }
        var presence: McpClientPresence?
        do throws(PageLampFailure) {
            presence = try await service.doctor().mcpClients
        } catch {
            presence = nil
        }
        let data = ConnectData(configs: configs, presence: presence, temporaryLocation: temporaryLocation)
        phase = .loaded(data)
        if selectedClient == nil || !data.configs.contains(where: { $0.client == selectedClient }) {
            selectedClient = ConnectSetup.preselected(data.configs, presence: presence)
        }
    }
}
