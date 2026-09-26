// The app's strings in the language the student picked (spec §7.3).
//
// Strings come from the generated Localizable.strings / .stringsdict (scripts/gen-strings.mjs) in
// the PageLamp target's resource bundle; `StringTable.app` (PageLamp target) describes them.
// Views never contain English literals: they call `l10n("some.key")`.

import Foundation
import Synchronization

/// Where the generated strings live, and what the generator says about each key.
public struct StringTable: Sendable {
    /// The resource bundle holding `en.lproj` / `zh-Hans.lproj`.
    public let bundleURL: URL
    /// Every key (L10nKeys.all).
    public let keys: Set<String>
    /// Keys whose text lives in Localizable.stringsdict (take `count` as an Int).
    public let plural: Set<String>
    /// Argument names per key, in `%N$` order (L10nKeys.arguments).
    public let arguments: [String: [String]]
    /// The .strings table name.
    public let tableName: String

    public init(
        bundleURL: URL,
        keys: Set<String>,
        plural: Set<String>,
        arguments: [String: [String]],
        tableName: String = "Localizable"
    ) {
        self.bundleURL = bundleURL
        self.keys = keys
        self.plural = plural
        self.arguments = arguments
        self.tableName = tableName
    }
}

/// Looks up and formats strings for one locale.
///
/// ```swift
/// l10n("mac.nav.thisWeek")                                        // "This Week" / "本周"
/// l10n("mac.capsule.syncing", ["done": "2", "total": "3", "source": label])
/// l10n.plural("mac.thisWeek.deadlineCount", count: 3)             // "3 deadlines"
/// l10n.plural("common.aiMaterials.readable", count: 14, ["indexed": "12"])
/// ```
///
/// Arguments are named like the i18next placeholders (`{{source}}`) and passed as Strings
/// (format numbers yourself, e.g. `n.formatted(.number.locale(l10n.locale))`); a plural's
/// `count` is the Int passed to `plural(_:count:_:)`. In debug builds an unknown key or a missing
/// argument is an assertion failure.
public struct L10n: Sendable {
    public let locale: Locale
    public let table: StringTable

    public init(locale: Locale, table: StringTable) {
        self.locale = locale
        self.table = table
    }

    /// The text of `key`, formatted with `arguments` if the key takes any.
    public func callAsFunction(_ key: String, _ arguments: [String: String] = [:]) -> String {
        format(key, arguments: arguments, count: nil)
    }

    /// A plural key (Localizable.stringsdict): `count` picks the form.
    public func plural(_ key: String, count: Int, _ arguments: [String: String] = [:]) -> String {
        assert(table.plural.contains(key), "\(key) is not a plural key")
        return format(key, arguments: arguments, count: count)
    }

    /// The unformatted key as a resource with this locale (for APIs that take one).
    public func resource(_ key: String) -> LocalizedStringResource {
        check(key)
        return LocalizedStringResource(
            String.LocalizationValue(key),
            table: table.tableName,
            locale: locale,
            bundle: .atURL(table.bundleURL)
        )
    }

    /// Whether `key` exists (for keys built from backend codes).
    public func has(_ key: String) -> Bool {
        table.keys.contains(key)
    }

    // MARK: - Formatting

    private func format(_ key: String, arguments: [String: String], count: Int?) -> String {
        let pattern = String(localized: resource(key))
        let names = table.arguments[key] ?? []
        guard !names.isEmpty else { return pattern }
        let isPlural = table.plural.contains(key)
        var values: [any CVarArg] = []
        values.reserveCapacity(names.count)
        for name in names {
            if isPlural, name == "count" {
                assert(count != nil, "\(key) needs a count: use l10n.plural(_:count:)")
                values.append(count ?? 0)
            } else {
                assert(arguments[name] != nil, "\(key) needs the argument \"\(name)\"")
                values.append(arguments[name] ?? "")
            }
        }
        return String(format: pattern, locale: locale, arguments: values)
    }

    private func check(_ key: String) {
        assert(table.keys.contains(key), "Unknown string key \"\(key)\" (not in L10nKeys.all)")
    }
}

extension L10n {
    /// The strings of the app's current language, for code without a model at hand (for
    /// example `LocalizedError.errorDescription`). AppModel keeps it up to date.
    public static var current: L10n? {
        get { shared.withLock { $0 } }
        set { shared.withLock { $0 = newValue } }
    }

    private static let shared = Mutex<L10n?>(nil)
}
