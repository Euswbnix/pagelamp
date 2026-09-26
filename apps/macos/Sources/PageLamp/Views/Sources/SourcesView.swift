// Sources & Sync (spec §3.3, M1: list, sync, progress). One section per source with its status,
// live progress, problem callout and Sync; the last run's results until Hide Results; the S17
// busy callout; the short disclosure, always. Add, Replace and Remove are M2.

import SwiftUI
import PageLampKit
import PageLampModel

struct SourcesView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n

    var body: some View {
        ScrollView {
            SourcesPage()
        }
        .scrollEdgeEffectStyle(.soft, for: .bottom)
        .accessoryBar()
        .navigationTitle(l10n("mac.nav.sources"))
        .navigationSubtitle(l10n("sources.description"))
        .toolbar { SourcesToolbar() }
        // The first rejected token/feed's fix outranks Sync All (spec §3.0).
        .primaryActionCandidates(SourcesPage.candidates(model.sourceRows))
    }
}

/// The page's document: rendered in the scroll view and by the snapshot harness.
struct SourcesPage: View {
    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n

    /// The arbiter's candidates on this page, in page order: each source's fix, then Sync All.
    static func candidates(_ rows: [SourceRow]) -> [PrimaryActionCandidate] {
        rows.compactMap { row in
            if case .expired = row.problem { return PrimaryActionCandidate.fixSource(row.id) }
            return nil
        } + [.pagePrimary]
    }

    var body: some View {
        let rows = model.sourceRows
        ReadingPage {
            LampBand(lit: false) {
                PageHeader(title: l10n("mac.nav.sources"), subtitle: l10n("sources.description"))
            }
        } content: {
            ReadingColumn(spacing: PLSpace.s6) {
                if model.externalSyncRunning {
                    BusyCallout()
                }
                if let run = model.lastRun, !model.isSyncing {
                    LastRunBar(run: run)
                }
                if rows.isEmpty {
                    // Add Your First Source arrives with the Add Source sheet (M2).
                    EmptyState(
                        symbol: "folder.badge.plus",
                        title: l10n("sources.empty.title"),
                        message: l10n("sources.empty.description")
                    )
                    .padding(.vertical, PLSpace.s8)
                } else {
                    ForEach(rows) { row in
                        SourceSection(row: row)
                    }
                }
                // The short disclosure, always (spec §3.3).
                Label {
                    Text(l10n("common.disclosure.short"))
                        .paragraphLineSpacing()
                        .fixedSize(horizontal: false, vertical: true)
                } icon: {
                    Image(systemName: "lock.shield")
                        .symbolRenderingMode(.hierarchical)
                }
                .font(PLType.callout.font)
                .foregroundStyle(.secondary)
            }
        }
        .primaryActionCandidates(Self.candidates(rows))
    }
}

/// S17: another process (usually the command line) holds the sync lock. Sync buttons are
/// disabled until Check Again finds it finished.
private struct BusyCallout: View {
    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n
    @State private var checking = false

    var body: some View {
        Callout(
            tone: .info,
            symbol: "hourglass",
            title: l10n("sources.busy.title"),
            message: l10n("common.sync.busy")
        ) {
            Button(l10n("mac.actions.checkAgain")) {
                checking = true
                Task {
                    await model.refresh()
                    checking = false
                }
            }
            .buttonStyle(.bordered)
            .disabled(checking)
        }
    }
}

/// The last finished run: its outcome and time, and Hide Results (each source shows its own
/// part under "Last run").
private struct LastRunBar: View {
    let run: SyncRun
    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n

    var body: some View {
        let ok = run.results.allSatisfy(\.ok)
        HStack(alignment: .firstTextBaseline, spacing: PLSpace.s2) {
            Image(systemName: ok ? "checkmark.circle" : "exclamationmark.triangle")
                .symbolRenderingMode(.hierarchical)
                .foregroundStyle(ok ? PLColor.success : PLColor.warning)
                .accessibilityHidden(true)
            Text(ok ? l10n("common.sync.done") : l10n("common.sync.doneWithErrors"))
                .font(PLType.headline.font)
            Text(verbatim: "·")
                .foregroundStyle(.tertiary)
                .accessibilityHidden(true)
            Text(run.finishedAt.formatted(.dateTime.hour().minute().locale(l10n.locale)))
                .monospacedDigit()
                .foregroundStyle(.secondary)
            Spacer(minLength: PLSpace.s4)
            Button(l10n("mac.actions.hideResults")) { model.hideResults() }
                .buttonStyle(.bordered)
                .controlSize(.small)
                .accessibilityLabel(l10n("sources.progress.dismissLabel"))
        }
        .font(PLType.body.font)
        .accessibilityElement(children: .contain)
    }
}
