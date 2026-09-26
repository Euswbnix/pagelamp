// The primary-action arbiter (spec §3.0): one tinted action per window, often none. The single
// source of truth for who holds the tint: the status capsule's fix bubble (spec §6.2) or one of
// the page's candidates.

/// A control on the page that may become the window's one tinted (prominent) action.
public enum PrimaryActionCandidate: Hashable, Sendable {
    /// (1) Save AI Policy while the inspector's draft is dirty [M2].
    case saveAIPolicy
    /// (2) The fix for a failing source (Replace Token… / Replace Feed Address…), by source id.
    /// A page lists it only when its button can do something (never a disabled Replace).
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

/// Who holds the window's one tint.
public enum PrimaryActionWinner: Hashable, Sendable {
    /// The status capsule's tinted fix bubble (S8 "attention, split"). While it shows, every
    /// candidate on the page renders `.bordered`.
    case capsuleFix
    /// A candidate on the page renders prominent (`.borderedProminent`; in the toolbar, the
    /// prominent glass style from Chrome/).
    case page(PrimaryActionCandidate)
}

public enum PrimaryActionArbiter {
    /// The page's candidate that renders prominent: the first present by rank; among several
    /// source fixes the first in `present` (the first failing source) wins. Every other
    /// candidate renders `.bordered`.
    public static func winner(_ present: [PrimaryActionCandidate]) -> PrimaryActionCandidate? {
        present.enumerated().min { lhs, rhs in
            lhs.element.rank == rhs.element.rank ? lhs.offset < rhs.offset : lhs.element.rank < rhs.element.rank
        }?.element
    }

    /// Whether the capsule shows its tinted fix bubble: in the attention state, on every page
    /// but Sources & Sync, where the failing source's own callout is the fix surface (and the
    /// row the bubble would lead to is already on screen).
    public static func showsCapsuleFix(capsule: CapsuleState, destination: Destination) -> Bool {
        guard case .attention = capsule else { return false }
        return destination != .sources
    }

    /// The window's one tinted action: the capsule's fix bubble while it shows, else the page's
    /// winner. When the capsule leaves (dismissed, or the source was fixed) the tint goes back
    /// to the page.
    public static func winner(
        page present: [PrimaryActionCandidate],
        capsule: CapsuleState,
        destination: Destination
    ) -> PrimaryActionWinner? {
        if showsCapsuleFix(capsule: capsule, destination: destination) { return .capsuleFix }
        return winner(present).map(PrimaryActionWinner.page)
    }
}

extension AppModel {
    /// Whether the status capsule shows its tinted fix bubble now (spec §6.2, §3.0).
    public var showsCapsuleFix: Bool {
        PrimaryActionArbiter.showsCapsuleFix(capsule: capsule, destination: destination)
    }

    /// The window's tinted action for a page that offers `candidates` (in page order).
    public func primaryActionWinner(for candidates: [PrimaryActionCandidate]) -> PrimaryActionWinner? {
        PrimaryActionArbiter.winner(page: candidates, capsule: capsule, destination: destination)
    }
}
