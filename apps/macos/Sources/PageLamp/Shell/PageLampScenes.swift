// Scenes (spec §2.1, M1): the main window and Settings. The welcome window (M2) and the menu bar
// extra (M3) come later.

import AppKit
import SwiftUI
import PageLampModel

/// The app's scenes; `PageLampApp` (the executable) owns the model and the delegate.
public struct PageLampScenes: Scene {
    /// The main window's scene id (`openWindow(id:)`).
    static let mainWindowID = "main"

    let model: AppModel

    public init(model: AppModel) {
        self.model = model
    }

    public var body: some Scene {
        Window(model.menuL10n("mac.app.name"), id: Self.mainWindowID) {
            RootView()
                .pageLampEnvironment(model)
        }
        .defaultSize(width: PLSize.windowMainWidth, height: PLSize.windowMainHeight)
        .windowResizability(.contentMinSize)
        .commands {
            SidebarCommands()
            InspectorCommands()
            ToolbarCommands()
            PageLampCommands(model: model)
            CourseMenuCommands(model: model)
            #if PAGELAMP_PREVIEW
            DebugCommands(model: model)
            #endif
        }

        Settings {
            SettingsView()
                .pageLampEnvironment(model)
        }
    }
}

/// Last window closed → quit (there is no menu bar extra yet, spec §2.2).
public final class PageLampAppDelegate: NSObject, NSApplicationDelegate {
    override public init() {
        super.init()
    }

    public func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        true
    }
}
