// Copying text and opening links, shared by every screen and command.

import AppKit
import SwiftUI
import PageLampModel

@MainActor
enum Pasteboard {
    /// Puts `text` on the general pasteboard; announces `announcement` (usually "Copied") to
    /// VoiceOver when given (spec §7.1: "Copied" is announced).
    static func copy(_ text: String, announce announcement: String? = nil) {
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(text, forType: .string)
        if let announcement {
            AccessibilityNotification.Announcement(announcement).post()
        }
    }
}

@MainActor
enum Links {
    /// Opens a material or deadline link: a course-folder document in its app, a web address in
    /// the browser. An app, executable or script in a course folder is shown in Finder instead of
    /// run (`LinkAction`). Anything else (no scheme, `javascript:`, …) never opens (`CourseLink`).
    static func open(_ link: CourseLink?, openURL: OpenURLAction) {
        guard let link else { return }
        switch LinkAction.of(link) {
        case .open(let url): NSWorkspace.shared.open(url)
        case .reveal(let url): showInFinder(url)
        case .openWeb(let url): openURL(url)
        }
    }

    /// Opens `string` only if it is a web address (course websites, event links).
    static func openWeb(_ string: String?, openURL: OpenURLAction) {
        if case .web(let url)? = CourseLink(string) { openURL(url) }
    }

    /// The web address in `string`, if it is one (http/https only).
    static func web(_ string: String?) -> URL? {
        if case .web(let url)? = CourseLink(string) { return url }
        return nil
    }

    static func showInFinder(_ url: URL) {
        NSWorkspace.shared.activateFileViewerSelecting([url])
    }

    /// Show in Finder for a path that may be gone by now (a moved course folder, a config file
    /// not created yet): selects it, else opens its folder if that exists, else beeps. Checked
    /// when clicked, so view bodies never touch the disk.
    static func revealInFinder(path: String) {
        let url = URL(filePath: (path as NSString).expandingTildeInPath)
        let folder = url.deletingLastPathComponent()
        var isDirectory: ObjCBool = false
        if FileManager.default.fileExists(atPath: url.path(percentEncoded: false)) {
            showInFinder(url)
        } else if FileManager.default.fileExists(atPath: folder.path(percentEncoded: false), isDirectory: &isDirectory),
                  isDirectory.boolValue {
            NSWorkspace.shared.open(folder)
        } else {
            NSSound.beep()
        }
    }
}
