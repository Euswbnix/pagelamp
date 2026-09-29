// Settings ▸ Help (spec §3.5 W8d, M1): Copy Diagnostic Report… (always the preview sheet first,
// the same one as Help ▸ Copy Diagnostic Report…; works without the database), Open Logs Folder,
// Report a Problem on GitHub, the file reader's state when something is wrong (v0.3 M0.5), and
// About (tagline, version, licence, website).

import SwiftUI
import PageLampModel

struct SettingsHelpTab: View {
    let settings: SettingsModel

    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n

    var body: some View {
        SettingsForm {
            Section {
                HelpRow(hint: l10n("settings.help.reportHint")) {
                    Button(l10n("mac.actions.copyDiagnosticReport")) {
                        // Always the preview first, on this window (SettingsView presents it).
                        Task { await model.showDiagnosticReport(in: .settings) }
                    }
                }
                HelpRow(hint: l10n("settings.help.logsHint")) {
                    Button(l10n("mac.actions.openLogsFolder")) {
                        Task { await AppActions.openLogsFolder(model) }
                    }
                }
                if BrandLinks.issues != nil {
                    HelpRow(hint: l10n("settings.help.issueHint")) {
                        Button {
                            AppActions.open(BrandLinks.issues)
                        } label: {
                            Label(l10n("mac.actions.reportProblemOnGitHub"), systemImage: "arrow.up.forward.app")
                        }
                    }
                }
            } header: {
                Text(l10n("settings.help.title"))
            } footer: {
                Text(l10n("settings.help.description"))
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }

            if let doctor = settings.doctor, let reader = FileReaderNotice(doctor) {
                Section {
                    if let warningKey = reader.warningKey {
                        Label {
                            Text(l10n(warningKey))
                                .fixedSize(horizontal: false, vertical: true)
                        } icon: {
                            Image(systemName: "exclamationmark.triangle")
                                .foregroundStyle(PLColor.warning)
                        }
                    }
                    if let line = reader.unreadableLine(l10n: l10n) {
                        Text(line)
                            .font(PLType.callout.font)
                            .foregroundStyle(.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
            }

            Section(l10n("settings.about.title")) {
                Text(l10n("mac.about.tagline"))
                LabeledContent(l10n("settings.about.version")) {
                    if let version = settings.version(status: model.status) {
                        Text(verbatim: version)
                            .font(PLType.mono.font)
                            .textSelection(.enabled)
                    } else {
                        Text(l10n("settings.about.unavailable"))
                            .foregroundStyle(.secondary)
                    }
                }
                LabeledContent(l10n("settings.about.license")) {
                    // SPDX identifier of the project licence (not translated).
                    Text(verbatim: "Apache-2.0")
                }
                if let homepage = BrandLinks.homepage {
                    LabeledContent(l10n("settings.about.homepage")) {
                        Button {
                            AppActions.open(homepage)
                        } label: {
                            HStack(alignment: .firstTextBaseline, spacing: PLSpace.s1) {
                                Text(verbatim: (homepage.host() ?? "") + homepage.path())
                                Image(systemName: "arrow.up.forward")
                                    .imageScale(.small)
                                    .accessibilityHidden(true)
                            }
                        }
                        .linkButtonStyle()
                        .help(homepage.absoluteString)
                    }
                }
            }
        }
    }
}

/// A help action: what it does on the leading side, the button trailing.
private struct HelpRow<Action: View>: View {
    let hint: String
    @ViewBuilder var action: Action

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: PLSpace.s4) {
            Text(hint)
                .font(PLType.callout.font)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
            action
                .fixedSize()
        }
    }
}
