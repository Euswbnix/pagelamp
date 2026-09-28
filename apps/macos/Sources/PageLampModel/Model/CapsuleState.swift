// The status capsule's states (spec §6.2, S8). M1: syncing, finished, attention; plus a short
// failure notice (the M2 fused notices with Try Again / Check Again come later).

import PageLampKit

/// What the bottom status capsule shows. `.hidden` when idle: the sidebar footer then carries
/// the persistent state ("Synced 2 h ago").
public enum CapsuleState: Equatable, Sendable {
    case hidden
    /// "◔ Syncing 2 of 3 · Canvas"
    case syncing(Syncing)
    /// "✓ Sync finished" for a few seconds (or "Sync finished with problems").
    case finished(Finished)
    /// A fix the student must make: "Demo Canvas needs a new token" + the tinted fix bubble.
    case attention(Attention)
    /// The sync call itself failed (busy, offline, …); shown for a few seconds.
    case failed(PageLampFailure.Kind)

    public struct Syncing: Equatable, Sendable {
        /// 1-based index of the source being synced (0 before the first starts).
        public var sourceIndex: Int
        public var sourceCount: Int
        public var sourceLabel: String?
        /// Progress of the current source, 0…1, when the core reports a total.
        public var fraction: Double?

        public init(sourceIndex: Int, sourceCount: Int, sourceLabel: String? = nil, fraction: Double? = nil) {
            self.sourceIndex = sourceIndex
            self.sourceCount = sourceCount
            self.sourceLabel = sourceLabel
            self.fraction = fraction
        }
    }

    public struct Finished: Equatable, Sendable {
        /// Sources that failed for a reason the student can't fix here (network, …).
        public var problems: Int

        public init(problems: Int) {
            self.problems = problems
        }
    }

    public struct Attention: Equatable, Sendable {
        public enum Fix: Equatable, Sendable {
            case replaceToken
            case replaceFeed
        }

        public var sourceId: String
        public var sourceLabel: String
        public var fix: Fix

        public init(sourceId: String, sourceLabel: String, fix: Fix) {
            self.sourceId = sourceId
            self.sourceLabel = sourceLabel
            self.fix = fix
        }

        /// The attention a source with this error needs, if any (only a rejected secret has a
        /// fix the student makes in PageLamp).
        public static func forSource(_ source: SourceRecord) -> Attention? {
            guard let fix = SourceProblem(source: source)?.fix else { return nil }
            return Attention(sourceId: source.id, sourceLabel: source.label, fix: fix)
        }
    }

    public var isVisible: Bool { self != .hidden }
}
