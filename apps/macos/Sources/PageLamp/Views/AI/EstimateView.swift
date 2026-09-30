// "≈ $x" before Generate (design §3.5, §7; the Tauri app's GenerateButton, without the button:
// each screen puts Generate where its buttons go, enabled by `CostEstimateModel.canGenerate`):
// the cost line, the tokens it may use, and the block the facade reports, with the two that can be
// settled here (go over the budget this time; use a model without a price).

import SwiftUI
import PageLampKit
import PageLampModel

struct EstimateView: View {
    let estimate: CostEstimateModel

    @Environment(\.l10n) private var l10n

    var body: some View {
        VStack(alignment: .leading, spacing: PLSpace.s1) {
            if let line = costLine {
                HStack(spacing: PLSpace.s2) {
                    Text(line.text)
                        .monospacedDigit()
                        .accessibilityLabel(line.spoken)
                    if estimate.loading {
                        ProgressView()
                            .controlSize(.mini)
                            .accessibilityHidden(true)
                    }
                }
                .font(PLType.callout.font)
            }
            if let value = estimate.estimate, estimate.request != nil, let details = l10n.estimateDetails(value) {
                Text(details)
                    .font(PLType.callout.font)
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
            }
            if let failure = estimate.failure {
                Text(l10n.aiError(failure))
                    .font(PLType.callout.font)
                    .foregroundStyle(PLColor.danger)
                    .fixedSize(horizontal: false, vertical: true)
            }
            if estimate.request != nil, let block = estimate.block {
                blockView(block)
            }
        }
    }

    /// The cost line, when the block has an amount to show.
    private var costLine: (text: String, spoken: String)? {
        guard estimate.showsCost, let value = estimate.estimate else { return nil }
        return l10n.estimateLine(value, backendKind: estimate.backendKind)
    }

    @ViewBuilder
    private func blockView(_ block: BlockReason) -> some View {
        switch block {
        case .budgetReached:
            Text(l10n("ai.estimate.overBudget"))
                .font(PLType.callout.font)
                .fixedSize(horizontal: false, vertical: true)
            Toggle(l10n("ai.estimate.overrideBudget"), isOn: Binding(
                get: { estimate.overrideBudget },
                set: { estimate.overrideBudget = $0 }
            ))
            .toggleStyle(.checkbox)
        case .priceUnknownNotAcknowledged:
            if estimate.choice != nil {
                Text(l10n("ai.estimate.useUnpricedHint"))
                    .font(PLType.callout.font)
                    .fixedSize(horizontal: false, vertical: true)
                Button(l10n("ai.estimate.useUnpriced")) {
                    Task { await estimate.acknowledgeUnpriced() }
                }
                .controlSize(.small)
                .disabled(estimate.acknowledging)
                if let failure = estimate.acknowledgeFailure {
                    Text(l10n.aiError(failure))
                        .font(PLType.callout.font)
                        .foregroundStyle(PLColor.danger)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        default:
            Text(l10n("ai.blocked.\(AiCodes.name(block))"))
                .font(PLType.callout.font)
                .fixedSize(horizontal: false, vertical: true)
            if block == .noModelChosen || block == .disclosureNotAcknowledged {
                // Settings ▸ AI ("AI models", like the Tauri app's link).
                SettingsLink {
                    Text(l10n("ai.settings.title"))
                }
                .linkButtonStyle()
            }
        }
    }
}

extension CostEstimateModel {
    /// What Generate's VoiceOver hint says: the cost, then why it's blocked.
    @MainActor
    func spokenHint(_ l10n: L10n) -> String {
        var parts: [String] = []
        if showsCost, let value = estimate, let line = l10n.estimateLine(value, backendKind: backendKind) {
            parts.append(line.spoken)
        }
        if let block, blocked, block != .priceUnknownNotAcknowledged {
            parts.append(block == .budgetReached ? l10n("ai.estimate.overBudget") : l10n("ai.blocked.\(AiCodes.name(block))"))
        } else if block == .priceUnknownNotAcknowledged {
            parts.append(l10n("ai.estimate.useUnpricedHint"))
        }
        return parts.joined(separator: " ")
    }
}
