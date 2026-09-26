// Connect your AI app (spec §3.4, M1): the numbered setup steps per client, the client picker's
// titles and preselection, and the "Good to know" notes. The steps are the Tauri app's
// `InstallSteps` (apps/desktop/src/features/connect/InstallSteps.tsx) with the Mac's own quit-first
// wording; everything is localized here so tests can read the exact text.

import Foundation
import PageLampKit

/// Text that can be copied from the Connect page (a snippet, a command, a path).
public struct ConnectCopyable: Equatable, Sendable {
    public enum Kind: Equatable, Sendable {
        /// The whole snippet, the entry only, or the `mcpServers` section of a config file.
        case configuration, entry, serversSection
        /// A terminal command, or the command that removes an earlier setup.
        case command, removeCommand
        /// The AI app's config file location.
        case path
    }

    public var kind: Kind
    public var text: String
    /// The visible button title ("Copy Configuration").
    public var copyTitle: String
    /// What VoiceOver hears ("Copy Claude Desktop configuration").
    public var copyAccessibilityLabel: String

    public init(kind: Kind, text: String, copyTitle: String, copyAccessibilityLabel: String) {
        self.kind = kind
        self.text = text
        self.copyTitle = copyTitle
        self.copyAccessibilityLabel = copyAccessibilityLabel
    }

    /// Whether the text contains PageLamp's own location, so it stops working once PageLamp
    /// moves (a temporary location asks before copying it, spec S15). The AI app's config path
    /// does not.
    public var dependsOnAppLocation: Bool { kind != .path }
}

/// One numbered step of a client's setup.
public struct InstallStep: Equatable, Sendable, Identifiable {
    /// What follows the step's sentence.
    public enum Extra: Equatable, Sendable {
        /// The config file's location (Copy Path, Show in Finder).
        case path(ConnectCopyable)
        /// A snippet or command with its copy button.
        case code(ConnectCopyable)
        /// A further sentence with an optional snippet (the paste variants, "remove first").
        case variant(title: String?, text: String, code: ConnectCopyable?)
        /// A further sentence without code.
        case note(String)
    }

    public var id: String
    public var text: String
    public var extras: [Extra]

    public init(id: String, text: String, extras: [Extra] = []) {
        self.id = id
        self.text = text
        self.extras = extras
    }
}

/// A "Good to know" note: localized from its code, or the core's English when this build has no
/// translation for it (shown tagged English).
public struct ConnectNote: Equatable, Sendable, Hashable {
    public var text: String
    public var isEnglishFallback: Bool
}

public enum ConnectSetup {
    // MARK: Steps

