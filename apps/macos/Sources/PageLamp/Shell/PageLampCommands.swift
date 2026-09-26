// Menus and shortcuts (spec §2.7, M1 items). The standard menus (Edit, Window, the sidebar,
// inspector and toolbar toggles) come from the system and follow AppleLanguages; our items use
// `model.menuL10n` (the launch language) so the menu bar never mixes two languages.

import SwiftUI
import PageLampModel

struct PageLampCommands: Commands {
    let model: AppModel

    var body: some Commands {
        let l10n = model.menuL10n

        // PageLamp ▸ About (the standard panel: version, tagline, licence, links)
        CommandGroup(replacing: .appInfo) {
            Button(l10n("mac.menu.about")) {
                Task { await AppActions.showAbout(model) }
            }
        }

        // File ▸ Sync Now ⌘R (Add Source… ⇧⌘N is M2)
        CommandGroup(replacing: .newItem) {
            Button(l10n("mac.actions.syncNow")) {
                Task { await model.syncAll() }
            }
            .keyboardShortcut("r")
            .disabled(!model.canSync)
        }

        // View ▸ This Week ⌘1 · Sources & Sync ⌘2 · Connect AI App ⌘3 (above the sidebar toggle)
        CommandGroup(before: .sidebar) {
            Button(l10n("mac.nav.thisWeek")) { model.destination = .thisWeek }
                .keyboardShortcut("1")
            Button(l10n("mac.nav.sources")) { model.destination = .sources }
                .keyboardShortcut("2")
            Button(l10n("mac.nav.connect")) { model.destination = .connect }
                .keyboardShortcut("3")
            Divider()
        }

        // Go ▸ Previous Week ⌘[ · Next Week ⌘] · Current Week ⇧⌘T (⌘T is HIG-reserved)
        CommandMenu(l10n("mac.menu.go")) {
            Button(l10n("mac.actions.previousWeek")) { model.stepWeek(by: -1) }
                .keyboardShortcut("[")
                .disabled(!model.canStepWeek(by: -1))
            Button(l10n("mac.actions.nextWeek")) { model.stepWeek(by: 1) }
                .keyboardShortcut("]")
                .disabled(!model.canStepWeek(by: 1))
            Button(l10n("mac.actions.currentWeek")) { model.showCurrentWeek() }
                .keyboardShortcut("t", modifiers: [.command, .shift])
                .disabled(!model.canShowCurrentWeek)
        }

        // Help
        CommandGroup(replacing: .help) {
            Button(l10n("mac.menu.help")) { AppActions.open(BrandLinks.homepage) }
            Divider()
            Button(l10n("mac.actions.copyDiagnosticReport")) {
                Task { await model.showDiagnosticReport(in: .main) }
            }
            Button(l10n("mac.actions.reportProblem")) { AppActions.open(BrandLinks.issues) }
            Button(l10n("mac.actions.openLogsFolder")) {
                Task { await AppActions.openLogsFolder(model) }
            }
        }
    }
}

/// The Course menu (spec §2.7, M1 items) for the course on screen, from the page's focused
/// value; disabled when no course is shown. Download Files…, Let My AI App Read Materials and
/// Hide Course are edits (M2).
struct CourseMenuCommands: Commands {
    let model: AppModel
    @FocusedValue(\.courseCommands) private var course

    var body: some Commands {
        let l10n = model.menuL10n
        CommandMenu(l10n("mac.menu.course")) {
            Button(l10n("mac.actions.openCourseWebsite")) {
                AppActions.open(course?.website)
            }
            .disabled(course?.website == nil)
            Divider()
            Button(l10n("mac.actions.aiPolicy")) { course?.showAIPolicy() }
                .disabled(course == nil)
            Button(l10n("mac.actions.setTermDates")) { course?.showTermDates() }
                .disabled(course == nil)
        }
    }
}

#if PAGELAMP_PREVIEW
/// Debug (preview build only): switch mock scenarios or to live data, run mock syncs.
struct DebugCommands: Commands {
    let model: AppModel

    var body: some Commands {
        let l10n = model.menuL10n
        let isMock = if case .mock = model.dataMode { true } else { false }

        CommandMenu(l10n("mac.debug.menu")) {
            Menu(l10n("mac.debug.dataSource")) {
                Picker(l10n("mac.debug.mockData"), selection: scenario) {
                    ForEach(MockScenario.allCases, id: \.self) { scenario in
                        Text(Self.title(of: scenario, l10n: l10n)).tag(Optional(scenario))
                    }
                }
                .pickerStyle(.inline)
                Divider()
                // Shares data with the installed PageLamp: asks first (RootView's alert).
                Toggle(l10n("mac.debug.liveData"), isOn: live)
            }
            .disabled(model.isSyncing)
            Divider()
            Button(l10n("mac.debug.runMockSync")) {
                Task { await model.syncAll() }
            }
            .disabled(!isMock || !model.canSync)
            Button(l10n("mac.debug.runMockSyncRejected")) {
                Task { await model.runMockSyncWithRejectedToken() }
            }
            .disabled(!isMock || !model.canSync)
        }
    }

    private var scenario: Binding<MockScenario?> {
        Binding(
            get: { if case .mock(let scenario) = model.dataMode { scenario } else { nil } },
            set: { scenario in
                guard let scenario else { return }
                Task { await model.useMock(scenario) }
            }
        )
    }

    private var live: Binding<Bool> {
        Binding(
            get: { model.dataMode == .live },
            set: { if $0, model.dataMode != .live { model.confirmingLiveData = true } }
        )
    }

    private static func title(of scenario: MockScenario, l10n: L10n) -> String {
        switch scenario {
        case .demo: l10n("mac.debug.scenario.demo")
        case .empty: l10n("mac.debug.scenario.empty")
        case .expired: l10n("mac.debug.scenario.expired")
        case .error: l10n("mac.debug.scenario.error")
        case .busy: l10n("mac.debug.scenario.busy")
        case .crashed: l10n("mac.debug.scenario.crashed")
        }
    }
}
#endif
