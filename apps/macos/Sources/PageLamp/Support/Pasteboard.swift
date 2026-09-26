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
    /// Opens a material or deadline link: a course-folder file in its app, a web address in the
    /// browser. Anything else (no scheme, `javascript:`, …) never opens (`CourseLink`).
    static func open(_ link: CourseLink?, openURL: OpenURLAction) {
        switch link {
        case .file(let url): NSWorkspace.shared.open(url)
        case .web(let url): openURL(url)
        case nil: break
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
}