    /// The steps for `config`, by install kind, with a few client-specific ones (Claude Desktop
    /// quits first; Claude Code's "new sessions"; Codex's "new session"; the generic wording).
    public static func steps(for config: McpClientConfig, l10n: L10n) -> [InstallStep] {
        let app = ["app": config.title]
        let snippet = ConnectCopyable(
            kind: config.installKind == .shellCommand ? .command : .configuration,
            text: config.content,
            copyTitle: config.installKind == .shellCommand
                ? l10n("mac.actions.copyCommand") : l10n("mac.actions.copyConfiguration"),
            copyAccessibilityLabel: config.installKind == .shellCommand
                ? l10n("connect.copyCommand", app) : l10n("connect.copyConfig", app)
        )
        let path: InstallStep.Extra? = config.configPathHint.map { hint in
            .path(ConnectCopyable(
                kind: .path,
                text: hint,
                copyTitle: l10n("mac.actions.copyPath"),
                copyAccessibilityLabel: l10n("connect.copyPath", app)
            ))
        }

        switch config.installKind {
        case .jsonSnippet:
            if config.client == .claudeDesktop {
                // Quit first: Claude Desktop writes this file when it quits, overwriting an edit
                // made while it was running.
                return [
                    InstallStep(id: "quit", text: l10n("mac.connect.quitFirst", app)),
                    InstallStep(
                        id: "open",
                        text: path == nil ? l10n("connect.steps.openJsonNoPath") : l10n("connect.steps.openJson"),
                        extras: path.map { [$0] } ?? []
                    ),
                    InstallStep(
                        id: "paste",
                        text: l10n("connect.steps.pasteJson"),
                        extras: [.code(snippet)] + existingFile(config, l10n: l10n)
                    ),
                    InstallStep(id: "reopen", text: l10n("connect.steps.saveAndReopen", app)),
                ]
            }
            if config.client == .generic {
                // A bare server definition: every client has its own config format.
                return [
                    InstallStep(id: "open", text: l10n("connect.steps.genericOpen")),
                    InstallStep(id: "add", text: l10n("connect.steps.genericAdd"), extras: [.code(snippet)]),
                ]
            }
            return [
                InstallStep(
                    id: "open",
                    text: path == nil ? l10n("connect.steps.openJsonNoPath") : l10n("connect.steps.openJson"),
                    extras: path.map { [$0] } ?? []
                ),
                InstallStep(id: "paste", text: l10n("connect.steps.pasteJsonMerge"), extras: [.code(snippet)]),
                InstallStep(id: "restart", text: l10n("connect.steps.restart")),
            ]

        case .tomlSnippet:
            var pasteExtras: [InstallStep.Extra] = [.code(snippet)]
            if let table = ConnectSnippets.tomlTable(config.content) {
                // Set up once already? A second copy of the table makes the file invalid TOML.
                pasteExtras.append(.note(l10n("connect.steps.replaceToml", ["table": table])))
            }
            return [
                InstallStep(
                    id: "open",
                    text: path == nil ? l10n("connect.steps.openTomlNoPath") : l10n("connect.steps.openToml"),
                    extras: path.map { [$0] } ?? []
                ),
                InstallStep(id: "paste", text: l10n("connect.steps.pasteToml"), extras: pasteExtras),
                InstallStep(
                    id: "restart",
                    text: config.client == .codex ? l10n("connect.steps.restartCodex") : l10n("connect.steps.restart")
                ),
            ]

        case .shellCommand:
            var runExtras: [InstallStep.Extra] = [.code(snippet)]
            if let remove = ConnectSnippets.claudeMcpRemove(config.content) {
                // `claude mcp add` fails while the server exists: remove it first, then add again.
                runExtras.append(.variant(
                    title: nil,
                    text: l10n("connect.steps.removeFirst"),
                    code: ConnectCopyable(
                        kind: .removeCommand,
                        text: remove,
                        copyTitle: l10n("mac.actions.copyRemoveCommand"),
                        copyAccessibilityLabel: l10n("connect.copyRemoveCommand", app)
                    )
                ))
            }
            var steps = [
                InstallStep(id: "terminal", text: l10n("connect.steps.openTerminal")),
                InstallStep(id: "run", text: l10n("connect.steps.runCommand"), extras: runExtras),
            ]
            if config.client == .claudeCode {
                steps.append(InstallStep(id: "after", text: l10n("connect.steps.claudeCodeAfter")))
            }
            return steps
        }
    }

    /// For a config file that isn't new: with an `mcpServers` section only the entry goes inside
    /// it; with other settings but no `mcpServers`, the whole `"mcpServers": { … }` key goes next
    /// to them. The full snippet fits neither (the three paste cases).
    private static func existingFile(_ config: McpClientConfig, l10n: L10n) -> [InstallStep.Extra] {
        let app = ["app": config.title]
        var extras: [InstallStep.Extra] = []
        if let entry = ConnectSnippets.mcpServerEntry(config.content),
           let name = ConnectSnippets.mcpServerName(config.content) {
            extras.append(.variant(
                title: l10n("mac.connect.existingFile.hasServers"),
                text: l10n("connect.steps.pasteJsonEntry", ["name": name]),
                code: ConnectCopyable(
                    kind: .entry,
                    text: entry,
                    copyTitle: l10n("mac.actions.copyEntryOnly"),
                    copyAccessibilityLabel: l10n("connect.copyEntry", app)
                )
            ))
        }
        if let key = ConnectSnippets.mcpServersKey(config.content) {
            extras.append(.variant(
                title: l10n("mac.connect.existingFile.otherSettings"),
                text: l10n("connect.steps.pasteJsonKey"),
                code: ConnectCopyable(
                    kind: .serversSection,
                    text: key,
                    copyTitle: l10n("mac.actions.copyServersSection"),
                    copyAccessibilityLabel: l10n("connect.copyKey", app)
                )
            ))
        }
        return extras
    }

    // MARK: Client picker

