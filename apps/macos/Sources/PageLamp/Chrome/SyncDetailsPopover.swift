// The capsule's details (spec §6.2): per-source progress of the running or last sync. A popover:
// the system draws its glass; its contents are plain rows (no glass on glass).

import SwiftUI
import PageLampKit
import PageLampModel

struct SyncDetailsPopover: View {
    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n

    var body: some View {
        VStack(alignment: .leading, spacing: PLSpace.s2) {
            ForEach(model.sources, id: \.id) { source in
                HStack(alignment: .firstTextBaseline, spacing: PLSpace.s3) {
                    Image(systemName: source.kind.symbol)
                        .symbolRenderingMode(.hierarchical)
                        .foregroundStyle(.secondary)
                        .accessibilityHidden(true)
                    Text(source.label)
                        .lineLimit(1)
                    Spacer(minLength: PLSpace.s4)
                    Text(state(for: source))
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                }
                .accessibilityElement(children: .combine)
            }
        }
        .font(PLType.body.font)
        .padding(PLSpace.s4)
        .frame(minWidth: 300)
    }

    private func state(for source: SourceRecord) -> String {
        if let progress = model.syncProgress {
            if let outcome = progress.outcomes[source.id] {
                return outcome.ok ? l10n("mac.syncDetails.done") : failure(outcome.errorKind)
            }
            if progress.sourceId == source.id {
                if let current = progress.current, let total = progress.total {
                    return l10n("mac.syncDetails.syncing") + " · "
                        + l10n("mac.syncDetails.progress", ["current": l10n.number(current), "total": l10n.number(total)])
                }
                return l10n("mac.syncDetails.syncing")
            }
            return l10n("mac.syncDetails.waiting")
        }
        if let result = model.lastRun?.result(for: source.id) {
            return result.ok ? l10n("mac.syncDetails.done") : failure(result.errorKind)
        }
        if let kind = source.lastErrorKind {
            return failure(kind)
        }
        return source.lastSyncedAt.map { l10n("common.sync.syncedAgo", ["when": l10n.relative($0, to: model.clock())]) }
            ?? l10n("common.sync.never")
    }

    private func failure(_ kind: SourceErrorKind?) -> String {
        kind.map { l10n.sourceError($0) } ?? l10n("mac.syncDetails.failed")
    }
}
