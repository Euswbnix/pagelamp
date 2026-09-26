// One source on Sources & Sync (spec §3.3): header with status (glyph + words), live progress,
// problem callout with the fix, non-secret details, the last run, and Sync. Drawn like a grouped
// form section in the content layer (fills and hairlines; never glass). Secrets are never shown
// (rule 3): a feed shows only "Private feed address (stored in your keychain)".

import AppKit
import SwiftUI
import PageLampKit
import PageLampModel

struct SourceSection: View {
    let row: SourceRow
    /// A fix elsewhere led here: an accent outline for a moment (selection colour, not glass),
    /// and VoiceOver moves to the source.
    var highlighted = false

    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @AccessibilityFocusState private var voiceOverFocus: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
                .padding(PLSpace.s4)
            if let progress = row.progress {
                SourceProgressBlock(label: row.source.label, progress: progress, warnings: row.liveWarnings)
                    .padding(.horizontal, PLSpace.s4)
                    .padding(.bottom, PLSpace.s4)
            }
            if let problem = row.problem {
                Divider().padding(.horizontal, PLSpace.s4)
                SourceProblemBlock(row: row, problem: problem)
                    .padding(PLSpace.s4)
            }
            Divider().padding(.horizontal, PLSpace.s4)
            SourceDetails(row: row)
                .padding(PLSpace.s4)
            Divider().padding(.horizontal, PLSpace.s4)
            actions
                .padding(.horizontal, PLSpace.s4)
                .padding(.vertical, PLSpace.s3)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .calloutSurface(.quaternary)
        .overlay {
            RoundedRectangle(cornerRadius: PLRadius.callout)
                .strokeBorder(Color.accentColor, lineWidth: 2)
                .opacity(highlighted ? 1 : 0)
                .accessibilityHidden(true)
        }
        .animation(reduceMotion ? PLMotion.reduced : PLMotion.calm, value: highlighted)
        .accessibilityElement(children: .contain)
        .accessibilityFocused($voiceOverFocus)
        .onChange(of: highlighted, initial: true) { _, highlighted in
            if highlighted { voiceOverFocus = true }
        }
    }

    // MARK: Header

    private var header: some View {
        HStack(alignment: .firstTextBaseline, spacing: PLSpace.s3) {
            Image(systemName: row.source.kind.symbol)
                .symbolRenderingMode(.hierarchical)
                .foregroundStyle(.secondary)
                .font(PLType.title3.font)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: PLSpace.s1) {
                Text(verbatim: row.source.label)
                    .font(PLType.title3.font)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityAddTraits(.isHeader)
                if subtitle != row.source.label {
                    Text(subtitle)
                        .font(PLType.callout.font)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            Spacer(minLength: PLSpace.s4)
            SourceStatusLabel(status: row.status)
        }
    }

    /// "Canvas · Connected as Demo Student", else the kind's name.
    private var subtitle: String {
        let kind = l10n.sourceKind(row.source.kind)
        if row.source.kind == .canvas, let name = row.config.accountName {
            return l10n("sources.card.kindConnectedAs", ["kind": kind, "name": name])
        }
        return kind
    }

    // MARK: Actions

    private var actions: some View {
        HStack(spacing: PLSpace.s2) {
            Button {
                Task { await model.syncSource(row.id) }
            } label: {
                Label(l10n("sources.actions.sync"), systemImage: "arrow.triangle.2.circlepath")
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
            .disabled(!model.canStartSync)
            .help(model.canStartSync ? l10n("sources.actions.syncSource", ["label": row.source.label]) : l10n("common.sync.busy"))
            .accessibilityLabel(l10n("sources.actions.syncSource", ["label": row.source.label]))
            Spacer(minLength: 0)
            // Replace… (not expired) and Remove… arrive with their sheets in M2.
        }
    }
}

/// Status glyph + words (spec §3.3): OK · Not synced yet · Waiting · Syncing · 12 of 40 ·
/// Access expired / Can't connect / … (`common.sourceError.*`).
struct SourceStatusLabel: View {
    let status: SourceStatus
    @Environment(\.l10n) private var l10n
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: PLSpace.s1) {
            Image(systemName: symbol)
                .symbolRenderingMode(.hierarchical)
                .foregroundStyle(glyphStyle)
                .accessibilityHidden(true)
            Text(words)
            if case .syncing(let current?, let total?) = status, total > 0 {
                Text(verbatim: "·")
                    .foregroundStyle(.tertiary)
                    .accessibilityHidden(true)
                Text(l10n("sources.progress.steps", ["current": l10n.number(current), "total": l10n.number(total)]))
                    .monospacedDigit()
                    .contentTransition(reduceMotion ? .identity : .numericText(value: Double(current)))
            }
        }
        .font(PLType.callout.font)
        .fixedSize()
        .accessibilityElement(children: .combine)
        .accessibilityLabel(l10n("sources.status.label") + l10n("common.punctuation.colon") + accessibilityWords)
    }

    private var accessibilityWords: String {
        if case .syncing(let current?, let total?) = status, total > 0 {
            return words + ", " + l10n("sources.progress.steps", ["current": l10n.number(current), "total": l10n.number(total)])
        }
        return words
    }

    private var symbol: String {
        switch status {
        case .ok: "checkmark.circle"
        case .neverSynced: "circle.dashed"
        case .waiting: "hourglass"
        case .syncing: "arrow.triangle.2.circlepath"
        case .failed(.authExpiredOrRevoked): "key"
        case .failed(.network): "wifi.slash"
        case .failed: "exclamationmark.triangle"
        }
    }

    private var glyphStyle: AnyShapeStyle {
        switch status {
        case .ok: AnyShapeStyle(PLColor.success)
        case .neverSynced, .waiting, .syncing: AnyShapeStyle(.secondary)
        case .failed(.authExpiredOrRevoked): AnyShapeStyle(PLColor.danger)
        case .failed: AnyShapeStyle(PLColor.warning)
        }
    }

    private var words: String {
        switch status {
        case .ok: l10n("sources.status.ok")
        case .neverSynced: l10n("common.sync.never")
        case .waiting: l10n("mac.syncDetails.waiting")
        case .syncing: l10n("sources.status.syncing")
        case .failed(let kind): l10n.sourceError(kind)
        }
    }
}


