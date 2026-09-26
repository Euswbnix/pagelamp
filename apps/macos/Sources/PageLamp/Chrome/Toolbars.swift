// Toolbars per screen (spec §2.5, M1 items). The system draws the toolbar's glass; the only
// custom glass here is Sync All's `.glassProminent` when it wins the arbiter (spec §3.0, §4.1).
// Every item is also a menu command (PageLampCommands).

import SwiftUI
import PageLampModel

/// Sync Now (⌘R in the File menu): untinted, icon-only, labelled. Disabled while any sync runs,
/// here or in another process (S17), with the reason as its help.
struct SyncNowButton: View {
    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n

    var body: some View {
        Button {
            Task { await model.syncAll() }
        } label: {
            Label(l10n("mac.actions.syncNow"), systemImage: "arrow.triangle.2.circlepath")
        }
        .disabled(!model.canSync)
        .help(model.externalSyncRunning ? l10n("common.sync.busy") : l10n("mac.actions.syncNow"))
    }
}

/// This Week: Sync Now.
struct ThisWeekToolbar: ToolbarContent {
    var body: some ToolbarContent {
        ToolbarItem(placement: .primaryAction) {
            SyncNowButton()
        }
    }
}

/// A course: ‹ › (This Week section only) and "This Week" when away from now; Open Course
/// Website and the inspector toggle on the trailing side. (Download Files… is M2.)
struct CourseToolbar: ToolbarContent {
    let courseId: String
    /// The course website; only a web address (http/https) gets the toolbar item.
    let website: URL?
    /// The detail column is narrower than 900 pt (drops the website item on macOS 26.0).
    let compact: Bool

    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n
    @Environment(\.openURL) private var openURL

    var body: some ToolbarContent {
        let ui = model.ui(for: courseId)
        if ui.section == .week {
            ToolbarItem(placement: .navigation) {
                ControlGroup {
                    Button {
                        model.stepWeek(by: -1)
                    } label: {
                        Label(l10n("mac.actions.previousWeek"), systemImage: "chevron.backward")
                    }
                    .disabled(ui.previousWeek == nil)
                    .help(l10n("mac.actions.previousWeek"))
                    Button {
                        model.stepWeek(by: 1)
                    } label: {
                        Label(l10n("mac.actions.nextWeek"), systemImage: "chevron.forward")
                    }
                    .disabled(ui.nextWeek == nil)
                    .help(l10n("mac.actions.nextWeek"))
                }
            }
        }
        if ui.section == .week, ui.isAwayFromDefault {
            // Back to now; with the current week unknown, back to "Recent materials".
            let title = ui.currentWeek == nil
                ? l10n("mac.toolbar.showRecentMaterials")
                : l10n("mac.toolbar.backToCurrentWeek")
            ToolbarItem(placement: .navigation) {
                Button {
                    model.showCurrentWeek()
                } label: {
                    Label(title, systemImage: "arrow.uturn.backward")
                }
                .help(ui.currentWeek == nil ? title : l10n("mac.actions.currentWeek"))
            }
        }
        if let website, ["http", "https"].contains(website.scheme?.lowercased() ?? "") {
            if #available(macOS 26.1, *) {
                ToolbarItem(placement: .primaryAction) {
                    websiteButton(website)
                }
                .visibilityPriority(.low)
            } else if !compact {
                ToolbarItem(placement: .primaryAction) {
                    websiteButton(website)
                }
            }
        }
        ToolbarSpacer(.fixed, placement: .primaryAction)
        ToolbarItem(placement: .primaryAction) {
            Button {
                model.inspectorShown.toggle()
            } label: {
                Label(l10n("mac.toolbar.inspector"), systemImage: "sidebar.trailing")
            }
            .help(l10n("mac.toolbar.inspector"))
        }
    }

    private func websiteButton(_ url: URL) -> some View {
        Button {
            openURL(url)
        } label: {
            Label(l10n("mac.actions.openCourseWebsite"), systemImage: "safari")
        }
        .help(l10n("mac.actions.openCourseWebsite"))
    }
}

/// Sources & Sync: Sync All, prominent (glass) only when it wins the arbiter. (Add Source… is M2.)
struct SourcesToolbar: ToolbarContent {
    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n
    @Environment(\.primaryActionWinner) private var winner

    var body: some ToolbarContent {
        let prominent = winner == .page(.pagePrimary)
        ToolbarItem(placement: .primaryAction) {
            ProminentToolbarButton(
                title: l10n("mac.actions.syncAll"),
                systemImage: "arrow.triangle.2.circlepath",
                prominent: prominent
            ) {
                Task { await model.syncAll() }
            }
            .disabled(!model.canSync)
            .help(model.externalSyncRunning ? l10n("common.sync.busy") : l10n("mac.actions.syncAll"))
        }
        .sharedBackgroundVisibility(prominent ? .hidden : .automatic)
    }
}

/// A text-and-icon toolbar button that is `.glassProminent` when it is the window's one tinted
/// action, else a default toolbar button. Pair with `.sharedBackgroundVisibility(.hidden)` on
/// its ToolbarItem when prominent, so text and icon-only items never share a background.
struct ProminentToolbarButton: View {
    var title: String
    var systemImage: String
    var prominent: Bool
    var action: () -> Void

    var body: some View {
        if prominent {
            Button(action: action) {
                Label(title, systemImage: systemImage)
                    .labelStyle(.titleAndIcon)
            }
            .buttonStyle(.glassProminent)
        } else {
            Button(action: action) {
                Label(title, systemImage: systemImage)
                    .labelStyle(.titleAndIcon)
            }
        }
    }
}
