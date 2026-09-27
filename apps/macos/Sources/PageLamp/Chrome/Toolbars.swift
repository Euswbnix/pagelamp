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

/// The window's one toolbar (spec §2.5), for every page: each item is always there and only
/// hidden where it doesn't apply. Adding and removing items on every page, section or week
/// switch made AppKit rebuild and re-tile the toolbar (with the inspector's split view, about
/// 100 ms per course page); hiding keeps its structure.
///
/// - This Week: Sync Now.
/// - A course: ‹ › and "This Week" when away from now, disabled outside the This Week section
///   (hidden, they re-tiled the toolbar on every section switch, mid-way through the picker's
///   thumb slide); Open Course Website and the inspector toggle on the trailing side. (Download
///   Files… is M2.)
/// - Sources & Sync: Sync All, prominent (glass) only when it wins the arbiter. (Add Source… is M2.)
struct WindowToolbar: ToolbarContent {
    /// Below this detail-column width a course drops the website item on macOS 26.0.
    nonisolated static let compactBelow: CGFloat = 900

    /// The detail column is narrower than `compactBelow`.
    let compact: Bool

    @Environment(AppModel.self) private var model

    // Each item's content is a parameterless view that reads the model itself: this body runs on
    // every page switch, and closures or strings passed in from here would make SwiftUI update
    // every item (re-tiling the toolbar and rebuilding its overflow menu) instead of only the
    // items whose `hidden` state changed.
    var body: some ToolbarContent {
        let courseId: String? = { if case .course(let id) = model.destination { id } else { nil } }()
        let awayFromNow = courseId.map { model.ui(for: $0).isAwayFromDefault } ?? false
        let hasWebsite = courseId.flatMap { model.course(id: $0) }.flatMap { Links.web($0.course.url) } != nil
        let prominent = model.destination == .sources
            && model.primaryActionWinner(for: SourceRow.primaryActionCandidates(model.sourceRows)) == .page(.pagePrimary)

        // Never reads the course section: the items disable themselves outside This Week.
        ToolbarItem(placement: .navigation) { WeekStepperControl() }
            .hidden(courseId == nil)
        ToolbarItem(placement: .navigation) { BackToCurrentWeekButton() }
            .hidden(!awayFromNow)
        ToolbarItem(placement: .primaryAction) { SyncNowButton() }
            .hidden(model.destination != .thisWeek)
        if #available(macOS 26.1, *) {
            ToolbarItem(placement: .primaryAction) { CourseWebsiteButton() }
                .visibilityPriority(.low)
                .hidden(!hasWebsite)
        } else {
            ToolbarItem(placement: .primaryAction) { CourseWebsiteButton() }
                .hidden(!hasWebsite || compact)
        }
        ToolbarSpacer(.fixed, placement: .primaryAction)
            .hidden(courseId == nil)
        ToolbarItem(placement: .primaryAction) { InspectorToggleButton() }
            .hidden(courseId == nil)
        ToolbarItem(placement: .primaryAction) { SyncAllToolbarButton(prominent: prominent) }
            .sharedBackgroundVisibility(prominent ? .hidden : .automatic)
            .hidden(model.destination != .sources)
    }
}

/// ‹ › for the course's week (enabled in the This Week section).
private struct WeekStepperControl: View {
    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n

    var body: some View {
        let ui: CourseUIState? = { if case .course(let id) = model.destination { model.ui(for: id) } else { nil } }()
        ControlGroup {
            Button {
                model.stepWeek(by: -1)
            } label: {
                Label(l10n("mac.actions.previousWeek"), systemImage: "chevron.backward")
            }
            .disabled(ui?.section != .week || ui?.previousWeek == nil)
            .help(l10n("mac.actions.previousWeek"))
            Button {
                model.stepWeek(by: 1)
            } label: {
                Label(l10n("mac.actions.nextWeek"), systemImage: "chevron.forward")
            }
            .disabled(ui?.section != .week || ui?.nextWeek == nil)
            .help(l10n("mac.actions.nextWeek"))
        }
    }
}

/// Back to now; with the current week unknown, back to "Recent materials".
private struct BackToCurrentWeekButton: View {
    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n

    var body: some View {
        let ui: CourseUIState? = { if case .course(let id) = model.destination { model.ui(for: id) } else { nil } }()
        let known = ui.map { $0.currentWeek != nil } ?? true
        let title = known ? l10n("mac.toolbar.backToCurrentWeek") : l10n("mac.toolbar.showRecentMaterials")
        Button {
            model.showCurrentWeek()
        } label: {
            Label(title, systemImage: "arrow.uturn.backward")
        }
        .help(known ? l10n("mac.actions.currentWeek") : title)
        .disabled(ui?.section != .week)
    }
}

/// Open Course Website: only a web address (http/https).
private struct CourseWebsiteButton: View {
    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n
    @Environment(\.openURL) private var openURL

    var body: some View {
        Button {
            if case .course(let id) = model.destination, let url = model.course(id: id).flatMap({ Links.web($0.course.url) }) {
                openURL(url)
            }
        } label: {
            Label(l10n("mac.actions.openCourseWebsite"), systemImage: "safari")
        }
        .help(l10n("mac.actions.openCourseWebsite"))
    }
}

/// Shows or hides the course inspector (⌃⌘I in the View menu).
private struct InspectorToggleButton: View {
    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n

    var body: some View {
        Button {
            model.inspectorShown.toggle()
        } label: {
            Label(l10n("mac.toolbar.inspector"), systemImage: "sidebar.trailing")
        }
        .help(l10n("mac.toolbar.inspector"))
    }
}

/// Sync All: prominent (glass) only when it is the window's one tinted action.
private struct SyncAllToolbarButton: View {
    let prominent: Bool
    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n

    var body: some View {
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