/// The running source's bar, the core's message (English) and its warnings so far.
private struct SourceProgressBlock: View {
    let label: String
    let progress: SourceLiveProgress
    let warnings: [String]
    @Environment(\.l10n) private var l10n

    var body: some View {
        VStack(alignment: .leading, spacing: PLSpace.s2) {
            Group {
                if let fraction = progress.fraction {
                    ProgressView(value: fraction)
                } else {
                    ProgressView()
                        .progressViewStyle(.linear)
                }
            }
            .accessibilityLabel(l10n("sources.progress.barLabel", ["label": label]))
            .accessibilityValue(steps ?? l10n("sources.status.syncing"))
            Group {
                if let message = progress.message {
                    Text.english(message)
                } else {
                    Text(l10n("sources.progress.starting"))
                }
            }
            .font(PLType.callout.font)
            .foregroundStyle(.secondary)
            .lineLimit(2)
            if !warnings.isEmpty {
                WarningsDisclosure(warnings: warnings)
            }
        }
    }

    private var steps: String? {
        guard let current = progress.current, let total = progress.total, total > 0 else { return nil }
        return l10n("sources.progress.steps", ["current": l10n.number(current), "total": l10n.number(total)])
    }
}

/// "Warnings (2)": skipped files and similar, not failures; the core's English, collapsed.
struct WarningsDisclosure: View {
    let warnings: [String]
    @Environment(\.l10n) private var l10n
    @State private var expanded = false

    var body: some View {
        DisclosureGroup(isExpanded: $expanded) {
            VStack(alignment: .leading, spacing: PLSpace.s1) {
                ForEach(warnings, id: \.self) { warning in
                    Text.english(warning)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            .font(PLType.callout.font)
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.top, PLSpace.s1)
        } label: {
            Label {
                Text(l10n("sources.progress.warnings", ["n": l10n.number(warnings.count)]))
            } icon: {
                Image(systemName: "exclamationmark.triangle")
                    .foregroundStyle(PLColor.warning)
            }
            .font(PLType.callout.font)
        }
    }
}

/// The problem callout inside a source (spec §3.3, S7): what went wrong, the core's English
/// message, and the fix. An expired token/feed's fix is Replace Token… / Replace Feed Address…,
/// the arbiter's candidate (2).
private struct SourceProblemBlock: View {
    let row: SourceRow
    let problem: SourceProblem
    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: PLSpace.s3) {
            Image(systemName: expiredFix == nil ? "exclamationmark.triangle" : "key")
                .symbolRenderingMode(.hierarchical)
                .foregroundStyle(expiredFix == nil ? PLColor.warning : PLColor.danger)
                .font(PLType.headline.font)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: PLSpace.s1) {
                Text(expiredFix == nil ? l10n("sources.problem.title") : l10n("sources.problem.expiredTitle"))
                    .font(PLType.headline.font)
                    .fixedSize(horizontal: false, vertical: true)
                if let fix = expiredFix {
                    Text(fix == .replaceFeed ? l10n("sources.problem.expiredFeed") : l10n("sources.problem.expiredCanvas"))
                        .font(PLType.body.font)
                        .paragraphLineSpacing()
                        .fixedSize(horizontal: false, vertical: true)
                }
                if let error = row.source.lastError {
                    Text.english(error)
                        .font(PLType.callout.font)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                        .textSelection(.enabled)
                }
                if let fix = expiredFix {
                    fixButton(fix)
                        .padding(.top, PLSpace.s2)
                }
            }
        }
        .accessibilityElement(children: .contain)
    }

    private var expiredFix: CapsuleState.Attention.Fix? { problem.fix }

    /// Replace Token… / Replace Feed Address…: the arbiter's candidate (2) once this build has
    /// the Replace sheet (M2 presents it from `AppModel.fixSource(_:)`). Until then it stays
    /// where it will live, disabled and plain (`.bordered`, never a candidate: Sync All keeps
    /// the tint), with a help tag saying it comes in the next update — like the busy-disabled
    /// sync buttons (S17). The callout's text says what to do meanwhile.
    @ViewBuilder private func fixButton(_ fix: CapsuleState.Attention.Fix) -> some View {
        let button = Button {
            model.fixSource(row.id)
        } label: {
            Label(l10n.fix(fix), systemImage: "key")
        }
        .accessibilityLabel(
            fix == .replaceFeed
                ? l10n("sources.actions.replaceFeedFor", ["label": row.source.label])
                : l10n("sources.actions.replaceTokenFor", ["label": row.source.label])
        )
        if SourceRow.canReplaceSecrets {
            button.arbitratedButtonStyle(.fixSource(row.id))
        } else {
            button
                .buttonStyle(.bordered)
                .disabled(true)
                .help(l10n("mac.sources.fixUnavailable"))
                .accessibilityHint(l10n("mac.sources.fixUnavailable"))
        }
    }
}

