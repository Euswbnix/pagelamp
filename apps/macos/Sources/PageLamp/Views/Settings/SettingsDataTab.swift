// Settings ▸ Data (spec §3.5, M1; `settings.data.*`): where the data lives (Copy Path, Show in
// Finder) and what's stored, from `status()` (or `doctor()` when the data can't be opened, S2).

import AppKit
import SwiftUI
import PageLampModel

struct SettingsDataTab: View {
    let settings: SettingsModel

    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        let dataDir = model.status?.dataDir ?? settings.doctor?.dataDir
        let counts = model.status.map { StoredCount.rows($0.counts) } ?? settings.doctor.map(StoredCount.rows) ?? []
        SettingsForm {
            Section {
                if let dataDir {
                    SettingsPathRow(
                        label: l10n("settings.data.folder"),
                        path: dataDir,
                        copyAccessibilityLabel: l10n("settings.data.copyFolder")
                    )
                }
                if let dbPath = model.status?.dbPath {
                    SettingsPathRow(
                        label: l10n("settings.data.database"),
                        path: dbPath,
                        copyAccessibilityLabel: l10n("settings.data.copyDatabase")
                    )
                }
                if dataDir == nil {
                    Text(l10n("settings.about.unavailable"))
                        .foregroundStyle(.secondary)
                }
            } header: {
                Text(l10n("settings.data.title"))
            } footer: {
                Text(l10n("settings.data.description"))
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }

            if !counts.isEmpty {
                Section(l10n("settings.data.countsTitle")) {
                    ForEach(counts) { count in
                        LabeledContent(l10n(count.labelKey)) {
                            Text(l10n.number(count.value))
                                .monospacedDigit()
                                .font(PLType.mono.font)
                        }
                    }
                }
            }

            if model.status != nil, model.sources.isEmpty {
                Section {
                    HStack(alignment: .firstTextBaseline, spacing: PLSpace.s4) {
                        Text(l10n("settings.data.nothingYet"))
                            .foregroundStyle(.secondary)
                            .frame(maxWidth: .infinity, alignment: .leading)
                        Button(l10n("mac.actions.openSourcesAndSync")) {
                            model.destination = .sources
                            openWindow(id: "main")
                        }
                    }
                }
            }
        }
    }
}

/// A path in SF Mono (selectable) with Copy Path and Show in Finder.
struct SettingsPathRow: View {
    let label: String
    let path: String
    let copyAccessibilityLabel: String

    @Environment(\.l10n) private var l10n

    var body: some View {
        VStack(alignment: .leading, spacing: PLSpace.s2) {
            Text(label)
            Text(verbatim: path)
                .font(PLType.mono.font)
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
            HStack(spacing: PLSpace.s2) {
                CopyButton(title: l10n("mac.actions.copyPath"), text: path, accessibilityLabel: copyAccessibilityLabel)
                Button(l10n("mac.actions.showInFinder")) {
                    Links.revealInFinder(path: path)
                }
                .buttonStyle(.bordered)
            }
            .controlSize(.small)
        }
        .accessibilityElement(children: .contain)
    }
}
