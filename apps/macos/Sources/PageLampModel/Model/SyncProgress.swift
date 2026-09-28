// Live sync progress and the last run's results (Sources & Sync, the capsule's details popover).

import Foundation
import PageLampKit

/// Everything the running sync has reported so far, fed by `SyncEvent`s.
public struct SyncProgress: Equatable, Sendable {
    /// How one source's run ended.
    public struct Outcome: Equatable, Sendable {
        public var ok: Bool
        /// The core's English message (shown tagged English, like Tauri).
        public var error: String?
        public var errorKind: SourceErrorKind?
    }

    /// The sources this run covers (a single-source sync has one).
    public var sourceCount: Int
    /// 1-based index of the running source; 0 before the first starts.
    public var sourceIndex = 0
    public var sourceId: String?
    public var sourceLabel: String?
    /// The core's latest progress line (English, e.g. "Indexing materials (3/9)").
    public var message: String?
    public var current: UInt32?
    public var total: UInt32?
    public var warnings: [String: [String]] = [:]
    public var outcomes: [String: Outcome] = [:]

    public init(sourceCount: Int) {
        self.sourceCount = sourceCount
    }

    /// Progress of the running source, 0…1, when the core reports a total.
    public var fraction: Double? {
        guard let current, let total, total > 0 else { return nil }
        return min(1, Double(current) / Double(total))
    }

    public mutating func apply(_ event: SyncEvent) {
        switch event {
        case .sourceStarted(let sourceId, let label):
            sourceIndex += 1
            self.sourceId = sourceId
            sourceLabel = label
            message = nil
            current = nil
            total = nil
        case .progress(let sourceId, let message, let current, let total):
            self.sourceId = sourceId
            self.message = message
            self.current = current
            self.total = total
        case .warning(let sourceId, let message):
            warnings[sourceId, default: []].append(message)
        case .sourceFinished(let sourceId, let ok, let error, let errorKind):
            outcomes[sourceId] = Outcome(ok: ok, error: error, errorKind: errorKind)
        }
    }

    /// The capsule's view of this progress.
    public var capsule: CapsuleState.Syncing {
        CapsuleState.Syncing(
            sourceIndex: sourceIndex,
            sourceCount: sourceCount,
            sourceLabel: sourceLabel,
            fraction: fraction
        )
    }
}

/// The last finished run, shown per source ("Last run") until Hide Results.
public struct SyncRun: Equatable, Sendable {
    public var finishedAt: Date
    public var results: [SourceSyncResult]

    public init(finishedAt: Date, results: [SourceSyncResult]) {
        self.finishedAt = finishedAt
        self.results = results
    }

    public func result(for sourceId: String) -> SourceSyncResult? {
        results.first { $0.sourceId == sourceId }
    }
}
