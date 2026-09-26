// The primary-action arbiter (spec §3.0): one tinted action per window, often none.

/// A control that may become the window's one tinted (prominent) action.
public enum PrimaryActionCandidate: Hashable, Sendable {
    /// (1) Save AI Policy while the inspector's draft is dirty [M2].
    case saveAIPolicy
    /// (2) The fix for a failing source (Replace Token… / Replace Feed Address…), by source id.
    case fixSource(String)
    /// (3) Set Term Dates… when the week is unknown, outside the term or low-confidence.
    case setTermDates
    /// (4) The page's own primary action (Sync All; Add a Source… in S3; Sync Now in S4;
    /// Try Again in S2).
    case pagePrimary

    var rank: Int {
        switch self {
        case .saveAIPolicy: 1
        case .fixSource: 2
        case .setTermDates: 3
        case .pagePrimary: 4
        }
    }
}

public enum PrimaryActionArbiter {
    /// The candidate that renders prominent: the first present by rank; among several source
    /// fixes the first in `present` (the first failing source) wins. Every other candidate
    /// renders `.bordered`.
    public static func winner(_ present: [PrimaryActionCandidate]) -> PrimaryActionCandidate? {
        present.enumerated().min { lhs, rhs in
            lhs.element.rank == rhs.element.rank ? lhs.offset < rhs.offset : lhs.element.rank < rhs.element.rank
        }?.element
    }
}
