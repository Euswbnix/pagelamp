// Sources & Sync (spec §3.3, M1): each source's status, problem and live progress, derived from
// the shell data (`AppModel.sources`, `status`) and the running sync (`syncProgress`, `lastRun`).
// Views only render a `SourceRow`.

import Foundation
import PageLampKit

/// The non-secret settings of a source (`SourceRecord.config`, untyped JSON from the core).
/// The one Swift adapter until the facade types it (spec §13 #6). Never holds a secret.
public struct SourceConfig: Equatable, Sendable {
    /// Canvas: the address the student opens Canvas at.
    public var baseURL: URL?
    /// Folder: the course folder.
    public var path: String?
    /// Folder: the term start the student entered ("YYYY-MM-DD").
    public var termStart: String?
    /// Canvas: the account's display name from when the token was checked (never an email or id).
    public var accountName: String?

    public init(baseURL: URL? = nil, path: String? = nil, termStart: String? = nil, accountName: String? = nil) {
        self.baseURL = baseURL
        self.path = path
        self.termStart = termStart
        self.accountName = accountName
    }

    /// Reads the known string fields; anything missing, empty or malformed is nil.
    public init(json: String) {
        let object = OrderedJSON(parsing: json)
        func string(_ key: String) -> String? {
            guard case .string(let value)? = object?[key] else { return nil }
            let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
            return trimmed.isEmpty ? nil : trimmed
        }
        self.init(
            baseURL: string("base_url").flatMap { text in
                // Only web addresses become links.
                URL(string: text).flatMap { ["http", "https"].contains($0.scheme?.lowercased() ?? "") ? $0 : nil }
            },
            path: string("path"),
            termStart: string("term_start"),
            accountName: string("account_name")
        )
    }
}

/// A source's state in its header: glyph + words, never colour alone.
public enum SourceStatus: Equatable, Sendable {
    case ok
    /// Added but never synced.
    case neverSynced
    /// Part of the running sync, not started yet.
    case waiting
    /// Being synced now ("Syncing · 12 of 40").
    case syncing(current: UInt32?, total: UInt32?)
    /// The last sync of this source failed.
    case failed(SourceErrorKind)
}

/// Why a source needs the student (S7): in its problem callout on Sources & Sync, in This
/// Week's callouts and in a course's header. One reading of `last_error_kind` for every screen.
public enum SourceProblem: Equatable, Sendable {
    /// Its token or feed address was rejected: the fix is to replace it (the arbiter's (2)).
    case expired(CapsuleState.Attention.Fix)
    /// Anything else (folder missing, can't connect, …); the core's English message says more.
    case failed(SourceErrorKind)

    /// The problem of a source whose last sync failed; nil when it is fine.
    public init?(source: SourceRecord?) {
        guard let source, let kind = source.lastErrorKind else { return nil }
        if kind == .authExpiredOrRevoked, source.kind != .folder {
            self = .expired(source.kind == .ical ? .replaceFeed : .replaceToken)
        } else {
            self = .failed(kind)
        }
    }

    /// Replace Token… / Replace Feed Address…, when the student must replace a secret.
    public var fix: CapsuleState.Attention.Fix? {
        if case .expired(let fix) = self { return fix }
        return nil
    }
}

/// The running source's progress line.
public struct SourceLiveProgress: Equatable, Sendable {
    /// The core's latest progress message (English; shown tagged English).
    public var message: String?
    public var current: UInt32?
    public var total: UInt32?
    /// 0…1 when the core reports a total.
    public var fraction: Double?
}

/// What the last finished run did for this source (shown until Hide Results).
public struct SourceLastRun: Equatable, Sendable {
    public var ok: Bool
    public var errorKind: SourceErrorKind?
    /// The core's English error.
    public var error: String?
    /// Distinct warnings (skipped files and similar), in order.
    public var warnings: [String]
}

/// Everything the Sources page shows for one source.
public struct SourceRow: Equatable, Sendable, Identifiable {
    public var source: SourceRecord
    public var config: SourceConfig
    public var status: SourceStatus
    public var problem: SourceProblem?
    /// Only while this source is being synced.
    public var progress: SourceLiveProgress?
    /// Warnings of the running sync so far.
    public var liveWarnings: [String]
    public var lastRun: SourceLastRun?

    public var id: String { source.id }

    /// Canvas and calendar feeds keep a secret (token / feed address) that can be replaced.
    public var hasSecret: Bool { source.kind == .canvas || source.kind == .ical }
}

extension AppModel {
    /// One row per source, in the core's order.
    public var sourceRows: [SourceRow] {
        SourceRow.rows(sources: sources, progress: syncProgress, lastRun: isSyncing ? nil : lastRun)
    }
}

extension SourceRow {
    /// Derives the rows; `progress` is the running sync (nil when idle).
    public static func rows(sources: [SourceRecord], progress: SyncProgress?, lastRun: SyncRun?) -> [SourceRow] {
        sources.map { source in
            SourceRow(
                source: source,
                config: SourceConfig(json: source.config),
                status: status(of: source, sources: sources, progress: progress),
                problem: SourceProblem(source: source),
                progress: progress.flatMap { live(of: source, progress: $0) },
                liveWarnings: unique(progress?.warnings[source.id] ?? []),
                lastRun: lastRun?.result(for: source.id).map { result in
                    SourceLastRun(
                        ok: result.ok,
                        errorKind: result.ok ? nil : (result.errorKind ?? .other),
                        error: result.error,
                        warnings: unique(result.warnings)
                    )
                }
            )
        }
    }

    private static func status(of source: SourceRecord, sources: [SourceRecord], progress: SyncProgress?) -> SourceStatus {
        if let progress {
            if let outcome = progress.outcomes[source.id] {
                return outcome.ok ? .ok : .failed(outcome.errorKind ?? .other)
            }
            if progress.sourceId == source.id {
                return .syncing(current: progress.current, total: progress.total)
            }
            // A Sync All covers every source; a single-source run only the one it started.
            if progress.sourceCount >= sources.count, sources.count > 1 {
                return .waiting
            }
        }
        if let kind = source.lastErrorKind { return .failed(kind) }
        return source.lastSyncedAt == nil ? .neverSynced : .ok
    }

    private static func live(of source: SourceRecord, progress: SyncProgress) -> SourceLiveProgress? {
        guard progress.sourceId == source.id, progress.outcomes[source.id] == nil else { return nil }
        return SourceLiveProgress(
            message: progress.message,
            current: progress.current,
            total: progress.total,
            fraction: progress.fraction
        )
    }

    private static func unique(_ items: [String]) -> [String] {
        var seen = Set<String>()
        return items.filter { seen.insert($0).inserted }
    }
}
