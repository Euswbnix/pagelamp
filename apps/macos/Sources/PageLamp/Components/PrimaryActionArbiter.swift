// The primary-action arbiter in views (spec §3.0): one tinted action per window, often none.
// The ranking lives in PageLampModel (`PrimaryActionArbiter.winner`).

import SwiftUI
import PageLampModel

extension EnvironmentValues {
    /// The window's winning candidate; set with `primaryActionCandidates(_:)`.
    @Entry var primaryActionWinner: PrimaryActionCandidate? = nil
}

extension View {
    /// Declares which candidates this page shows, in page order (the first failing source
    /// first). Apply it above the page's buttons and toolbar.
    func primaryActionCandidates(_ candidates: [PrimaryActionCandidate]) -> some View {
        environment(\.primaryActionWinner, PrimaryActionArbiter.winner(candidates))
    }

    /// Styles a content button as `candidate`: `.borderedProminent` if it won the arbiter, else
    /// `.bordered`. (Toolbar buttons use `ProminentToolbarButton` from Chrome/.)
    func arbitratedButtonStyle(_ candidate: PrimaryActionCandidate) -> some View {
        modifier(ArbitratedButtonStyle(candidate: candidate))
    }
}

private struct ArbitratedButtonStyle: ViewModifier {
    var candidate: PrimaryActionCandidate
    @Environment(\.primaryActionWinner) private var winner

    func body(content: Content) -> some View {
        if winner == candidate {
            content.buttonStyle(.borderedProminent)
        } else {
            content.buttonStyle(.bordered)
        }
    }
}
