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
    /// This locale's plain strings, resolved once (see `StringsCache`).
    private let strings: [String: String]
    /// This locale's .lproj as a bundle, for plural keys (Foundation caches its tables).
    private let pluralBundle: Bundle?

    public init(locale: Locale, table: StringTable) {
        self.locale = locale
        self.table = table
        self.strings = StringsCache.strings(for: table, locale: locale)
        self.pluralBundle = StringsCache.localizationBundle(for: table, locale: locale)
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
        let pattern = lookUp(key)
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

    /// The pattern for `key` in this locale. Plain keys come from the in-memory table
    /// (`StringsCache`); plural keys, and any key the table lacks, go through Foundation so the
    /// .stringsdict rules and the development-language fallback still apply.
    private func lookUp(_ key: String) -> String {
        check(key)
        if !table.plural.contains(key) {
            if let pattern = strings[key] { return pattern }
        } else if let bundle = pluralBundle {
            // The .stringsdict format: String(format:) picks the plural form for the count.
            let pattern = bundle.localizedString(forKey: key, value: nil, table: table.tableName)
            if pattern != key { return pattern }
        }
        return String(localized: resource(key))
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

/// The generated .strings tables, loaded once per resource bundle, table and locale.
///
/// `String(localized:)` with a `.atURL` bundle reloads and re-parses the whole table on every
/// call: a sampled page switch spent about half of the main thread's busy time there, and every
/// page makes hundreds of lookups. This keeps each table in memory for the life of the process.
enum StringsCache {
    private static let tables = Mutex<[String: [String: String]]>([:])

    private static let bundles = Mutex<[String: Bundle]>([:])

    /// The localization of `table` that `locale` resolves to (e.g. "zh-Hans" for zh-CN).
    static func localization(for table: StringTable, locale: Locale) -> String {
        let available = Bundle(url: table.bundleURL)?.localizations ?? []
        return Bundle.preferredLocalizations(from: available, forPreferences: [locale.identifier]).first ?? "en"
    }

    /// That localization's .lproj folder as a bundle (plural keys read its .stringsdict).
    static func localizationBundle(for table: StringTable, locale: Locale) -> Bundle? {
        let key = "\(table.bundleURL.path)|\(locale.identifier)"
        if let cached = bundles.withLock({ $0[key] }) { return cached }
        let localization = localization(for: table, locale: locale)
        guard let url = Bundle(url: table.bundleURL)?.url(forResource: localization, withExtension: "lproj"),
              let bundle = Bundle(url: url) else { return nil }
        bundles.withLock { $0[key] = bundle }
        return bundle
    }

    static func strings(for table: StringTable, locale: Locale) -> [String: String] {
        let key = "\(table.bundleURL.path)|\(table.tableName)|\(locale.identifier)"
        if let cached = tables.withLock({ $0[key] }) { return cached }
        let bundle = Bundle(url: table.bundleURL)
        let available = bundle?.localizations ?? []
        let localization = Bundle.preferredLocalizations(from: available, forPreferences: [locale.identifier]).first ?? "en"
        // Ask the bundle where the table is (a macOS resource bundle keeps it in Contents/Resources).
        let url = bundle?.url(forResource: table.tableName, withExtension: "strings", subdirectory: nil, localization: localization)
        let loaded = url.flatMap { NSDictionary(contentsOf: $0) as? [String: String] } ?? [:]
        assert(!loaded.isEmpty, "No \(table.tableName).strings for \(localization) in \(table.bundleURL.path)")
        tables.withLock { $0[key] = loaded }
        return loaded
    }
}