/// Non-secret settings and freshness as label/value rows (`sources.card.*`).
private struct SourceDetails: View {
    let row: SourceRow
    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n
    @Environment(\.openURL) private var openURL

    var body: some View {
        Grid(alignment: .leadingFirstTextBaseline, horizontalSpacing: PLSpace.s4, verticalSpacing: PLSpace.s2) {
            if row.source.kind == .canvas, let url = row.config.baseURL {
                GridRow {
                    label(l10n("sources.card.canvasAddress"))
                    Button {
                        openURL(url)
                    } label: {
                        HStack(alignment: .firstTextBaseline, spacing: PLSpace.s1) {
                            Text(verbatim: url.absoluteString)
                            Image(systemName: "arrow.up.forward")
                                .imageScale(.small)
                                .accessibilityHidden(true)
                        }
                    }
                    .linkButtonStyle()
                    .help(l10n("common.actions.openInBrowser"))
                }
            }
            if row.source.kind == .folder, let path = row.config.path {
                GridRow {
                    label(l10n("sources.card.folderPath"))
                    HStack(alignment: .firstTextBaseline, spacing: PLSpace.s3) {
                        Text(verbatim: path)
                            .font(PLType.mono.font)
                            .textSelection(.enabled)
                            .fixedSize(horizontal: false, vertical: true)
                            .frame(maxWidth: .infinity, alignment: .leading)
                        Button(l10n("mac.actions.showInFinder")) {
                            Links.revealInFinder(path: path)
                        }
                        .buttonStyle(.bordered)
                        .controlSize(.small)
                    }
                }
                if let start = row.config.termStart,
                   let date = IsoDate.date(from: start, calendar: model.calendar) {
                    GridRow {
                        label(l10n("sources.card.termStart"))
                        Text(date.formatted(.dateTime.year().month(.abbreviated).day().locale(l10n.locale)))
                            .monospacedDigit()
                    }
                }
            }
            if row.source.kind == .ical {
                GridRow {
                    label(l10n("sources.card.feed"))
                    Label {
                        Text(l10n("sources.card.feedPrivate"))
                            .fixedSize(horizontal: false, vertical: true)
                    } icon: {
                        Image(systemName: "lock")
                    }
                    .foregroundStyle(.secondary)
                }
            }
            GridRow {
                label(l10n("sources.card.lastSynced"))
                if let synced = row.source.lastSyncedAt {
                    Text(l10n.relative(synced, to: model.clock(), calendar: model.calendar))
                        .monospacedDigit()
                        .help(synced.formatted(.dateTime.locale(l10n.locale)))
                } else {
                    Text(l10n("common.sync.never"))
                        .foregroundStyle(.secondary)
                }
            }
            if let run = row.lastRun {
                GridRow {
                    label(l10n("mac.sources.lastRun"))
                    LastRunValue(run: run, persistedError: row.source.lastError)
                }
            }
        }
        .font(PLType.body.font)
    }

    private func label(_ text: String) -> some View {
        Text(text)
            .foregroundStyle(.secondary)
            .gridColumnAlignment(.leading)
    }
}

/// "Done", or the failure's words with the core's English message; then its warnings.
private struct LastRunValue: View {
    let run: SourceLastRun
    /// The source's own last error, already shown in its problem callout.
    let persistedError: String?
    @Environment(\.l10n) private var l10n

    var body: some View {
        VStack(alignment: .leading, spacing: PLSpace.s1) {
            Label {
                Text(run.ok ? l10n("sources.progress.ok") : l10n.sourceError(run.errorKind ?? .other))
            } icon: {
                Image(systemName: run.ok ? "checkmark.circle" : "exclamationmark.triangle")
                    .foregroundStyle(run.ok ? PLColor.success : PLColor.warning)
            }
            if !run.ok, let error = run.error, error != persistedError {
                Text.english(error)
                    .font(PLType.callout.font)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            if !run.warnings.isEmpty {
                WarningsDisclosure(warnings: run.warnings)
            }
        }
    }
}
