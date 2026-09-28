// Whole-window states (spec §3.9): S1 loading and S2 backend unavailable.

import AppKit
import SwiftUI
import PageLampModel

/// S1: blank for 250 ms, then a redacted placeholder page.
struct LoadingState: View {
    @Environment(\.l10n) private var l10n
    @State private var showsPlaceholder = false

    var body: some View {
        ScrollView {
            if showsPlaceholder {
                ReadingPage {
                    LampBand(lit: false) {
                        PageHeader(title: l10n("common.states.loading"), subtitle: l10n("common.states.loading"))
                    }
                } content: {
                    ReadingColumn {
                        ForEach(0 ..< 3, id: \.self) { _ in
                            SectionHeader(title: l10n("common.states.loading"))
                        }
                    }
                }
                .redacted(reason: .placeholder)
                .accessibilityLabel(l10n("common.states.loading"))
            }
        }
        .task {
            try? await Task.sleep(for: .milliseconds(250))
            showsPlaceholder = true
        }
    }
}

/// S2: PageLamp couldn't open its data. Try Again, and the diagnostic tools, which work without
/// the database.
struct BackendUnavailableView: View {
    let failure: PageLampFailure
    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n

    var body: some View {
        EmptyState(
            symbol: "externaldrive.badge.exclamationmark",
            title: l10n("common.states.backendUnavailableTitle"),
            message: failure.kind == .schema
                ? failure.localizedDescription(in: l10n)
                : l10n("common.states.backendUnavailableBody")
        ) {
            HStack {
                Button(l10n("mac.actions.tryAgain")) {
                    Task {
                        if model.dataMode == .live {
                            await model.useLive()
                        } else {
                            await model.refresh()
                        }
                    }
                }
                .arbitratedButtonStyle(.pagePrimary)
                Button(l10n("mac.actions.copyDiagnosticReport")) {
                    Task { await model.showDiagnosticReport(in: .main) }
                }
                .buttonStyle(.bordered)
                Button(l10n("mac.actions.openLogsFolder")) {
                    Task { await AppActions.openLogsFolder(model) }
                }
                .buttonStyle(.bordered)
            }
        }
        .primaryActionCandidates([.pagePrimary])
    }
}

/// Actions shared by menus and pages.
enum AppActions {
    /// Help ▸ Open Logs Folder (works in S2 through the core's diagnostics).
    static func openLogsFolder(_ model: AppModel) async {
        do throws(PageLampFailure) {
            let path = try await model.service.logsDir()
            NSWorkspace.shared.open(URL(filePath: path, directoryHint: .isDirectory))
        } catch {
            NSSound.beep()
        }
    }

    static func open(_ url: URL?) {
        guard let url else { return }
        NSWorkspace.shared.open(url)
    }

    /// PageLamp ▸ About (spec §3.5): the standard panel with the core's version
    /// (`status().version`, else `doctor().version`, which works without the database), the
    /// tagline, the licence and the website. Menus speak the launch language, so does the panel.
    static func showAbout(_ model: AppModel) async {
        var version = model.status?.version
        if version == nil {
            do throws(PageLampFailure) {
                version = try await model.service.doctor().version
            } catch {
                version = nil
            }
        }
        var options: [NSApplication.AboutPanelOptionKey: Any] = [
            .credits: aboutCredits(model.menuL10n),
        ]
        if let version {
            options[.applicationVersion] = version
        }
        NSApp.orderFrontStandardAboutPanel(options: options)
    }

    /// "A reading lamp for your courses." · "Apache-2.0" · the website, centred like the panel.
    static func aboutCredits(_ l10n: L10n) -> NSAttributedString {
        let paragraph = NSMutableParagraphStyle()
        paragraph.alignment = .center
        let body: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: NSFont.smallSystemFontSize),
            .foregroundColor: NSColor.labelColor,
            .paragraphStyle: paragraph,
        ]
        let credits = NSMutableAttributedString(string: l10n("mac.about.tagline") + "\n", attributes: body)
        // The SPDX identifier of the project licence (not translated).
        credits.append(NSAttributedString(string: l10n("settings.about.license") + l10n("common.punctuation.colon") + "Apache-2.0", attributes: body))
        if let homepage = BrandLinks.homepage {
            var link = body
            link[.link] = homepage
            credits.append(NSAttributedString(string: "\n", attributes: body))
            credits.append(NSAttributedString(string: (homepage.host() ?? "") + homepage.path(), attributes: link))
        }
        return credits
    }
}
