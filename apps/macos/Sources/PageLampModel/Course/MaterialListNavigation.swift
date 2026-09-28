// The Mac list behaviour of a week's materials (spec §3.2 "Mac list behaviour") without the
// views: which row is selected and where ↑/↓ move it, what Return / a double-click does with a
// material, and what ⌘C copies. The view (CourseMaterialList) only forwards events here.

import Foundation
import PageLampKit
import UniformTypeIdentifiers

/// The selected material of the list. The list is one keyboard stop (like a table): the
/// selection moves inside it with ↑/↓ and a click selects.
public struct MaterialListSelection: Equatable, Sendable {
    public private(set) var selected: String?

    public init(selected: String? = nil) {
        self.selected = selected
    }

    /// A click (or VoiceOver) on a row.
    public mutating func select(_ id: String) {
        selected = id
    }

    /// ↑ (-1) / ↓ (+1). Without a selection ↓ selects the first row and ↑ the last; the
    /// selection stops at either end (no wrapping).
    public mutating func move(by delta: Int, in ids: [String]) {
        guard !ids.isEmpty, delta != 0 else { return }
        guard let selected, let index = ids.firstIndex(of: selected) else {
            self.selected = delta > 0 ? ids.first : ids.last
            return
        }
        self.selected = ids[min(max(index + delta, 0), ids.count - 1)]
    }

    /// Keeps the selection only while its material is still listed (another week, a sync).
    public mutating func keep(in ids: [String]) {
        if let selected, !ids.contains(selected) { self.selected = nil }
    }

    /// The list's handler: every key and pointer event the view receives comes through here, so
    /// the behaviour is testable without a window. Returns what the view must do, if anything;
    /// nil for Return without a selection means the key isn't handled (it goes on to the
    /// window, e.g. a default button).
    public mutating func handle(_ event: MaterialListEvent, in ids: [String]) -> MaterialListEffect? {
        switch event {
        case .focusEntered:
            // Tab into the list selects its first row, like a table; a kept selection stays.
            keep(in: ids)
            if selected == nil { move(by: 1, in: ids) }
            return nil
        case .click(let id):
            guard ids.contains(id) else { return nil }
            select(id)
            return nil
        case .doubleClick(let id):
            guard ids.contains(id) else { return nil }
            select(id)
            return .open(id)
        case .move(let delta):
            move(by: delta, in: ids)
            return nil
        case .returnKey:
            guard let selected, ids.contains(selected) else { return nil }
            return .open(selected)
        }
    }

    /// What ⌘C copies: the selected material's link or title (`MaterialCopy`); nothing without
    /// a selection (the Copy item is then disabled).
    public func copyItems(_ materials: [MaterialView]) -> [String] {
        materials.first { $0.id == selected }.map { [MaterialCopy.text(for: $0)] } ?? []
    }
}

/// A key or pointer event on the materials list (spec §3.2 "Mac list behaviour").
public enum MaterialListEvent: Equatable, Sendable {
    /// Keyboard focus moved into the list (Tab, or a click).
    case focusEntered
    /// A click on a row.
    case click(String)
    /// A double-click on a row.
    case doubleClick(String)
    /// ↑ (-1) / ↓ (+1).
    case move(Int)
    /// Return.
    case returnKey
}

/// What the list does after an event.
public enum MaterialListEffect: Equatable, Sendable {
    /// Opens the material with this id (`LinkAction` decides how).
    case open(String)
}

/// What opening a material or deadline link does.
public enum LinkAction: Equatable, Sendable {
    /// A document in a course folder: opens in its app.
    case open(URL)
    /// An app, an executable or a script in a course folder: shown in Finder, never run from
    /// PageLamp (a double-click or Return on a row must not launch a program).
    case reveal(URL)
    /// A web address: the browser.
    case openWeb(URL)

    /// The action for `link`; `isLaunchable` decides for local files (`LocalFile.isLaunchable`).
    public static func of(_ link: CourseLink, isLaunchable: (URL) -> Bool = LocalFile.isLaunchable) -> LinkAction {
        switch link {
        case .web(let url): .openWeb(url)
        case .file(let url): isLaunchable(url) ? .reveal(url) : .open(url)
        }
    }
}

public enum LocalFile {
    /// Extensions whose default app runs them or installs something (Terminal, Installer, Script
    /// Editor's runner, Automator, the Java launcher), or that are programs themselves: caught
    /// by name even when the file's type isn't known (a file that isn't there yet).
    public static let launchableExtensions: Set<String> = [
        "app", "command", "tool", "terminal", "sh", "bash", "zsh", "csh", "tcsh", "ksh", "fish",
        "py", "pyw", "rb", "pl", "php", "jar", "pkg", "mpkg", "workflow", "action", "scpt",
        "scptd", "applescript", "prefpane", "saver", "osax", "kext", "plugin", "bundle",
        // Documents whose default app opens something else (a file, a web page, a profile).
        "fileloc", "webloc", "inetloc", "mobileconfig",
    ]

    /// Whether opening `url` would run a program: a launchable extension, an application, or a
    /// file whose type is a program or a script (a Unix executable without an extension, which
    /// Finder opens in Terminal). A document stays a document even with its executable bit set
    /// (every file on an exFAT drive has one): its type decides, like Finder. Reads the file
    /// system, so call it on an action (a click, Return), never from a view body.
    public static func isLaunchable(_ url: URL) -> Bool {
        // NSWorkspace follows symlinks, so judge the file a link points to, not the link.
        let target = url.resolvingSymlinksInPath()
        if target != url, isLaunchable(target) { return true }
        if launchableExtensions.contains(url.pathExtension.lowercased()) { return true }
        guard let values = try? url.resourceValues(forKeys: [.isApplicationKey, .contentTypeKey]) else { return false }
        if values.isApplication == true { return true }
        guard let type = values.contentType else { return false }
        return type.conforms(to: .executable) || type.conforms(to: .script)
    }
}

/// What ⌘C copies for a material: its link (a folder file's path, a web address), else its title.
public enum MaterialCopy {
    public static func text(for material: MaterialView) -> String {
        switch CourseLink(material.url) {
        case .file(let url): url.path(percentEncoded: false)
        case .web(let url): url.absoluteString
        case nil: material.title
        }
    }
}
