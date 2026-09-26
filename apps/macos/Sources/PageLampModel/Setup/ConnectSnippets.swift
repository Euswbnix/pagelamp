// Connect your AI app (spec §3.4): what can be derived from the core's config snippets. A port of
// the Tauri app's apps/desktop/src/features/connect/{snippet,order}.ts and QuarantineHint.tsx,
// kept until the facade returns the variants itself (spec §13 #5).

import Foundation
import PageLampKit

public enum ConnectSnippets {
    /// The one server of a `{ "mcpServers": { "<key>": { … } } }` snippet, or nil when the
    /// snippet doesn't have exactly that shape.
    private static func server(_ content: String) -> (key: String, value: OrderedJSON)? {
        guard let servers = OrderedJSON(parsing: content)?["mcpServers"]?.members, servers.count == 1 else {
            return nil
        }
        return servers[0]
    }

    /// `"<key>": { … }`: what to add inside a config file's existing `mcpServers` section.
    public static func mcpServerEntry(_ content: String) -> String? {
        guard let (key, value) = server(content) else { return nil }
        return "\(OrderedJSON.quote(key)): \(value.pretty())"
    }

    /// The server's key ("pagelamp").
    public static func mcpServerName(_ content: String) -> String? {
        server(content)?.key
    }

    /// `"mcpServers": { "<key>": { … } }`: what to add as a top-level key when the file has
    /// other settings (Claude Desktop writes "preferences" itself) but no `mcpServers` yet.
    public static func mcpServersKey(_ content: String) -> String? {
        guard let (key, value) = server(content) else { return nil }
        return "\"mcpServers\": \(OrderedJSON.object([(key, value)]).pretty())"
    }

    /// The table a TOML snippet defines ("mcp_servers.pagelamp"), from its first `[…]` line.
    public static func tomlTable(_ content: String) -> String? {
        // Like /^\[([^\]\s]+)\]\s*$/m in the Tauri app.
        for line in content.split(separator: "\n", omittingEmptySubsequences: false) {
            if let match = line.wholeMatch(of: #/\[([^\]\s]+)\]\s*/#) {
                return String(match.1)
            }
        }
        return nil
    }

    /// The command that undoes a `claude mcp add … <name> -- <command>` snippet, with the same
    /// scope (running `add` again fails while a server with that name exists).
    public static func claudeMcpRemove(_ content: String) -> String? {
        guard let separator = content.range(of: " -- ") else { return nil }
        let head = content[..<separator.lowerBound]
        guard head.hasPrefix("claude mcp add ") else { return nil }
        let words = head.split(whereSeparator: \.isWhitespace)
        guard let name = words.last, !name.hasPrefix("-") else { return nil }
        var scope: Substring?
        if let flag = words.firstIndex(of: "--scope"), words.index(after: flag) < words.endIndex {
            scope = words[words.index(after: flag)]
        }
        return "claude mcp remove" + (scope.map { " --scope \($0)" } ?? "") + " \(name)"
    }

    /// Clients in the order the page offers them: Claude Desktop (works on every Claude plan,
    /// no terminal), Claude Code, Codex, then everything else in the core's order.
    public static func sorted(_ configs: [McpClientConfig]) -> [McpClientConfig] {
        configs.enumerated()
            .sorted { lhs, rhs in
                let (a, b) = (rank(lhs.element.client), rank(rhs.element.client))
                return a == b ? lhs.offset < rhs.offset : a < b
            }
            .map(\.element)
    }

    private static func rank(_ client: McpClient) -> Int {
        switch client {
        case .claudeDesktop: 0
        case .claudeCode: 1
        case .codex: 2
        case .generic: 3
        }
    }

    /// Where every AI app would launch PageLamp from, when that place won't last (S15). All
    /// configs carry the same launch; the first that says so decides.
    public static func temporaryLocation(_ configs: [McpClientConfig]) -> TemporaryLocation? {
        configs.lazy.compactMap(\.launch.temporaryLocation).first
    }
}

/// The macOS quarantine fallback (Connect ▸ "If macOS blocks PageLamp"): the beta is only
/// ad-hoc signed, so the bundled `pagelamp` may be blocked when an AI app starts it.
public enum QuarantineHint {
    /// What to clear the flag on: the whole `.app` when the binary is inside one, else the binary.
    public static func target(command: String) -> (path: String, recursive: Bool) {
        if let match = command.wholeMatch(of: #/(.+?\.app)\/Contents\/MacOS\/[^\/]+/#) {
            return (String(match.1), true)
        }
        return (command, false)
    }

    /// `xattr -d[r] com.apple.quarantine '<path>'`, the path quoted as one POSIX shell word.
    public static func command(for launchCommand: String) -> String {
        let target = target(command: launchCommand)
        return "xattr -d\(target.recursive ? "r" : "") com.apple.quarantine \(shellQuote(target.path))"
    }

    /// Single quotes, with any ' written as '\''.
    public static func shellQuote(_ text: String) -> String {
        "'" + text.replacingOccurrences(of: "'", with: #"'\''"#) + "'"
    }
}
