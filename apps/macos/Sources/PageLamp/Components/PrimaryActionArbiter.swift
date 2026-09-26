// The primary-action arbiter in views (spec §3.0): one tinted action per window, often none.
// The ranking, and whether the capsule's fix bubble holds the tint, live in PageLampModel
// (`PrimaryActionArbiter`); views only read the result.

import SwiftUI
import PageLampModel

extension EnvironmentValues {
    /// The window's winner for the page on screen; set with `primaryActionCandidates(_:)`.
    @Entry var primaryActionWinner: PrimaryActionWinner? = nil
}

extension View {
    /// Declares which candidates this page shows, in page order (the first failing source
    /// first). Apply it above the page's buttons and toolbar. While the status capsule shows its
    /// tinted fix bubble, no candidate here wins (spec §6.2: one tinted action per window).
    package func primaryActionCandidates(_ candidates: [PrimaryActionCandidate]) -> some View {
        modifier(PrimaryActionCandidates(candidates: candidates))
    }

    /// Styles a content button as `candidate`: `.borderedProminent` if it holds the window's
    /// tint, else `.bordered`. (Toolbar buttons use `ProminentToolbarButton` from Chrome/.)
    package func arbitratedButtonStyle(_ candidate: PrimaryActionCandidate) -> some View {
        modifier(ArbitratedButtonStyle(candidate: candidate))
    }
}

private struct PrimaryActionCandidates: ViewModifier {
    var candidates: [PrimaryActionCandidate]
    @Environment(AppModel.self) private var model

    func body(content: Content) -> some View {
        content.environment(\.primaryActionWinner, model.primaryActionWinner(for: candidates))
    }
}

private struct ArbitratedButtonStyle: ViewModifier {
    var candidate: PrimaryActionCandidate
    @Environment(\.primaryActionWinner) private var winner

    func body(content: Content) -> some View {
        if winner == .page(candidate) {
            content.buttonStyle(.borderedProminent)
        } else {
            content.buttonStyle(.bordered)
        }
    }
}