    /// The picker's short title: "Other app" for the generic client, else the product name before
    /// any " / " or " (…)" ("Codex / ChatGPT desktop (Work/Codex mode)" → "Codex").
    public static func pickerTitle(for config: McpClientConfig, l10n: L10n) -> String {
        if config.client == .generic { return l10n("mac.connect.otherApp") }
        var title = Substring(config.title)
        if let slash = title.range(of: " / ") { title = title[..<slash.lowerBound] }
        if let paren = title.range(of: " (") { title = title[..<paren.lowerBound] }
        let trimmed = title.trimmingCharacters(in: .whitespaces)
        return trimmed.isEmpty ? config.title : trimmed
    }

    /// Whether `doctor().mcp_clients` says this client already has a PageLamp entry.
    public static func isConfigured(_ client: McpClient, in presence: McpClientPresence?) -> Bool {
        guard let presence else { return false }
        return switch client {
        case .claudeDesktop: presence.claudeDesktop
        case .claudeCode: presence.claudeCode
        case .codex: presence.codex
        case .generic: false
        }
    }

    /// The client shown first: the first one already set up (a returning student re-copying after
    /// moving the app), else Claude Desktop, else the first offered.
    public static func preselected(_ sorted: [McpClientConfig], presence: McpClientPresence?) -> McpClient? {
        sorted.first { isConfigured($0.client, in: presence) }?.client
            ?? sorted.first { $0.client == .claudeDesktop }?.client
            ?? sorted.first?.client
    }

    // MARK: Notes

    /// The core's notes for one client, minus those said elsewhere: the page-level temporary
    /// location warning, and what that client's own steps already say. `note_codes[i]` belongs to
    /// `notes[i]` only when both lists have the same length.
    public static func notes(for config: McpClientConfig, l10n: L10n) -> [ConnectNote] {
        let paired = config.noteCodes.count == config.notes.count
        var seen = Set<String>()
        return config.notes.enumerated().compactMap { index, note in
            let code = paired ? config.noteCodes[index] : nil
            if let code, !showsOnCard(client: config.client, code: code) { return nil }
            let result = code.flatMap { key(for: $0) }.map { ConnectNote(text: l10n($0), isEnglishFallback: false) }
                ?? ConnectNote(text: note, isEnglishFallback: true)
            return seen.insert(result.text).inserted ? result : nil
        }
    }

    /// Whether a note with this code belongs in the client's "Good to know".
    public static func showsOnCard(client: McpClient, code: McpNoteCode) -> Bool {
        if code == .runFromTemporaryLocation { return false }
        let coveredBySteps: [McpNoteCode] = switch client {
        case .claudeDesktop: [.restartClientAfterChange, .quitBeforeEditing]
        case .claudeCode, .codex: [.restartClientAfterChange]
        case .generic: [.genericStdioClient]
        }
        return !coveredBySteps.contains(code)
    }

    /// The translation of a note code (nil: this build shows the core's English).
    static func key(for code: McpNoteCode) -> String? {
        switch code {
        case .worksOnAllClaudePlans: "connect.noteCodes.works_on_all_claude_plans"
        case .adminsMayDisableExtensions: "connect.noteCodes.admins_may_disable_extensions"
        case .needsPaidClaudePlan: "connect.noteCodes.needs_paid_claude_plan"
        case .codexConfigSharedWithChatgptDesktop: "connect.noteCodes.codex_config_shared_with_chatgpt_desktop"
        case .codexPlusAndEduDocumented: "connect.noteCodes.codex_plus_and_edu_documented"
        case .freeGoUndocumented: "connect.noteCodes.free_go_undocumented"
        case .restartClientAfterChange: "connect.noteCodes.restart_client_after_change"
        case .customDataDir: "connect.noteCodes.custom_data_dir"
        case .genericStdioClient: "connect.noteCodes.generic_stdio_client"
        case .quitBeforeEditing, .runFromTemporaryLocation: nil
        }
    }

    // MARK: Page texts

    /// "Add to a config file" / "Run in a terminal".
    public static func installKindText(_ kind: InstallKind, l10n: L10n) -> String {
        switch kind {
        case .jsonSnippet: l10n("connect.installKind.json_snippet")
        case .tomlSnippet: l10n("connect.installKind.toml_snippet")
        case .shellCommand: l10n("connect.installKind.shell_command")
        }
    }

    /// The S15 warning's title and body.
    public static func temporaryLocationText(_ location: TemporaryLocation, l10n: L10n) -> (title: String, body: String) {
        let key = switch location {
        case .diskImage: "disk_image"
        case .translocated: "translocated"
        case .appImage: "appimage"
        }
        return (l10n("connect.temporaryLocation.\(key).title"), l10n("connect.temporaryLocation.\(key).body"))
    }
}
