// The status capsule (spec §6.2, S8): the transient event channel of the current page. M1 states:
// syncing, finished, attention (plus a short failure notice); the M2 fused notices and the
// union/split animation come later. Chrome/ is the only folder that may use glass (spec §1.3).

import SwiftUI
import PageLampKit
import PageLampModel

struct StatusCapsule: View {
    let state: CapsuleState
    var ns: Namespace.ID

    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.appearsActive) private var appearsActive
    @State private var showsDetails = false

    var body: some View {
        // Resting gap 14 > container spacing 12: the two shapes stay separate (no union in M1).
        HStack(spacing: PLLayout.accessoryGap) {
            if state.isVisible {
                HStack(spacing: PLSpace.s2) {
                    Button {
                        showsDetails.toggle()
                    } label: {
                        CapsuleLabel(state: state)
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel(l10n("mac.a11y.capsuleLabel", ["status": CapsuleLabel.text(for: state, l10n: l10n)]))
                    .accessibilityHint(l10n("mac.a11y.capsuleHint"))
                    .accessibilityAddTraits(.updatesFrequently)
                    if case .attention = state {
                        // Inline ×; Esc does the same (spec §7.1).
                        Button {
                            model.dismissAttention()
                        } label: {
                            Image(systemName: "xmark")
                                .imageScale(.small)
                                .foregroundStyle(.secondary)
                        }
                        .buttonStyle(.plain)
                        .keyboardShortcut(.cancelAction)
                        .help(l10n("mac.actions.dismiss"))
                        .accessibilityLabel(l10n("mac.actions.dismiss"))
                    }
                }
                .padding(.horizontal, PLSpace.s4)
                .frame(height: PLSize.accessoryHeight)
                .glassEffect(.regular.interactive(), in: .capsule)
                .glassEffectID("status", in: ns)
                .glassEffectTransition(reduceMotion ? .identity : .materialize)
                .popover(isPresented: $showsDetails, arrowEdge: .top) {
                    SyncDetailsPopover()
                        .pageLampEnvironment(model)
                }
            }
            if case .attention(let attention) = state {
                // The one tinted control; never in a union (spec §6.2).
                Button {
                    model.performCapsuleFix()
                } label: {
                    Text(l10n.fix(attention.fix))
                        .fontWeight(.semibold)
                        .foregroundStyle(.white)
                        .padding(.horizontal, PLSpace.s4)
                        .frame(height: PLSize.accessoryHeight)
                }
                .buttonStyle(.plain)
                .glassEffect(.regular.tint(.accentColor).interactive(), in: .capsule)
                .glassEffectID("status.fix", in: ns)
                .glassEffectTransition(reduceMotion ? .identity : .materialize)
            }
        }
        .frame(maxWidth: PLLayout.capsuleMax)
        .opacity(appearsActive ? 1 : 0.6)
        .animation(reduceMotion ? PLMotion.reduced : PLMotion.quick, value: state)
        .onChange(of: state) { old, new in announce(from: old, to: new) }
    }

    /// VoiceOver hears start, end and new problems only (spec §7.1).
    private func announce(from old: CapsuleState, to new: CapsuleState) {
        let message: String? = switch (old, new) {
        case (.syncing, .syncing): nil
        case (_, .syncing): l10n("mac.a11y.syncStarted")
        case (_, .finished), (_, .failed): CapsuleLabel.text(for: new, l10n: l10n)
        case (.attention(let before), .attention(let after)) where before == after: nil
        case (_, .attention): CapsuleLabel.text(for: new, l10n: l10n)
        case (_, .hidden): nil
        }
        if let message {
            AccessibilityNotification.Announcement(message).post()
        }
    }
}

/// Glyph + words for a capsule state.
struct CapsuleLabel: View {
    let state: CapsuleState
    @Environment(\.l10n) private var l10n
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        HStack(spacing: PLSpace.s2) {
            glyph
            Text(Self.text(for: state, l10n: l10n))
                .monospacedDigit()
                .lineLimit(1)
                .contentTransition(reduceMotion ? .identity : .numericText())
        }
        .font(PLType.body.font)
    }

    @ViewBuilder private var glyph: some View {
        switch state {
        case .syncing(let syncing):
            if let fraction = syncing.fraction {
                ProgressView(value: fraction)
                    .progressViewStyle(.circular)
                    .controlSize(.mini)
            } else {
                ProgressView()
                    .controlSize(.mini)
            }
        case .finished(let finished):
            Image(systemName: finished.problems == 0 ? "checkmark.circle.fill" : "exclamationmark.triangle")
                .foregroundStyle(finished.problems == 0 ? PLColor.success : PLColor.warning)
        case .attention:
            Image(systemName: "key")
                .foregroundStyle(PLColor.danger)
        case .failed(let kind):
            Image(systemName: kind == .network ? "wifi.slash" : kind == .busy ? "circle.dotted" : "exclamationmark.triangle")
                .foregroundStyle(kind == .busy ? AnyShapeStyle(.secondary) : AnyShapeStyle(PLColor.warning))
        case .hidden:
            EmptyView()
        }
    }

    static func text(for state: CapsuleState, l10n: L10n) -> String {
        switch state {
        case .hidden:
            ""
        case .syncing(let syncing):
            if let label = syncing.sourceLabel {
                l10n("mac.capsule.syncing", [
                    "done": l10n.number(max(syncing.sourceIndex, 1)),
                    "total": l10n.number(syncing.sourceCount),
                    "source": label,
                ])
            } else {
                l10n("common.sync.syncing")
            }
        case .finished(let finished):
            finished.problems == 0 ? l10n("common.sync.done") : l10n("common.sync.doneWithErrors")
        case .attention(let attention):
            attention.fix == .replaceFeed
                ? l10n("mac.capsule.needsFeed", ["source": attention.sourceLabel])
                : l10n("mac.capsule.needsToken", ["source": attention.sourceLabel])
        case .failed(let kind):
            switch kind {
            case .busy: l10n("mac.capsule.busy")
            case .network: l10n("mac.capsule.offline")
            default: l10n("common.sync.failed")
            }
        }
    }
}
