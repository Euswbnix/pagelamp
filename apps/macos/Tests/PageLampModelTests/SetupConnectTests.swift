// Connect your AI app (spec §3.4): the order-preserving JSON, the snippet variants (a port of the
// Tauri snippet.test.ts), the quarantine command, the steps per client, notes and preselection.

import Foundation
import PageLamp
import PageLampKit
@testable import PageLampModel
import Testing

@Suite("Connect your AI app") @MainActor
struct SetupConnectTests {
    let en = L10n(locale: Locale(identifier: "en"), table: .app)
    let zh = L10n(locale: Locale(identifier: "zh-Hans"), table: .app)
    let configs = ConnectSnippets.sorted(MockFixtures.mcpClientConfigs(binary: MockService.binaryPath))

    // MARK: JSON

    @Test("ordered JSON prints like JSON.stringify(value, null, 2) and keeps key order")
    func orderedJSON() throws {
        let text = #"{"z": 1, "a": [true, null, -2.5e3, {}], "m": {"k": "v\"\\/\u00e9\n"}, "e": []}"#
        let value = try #require(OrderedJSON(parsing: text))
        #expect(value.members?.map(\.key) == ["z", "a", "m", "e"])
        #expect(value.pretty() == """
        {
          "z": 1,
          "a": [
            true,
            null,
            -2.5e3,
            {}
          ],
          "m": {
            "k": "v\\"\\\\/é\\n"
          },
          "e": []
        }
        """)
        // Surrogate pairs and control characters.
        #expect(OrderedJSON(parsing: #""\ud83d\ude00""#) == .string("😀"))
        #expect(OrderedJSON.quote("a\u{01}b") == #""a\u0001b""#)
        // Round trip.
        #expect(OrderedJSON(parsing: value.pretty()) == value)
    }

    @Test("ordered JSON rejects what JSON.parse rejects", arguments: [
        "", "{", "{\"a\" 1}", "{\"a\": 1,}", "[1,]", "01", "1.", "\"\\x\"", "tru", "{} {}", "\"\u{01}\"", "\"\\ud83d\"",
    ])
    func orderedJSONInvalid(_ text: String) {
        #expect(OrderedJSON(parsing: text) == nil)
    }

    // MARK: Snippets (snippet.test.ts)

    @Test("the one server entry, ready to paste into an existing mcpServers")
    func serverEntry() throws {
        let content = """
        {
          "mcpServers": {
            "pagelamp": {
              "command": "/Applications/PageLamp.app/x",
              "args": [
                "mcp"
              ]
            }
          }
        }
        """
        let entry = try #require(ConnectSnippets.mcpServerEntry(content))
        #expect(entry == "\"pagelamp\": {\n  \"command\": \"/Applications/PageLamp.app/x\",\n  \"args\": [\n    \"mcp\"\n  ]\n}")
        // Valid JSON once wrapped in an object, equal to the snippet's mcpServers.
        #expect(OrderedJSON(parsing: "{\(entry)}") == OrderedJSON(parsing: content)?["mcpServers"])
        #expect(ConnectSnippets.mcpServerName(content) == "pagelamp")

        let key = try #require(ConnectSnippets.mcpServersKey(content))
        #expect(key.hasPrefix("\"mcpServers\": {"))
        #expect(OrderedJSON(parsing: "{\"preferences\": {}, \(key)}")?["mcpServers"] == OrderedJSON(parsing: content)?["mcpServers"])
    }

    @Test("gives up on anything that isn't exactly one server", arguments: [
        "not json", "[mcp_servers.pagelamp]", #"{"mcpServers": {}}"#, #"{"mcpServers": {"a": {}, "b": {}}}"#,
        #"{"other": 1}"#, #"{"command": "x", "args": ["mcp"]}"#, #"{"mcpServers": []}"#,
    ])
    func serverEntryGivesUp(_ content: String) {
        #expect(ConnectSnippets.mcpServerEntry(content) == nil)
        #expect(ConnectSnippets.mcpServersKey(content) == nil)
        #expect(ConnectSnippets.mcpServerName(content) == nil)
    }

    @Test("TOML table and the claude mcp remove command")
    func tomlAndRemove() {
        let toml = "[mcp_servers.pagelamp]\ncommand = \"/x/pagelamp\"\nargs = [\"mcp\"]\n\n[mcp_servers.pagelamp.env]\nPAGELAMP_HOME = \"/d\"\n"
        #expect(ConnectSnippets.tomlTable(toml) == "mcp_servers.pagelamp")
        #expect(ConnectSnippets.tomlTable("no table here") == nil)
        #expect(ConnectSnippets.tomlTable("  [indented]") == nil)

        let add = "claude mcp add --scope user --env PAGELAMP_HOME='/Users/demo/My Data' --transport stdio pagelamp -- '/Applications/PageLamp.app/Contents/MacOS/pagelamp' mcp"
        #expect(ConnectSnippets.claudeMcpRemove(add) == "claude mcp remove --scope user pagelamp")
        #expect(ConnectSnippets.claudeMcpRemove("claude mcp add pagelamp -- /x/pagelamp mcp") == "claude mcp remove pagelamp")
        #expect(ConnectSnippets.claudeMcpRemove("npx something") == nil)
        #expect(ConnectSnippets.claudeMcpRemove("claude mcp add --scope user pagelamp") == nil)
    }

    @Test("the quarantine command targets the whole app, quoted for the shell")
    func quarantine() {
        #expect(QuarantineHint.command(for: "/Applications/PageLamp.app/Contents/MacOS/pagelamp")
            == "xattr -dr com.apple.quarantine '/Applications/PageLamp.app'")
        #expect(QuarantineHint.command(for: "/Users/demo/My Apps/Page'Lamp.app/Contents/MacOS/pagelamp")
            == #"xattr -dr com.apple.quarantine '/Users/demo/My Apps/Page'\''Lamp.app'"#)
        #expect(QuarantineHint.target(command: "/Users/demo/bin/pagelamp") == ("/Users/demo/bin/pagelamp", false))
        // The .app whose Contents/MacOS holds the binary, even if an outer folder ends in .app.
        #expect(QuarantineHint.target(command: "/Vol/A.app/Contents/MacOS/x/B.app/Contents/MacOS/pagelamp").path
            == "/Vol/A.app/Contents/MacOS/x/B.app")
    }

    @Test("the Connect page's quarantine command: the real app, never while in a temporary location (S15)")
    func quarantineOnPage() {
        let launch = "/Applications/PageLamp Preview.app/Contents/MacOS/pagelamp"
        // The running app's own bundle wins (the app the student would move and unblock).
        #expect(QuarantineHint.command(launchCommand: launch, appBundlePath: "/Users/demo/Apps/PageLamp.app", temporaryLocation: nil)
            == "xattr -dr com.apple.quarantine '/Users/demo/Apps/PageLamp.app'")
        // Not running as an app (swift run, snapshots): the launch command's app.
        #expect(QuarantineHint.command(launchCommand: launch, appBundlePath: nil, temporaryLocation: nil)
            == "xattr -dr com.apple.quarantine '/Applications/PageLamp Preview.app'")
        // A temporary copy: no command (the warning above says to move the app first).
        for location in [TemporaryLocation.translocated, .diskImage] {
            #expect(QuarantineHint.command(launchCommand: launch, appBundlePath: "/private/var/folders/x/AppTranslocation/y/d/PageLamp.app", temporaryLocation: location) == nil)
        }
        // Nothing absolute to point at: no command.
        #expect(QuarantineHint.command(launchCommand: "pagelamp", appBundlePath: nil, temporaryLocation: nil) == nil)
        #expect(QuarantineHint.command(launchCommand: nil, appBundlePath: nil, temporaryLocation: nil) == nil)
        // The test runner is not an app.
        #expect(QuarantineHint.runningAppPath == nil)
    }

    // MARK: Clients

    @Test("clients are offered Claude Desktop first, generic last, whatever the core's order")
    func order() {
        let shuffled = MockFixtures.mcpClientConfigs(binary: MockService.binaryPath).reversed()
        #expect(ConnectSnippets.sorted(Array(shuffled)).map(\.client) == [.claudeDesktop, .claudeCode, .codex, .generic])
        #expect(configs.map { ConnectSetup.pickerTitle(for: $0, l10n: en) } == ["Claude Desktop", "Claude Code", "Codex", "Other app"])
        #expect(ConnectSetup.pickerTitle(for: configs[3], l10n: zh) == "其他应用")
    }

    @Test("the first client already set up is preselected, else Claude Desktop")
    func preselection() {
        let codeSetUp = McpClientPresence(claudeDesktop: false, claudeCode: true, codex: false)
        #expect(ConnectSetup.preselected(configs, presence: codeSetUp) == .claudeCode)
        #expect(ConnectSetup.preselected(configs, presence: nil) == .claudeDesktop)
        let none = McpClientPresence(claudeDesktop: false, claudeCode: false, codex: false)
        #expect(ConnectSetup.preselected(configs, presence: none) == .claudeDesktop)
        #expect(ConnectSetup.preselected(Array(configs.dropFirst()), presence: none) == .claudeCode)
        #expect(ConnectSetup.preselected([], presence: none) == nil)
        #expect(!ConnectSetup.isConfigured(.generic, in: codeSetUp))
    }

    // MARK: Steps

    @Test("Claude Desktop: quit first, open the file, paste (three cases), reopen")
    func claudeDesktopSteps() throws {
        let steps = ConnectSetup.steps(for: configs[0], l10n: en)
        #expect(steps.map(\.id) == ["quit", "open", "paste", "reopen"])
        #expect(steps[0].text == "Quit Claude Desktop completely first (⌘Q). It saves over this file when it quits, so changes made while it's open are lost.")
        #expect(steps[1].text == "Open the configuration file at this location. Create it if it isn't there yet.")
        guard case .path(let path) = steps[1].extras.first else {
            Issue.record("no path")
            return
        }
        #expect(path.text == "~/Library/Application Support/Claude/claude_desktop_config.json")
        #expect(path.copyTitle == "Copy Path")
        #expect(path.copyAccessibilityLabel == "Copy Claude Desktop file path")
        #expect(!path.dependsOnAppLocation)

        #expect(steps[2].extras.count == 3)
        guard case .code(let snippet) = steps[2].extras[0],
              case .variant(let entryTitle, let entryText, let entry?) = steps[2].extras[1],
              case .variant(let keyTitle, let keyText, let key?) = steps[2].extras[2]
        else {
            Issue.record("unexpected paste extras: \(steps[2].extras)")
            return
        }
        #expect(snippet.text == configs[0].content)
        #expect(snippet.copyTitle == "Copy Configuration")
        #expect(snippet.dependsOnAppLocation)
        #expect(entryTitle == "It already has an \"mcpServers\" section")
        #expect(entryText.contains("\"pagelamp\" entry"))
        #expect(entry.copyTitle == "Copy Entry Only")
        #expect(entry.text.hasPrefix("\"pagelamp\": {\n  \"command\": \"\(MockService.binaryPath)\""))
        #expect(keyTitle == "It has other settings but no \"mcpServers\"")
        #expect(keyText.hasPrefix("If it has other settings"))
        #expect(key.copyTitle == "Copy mcpServers Section")
        #expect(key.copyAccessibilityLabel == "Copy Claude Desktop mcpServers section")
        #expect(steps[3].text == "Save the file, then open Claude Desktop again.")

        let chinese = ConnectSetup.steps(for: configs[0], l10n: zh)
        #expect(chinese[0].text == "先完全退出 Claude Desktop（⌘Q）。它退出时会覆盖这个文件，所以在它运行时做的修改会丢失。")
    }

    @Test("Claude Code: a terminal command, the remove-first fallback, then new sessions")
    func claudeCodeSteps() {
        let steps = ConnectSetup.steps(for: configs[1], l10n: en)
        #expect(steps.map(\.id) == ["terminal", "run", "after"])
        guard case .code(let command) = steps[1].extras.first,
              case .variant(nil, let text, let remove?) = steps[1].extras.last
        else {
            Issue.record("unexpected run extras: \(steps[1].extras)")
            return
        }
        #expect(command.kind == .command)
        #expect(command.copyTitle == "Copy Command")
        #expect(command.copyAccessibilityLabel == "Copy Claude Code command")
        #expect(text.hasPrefix("Added before"))
        #expect(remove.text == "claude mcp remove --scope user pagelamp")
        #expect(remove.copyTitle == "Copy Remove Command")
    }

    @Test("Codex: open the TOML file, add the lines (or replace the table), start a new session")
    func codexSteps() {
        let steps = ConnectSetup.steps(for: configs[2], l10n: en)
        #expect(steps.map(\.id) == ["open", "paste", "restart"])
        #expect(steps[0].text == "Open this TOML file. Create it if it isn't there yet.")
        guard case .note(let note) = steps[1].extras.last else {
            Issue.record("no replace note")
            return
        }
        #expect(note.contains("[mcp_servers.pagelamp]"))
        #expect(steps[2].text == "Save the file, then restart the app, or start a new Codex session.")
    }

    @Test("other apps: open the MCP settings, add the server")
    func genericSteps() {
        let steps = ConnectSetup.steps(for: configs[3], l10n: en)
        #expect(steps.map(\.id) == ["open", "add"])
        #expect(steps[1].extras.count == 1)
    }

    @Test("a JSON client without a path, and a config without a known shape")
    func otherJSONClient() {
        var config = configs[0]
        config = McpClientConfig(
            client: .codex, title: "Some App", installKind: .jsonSnippet, configPathHint: nil,
            content: config.content, notes: [], noteCodes: [], launch: config.launch
        )
        let steps = ConnectSetup.steps(for: config, l10n: en)
        #expect(steps.map(\.id) == ["open", "paste", "restart"])
        #expect(steps[0].text == "Open the app's configuration file.")
        #expect(steps[0].extras.isEmpty)
        #expect(steps[1].text.hasPrefix("Paste in this snippet."))
    }

    // MARK: Notes

    @Test("notes are localized from their codes, minus what the page or the steps already say")
    func notes() {
        #expect(ConnectSetup.notes(for: configs[0], l10n: en).map(\.text) == [
            "Works on every Claude plan, including Free.",
            "On Team, Enterprise and Education plans an admin can turn extensions off.",
        ])
        #expect(ConnectSetup.notes(for: configs[1], l10n: en).map(\.text) == [
            "Claude Code needs a paid Claude plan (Pro or higher).",
        ])
        #expect(ConnectSetup.notes(for: configs[2], l10n: zh).count == 3)
        #expect(ConnectSetup.notes(for: configs[3], l10n: en).isEmpty)
        #expect(ConnectSetup.notes(for: configs[0], l10n: en).allSatisfy { !$0.isEnglishFallback })

        // Codes that don't pair with the notes: every note is shown as the core's English.
        let unpaired = McpClientConfig(
            client: .codex, title: "X", installKind: .tomlSnippet, configPathHint: nil, content: "",
            notes: ["A note from a newer core.", "Another."], noteCodes: [.customDataDir], launch: configs[0].launch
        )
        let shown = ConnectSetup.notes(for: unpaired, l10n: zh)
        #expect(shown == [
            ConnectNote(text: "A note from a newer core.", isEnglishFallback: true),
            ConnectNote(text: "Another.", isEnglishFallback: true),
        ])
        // A known code without a translation (quit_before_editing) on another client: English.
        let quit = McpClientConfig(
            client: .codex, title: "X", installKind: .tomlSnippet, configPathHint: nil, content: "",
            notes: ["Quit first."], noteCodes: [.quitBeforeEditing], launch: configs[0].launch
        )
        #expect(ConnectSetup.notes(for: quit, l10n: en) == [ConnectNote(text: "Quit first.", isEnglishFallback: true)])
        #expect(!ConnectSetup.showsOnCard(client: .codex, code: .runFromTemporaryLocation))
    }

    @Test("S15 texts per temporary location")
    func temporaryLocationTexts() {
        #expect(ConnectSetup.temporaryLocationText(.translocated, l10n: en).title == "Move PageLamp to Applications before connecting")
        #expect(ConnectSetup.temporaryLocationText(.diskImage, l10n: en).body.contains("disk image"))
        #expect(ConnectSetup.temporaryLocationText(.appImage, l10n: zh).title == "请为 AI 应用使用安装版")
        #expect(ConnectSetup.installKindText(.shellCommand, l10n: en) == "Run in a terminal")
    }

    // MARK: Loading

    @Test("the page loads the configs and preselects the client already set up")
    func loadFromMock() async {
        let mock = MockService(scenario: .demo, timing: .instant, calendar: TestClock.calendar, now: { TestClock.now })
        let connect = ConnectModel()
        #expect(connect.phase == .loading)
        await connect.load(service: mock, binary: MockService.binaryPath)
        #expect(connect.data?.configs.map(\.client) == [.claudeDesktop, .claudeCode, .codex, .generic])
        #expect(connect.selectedClient == .claudeCode)
        #expect(connect.selectedConfig?.client == .claudeCode)
        #expect(connect.data?.temporaryLocation == nil)
        #expect(connect.data?.launchCommand == MockService.binaryPath)

        // A reload keeps the student's choice; a temporary location is passed through.
        connect.selectedClient = .codex
        await connect.load(service: mock, binary: MockService.binaryPath, temporaryLocation: .diskImage)
        #expect(connect.selectedClient == .codex)
        #expect(connect.data?.temporaryLocation == .diskImage)
    }

    @Test("a failing core shows the error state; a failed reload keeps the page and says so")
    func loadFails() async {
        // Every call goes to the mock (never the core's diagnostics, which would read the real
        // data folder); only mcp_client_configs fails.
        let mock = MockService(scenario: .demo, timing: .instant, calendar: TestClock.calendar, now: { TestClock.now })
        let failing = FixtureService(base: mock, failing: [.clientConfigs])
        let connect = ConnectModel()
        await connect.load(service: failing, binary: MockService.binaryPath)
        guard case .failed(let failure) = connect.phase else {
            Issue.record("expected the error state, got \(connect.phase)")
            return
        }
        #expect(failure.kind == .internal)
        #expect(connect.selectedConfig == nil)
        #expect(connect.reloadFailure == nil)

        // Try Again asks the page to reload (its load key changes) …
        let attempt = connect.attempt
        connect.retry()
        #expect(connect.attempt == attempt + 1)
        // … and succeeds.
        await connect.load(service: mock, binary: MockService.binaryPath)
        let loaded = connect.data
        #expect(loaded?.configs.count == 4)
        connect.selectedClient = .codex

        // A reload that fails keeps the steps and the selection, with an inline error.
        await connect.load(service: failing, binary: MockService.binaryPath)
        #expect(connect.data == loaded)
        #expect(connect.selectedClient == .codex)
        #expect(connect.reloadFailure?.kind == .internal)
        // The next good load clears it.
        await connect.load(service: mock, binary: MockService.binaryPath)
        #expect(connect.reloadFailure == nil)
    }

    @Test("another data source starts the page over; of overlapping loads only the latest applies")
    func dataSourceAndOverlap() async {
        let mock = MockService(scenario: .demo, timing: .instant, calendar: TestClock.calendar, now: { TestClock.now })
        let connect = ConnectModel()
        await connect.load(service: mock, binary: MockService.binaryPath, dataMode: .mock(.demo))
        #expect(connect.data?.temporaryLocation == nil)

        // A reload (say from a temporary location) is still waiting for the core when a newer one
        // starts and finishes: the older answer arrives last and is dropped.
        let hold = CallHold()
        await hold.arm()
        let held = HeldService(base: mock, configsHold: hold)
        let stale = Task {
            await connect.load(service: held, binary: MockService.binaryPath, temporaryLocation: .diskImage, dataMode: .mock(.demo))
        }
        await hold.waitUntilHeld()
        await connect.load(service: mock, binary: MockService.binaryPath, dataMode: .mock(.demo))
        await hold.release()
        await stale.value
        #expect(connect.data?.temporaryLocation == nil)
        #expect(connect.reloadFailure == nil)

        // Mock → live, and live fails: the mock's steps would be wrong, so the error state shows
        // instead of them.
        let failing = FixtureService(base: mock, failing: [.clientConfigs])
        await connect.load(service: failing, binary: "/Applications/PageLamp Preview.app/Contents/MacOS/pagelamp", dataMode: .live)
        guard case .failed = connect.phase else {
            Issue.record("expected the error state, got \(connect.phase)")
            return
        }
        #expect(connect.reloadFailure == nil)
    }
}

