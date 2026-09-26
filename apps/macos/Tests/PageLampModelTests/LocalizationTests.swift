// The language setting and the L10n helper over the generated string tables.

import Foundation
import PageLamp
import PageLampKit
import PageLampModel
import Testing

@Suite("Language and strings") @MainActor
struct LocalizationTests {
    @Test("switching the language switches every string live and persists the choice")
    func languageSwitch() {
        let settings = InMemorySettingsStore()
        let (model, _) = makeModel(preferredLanguages: ["zh-Hans-CN", "en-US"], settings: settings)
        // System: the system prefers Chinese.
        #expect(model.language == .system)
        #expect(model.localization == "zh-Hans")
        #expect(model.l10n("mac.nav.thisWeek") == "本周")
        #expect(model.locale.language.languageCode == .chinese)

        model.language = .english
        #expect(settings.language == .english)
        #expect(model.localization == "en")
        #expect(model.l10n("mac.nav.thisWeek") == "This Week")
        #expect(model.l10n.plural("mac.thisWeek.deadlineCount", count: 1) == "1 deadline")
        #expect(model.l10n.plural("mac.thisWeek.deadlineCount", count: 3) == "3 deadlines")
        #expect(L10n.current != nil)

        model.language = .simplifiedChinese
        #expect(settings.language == .simplifiedChinese)
        #expect(model.l10n("mac.nav.thisWeek") == "本周")
        #expect(model.l10n.plural("mac.thisWeek.deadlineCount", count: 3) == "3 个截止日期")
        #expect(model.l10n("mac.capsule.syncing", ["done": "2", "total": "3", "source": "Canvas"]) == "正在同步 2/3 · Canvas")

        // Menus keep the launch language (like the system's own items) until Reopen Now.
        #expect(model.menuL10n("mac.nav.thisWeek") == "本周")
        model.language = .english
        #expect(model.menuL10n("mac.nav.thisWeek") == "本周")
        model.language = .simplifiedChinese
        model.applyLanguageToMenus()
        #expect(settings.appleLanguages == ["zh-Hans"])
        model.language = .system
        model.applyLanguageToMenus()
        #expect(settings.appleLanguages == nil)
    }

    @Test("the saved language is restored, and an unsupported system language falls back to English")
    func restoreAndFallback() {
        let (saved, _) = makeModel(settings: InMemorySettingsStore(language: .simplifiedChinese))
        #expect(saved.language == .simplifiedChinese)
        #expect(saved.l10n("common.sync.done") == "同步完成")

        let (french, _) = makeModel(language: .system, preferredLanguages: ["fr-FR"])
        #expect(french.localization == "en")
        #expect(french.l10n("common.sync.done") == "Sync finished")
    }

    @Test("arguments are placed by name, in each language's order")
    func arguments() {
        let english = L10n(locale: Locale(identifier: "en_US"), table: .app)
        let chinese = L10n(locale: Locale(identifier: "zh-Hans_CN"), table: .app)
        #expect(english.plural("common.aiMaterials.readable", count: 14, ["indexed": "12"]) == "12 of 14 materials readable by your AI app")
        #expect(chinese.plural("common.aiMaterials.readable", count: 14, ["indexed": "12"]) == "共 14 份资料，AI 应用可读取 12 份")
        #expect(english("mac.capsule.needsToken", ["source": "Demo Canvas"]) == "Demo Canvas needs a new token")
        #expect(english("mac.errors.schema").contains("PageLamp"))
        #expect(english.resource("mac.nav.sources").locale == Locale(identifier: "en_US"))
    }

    @Test("every key resolves in both languages")
    func everyKeyResolves() {
        for identifier in ["en_US", "zh-Hans_CN"] {
            let l10n = L10n(locale: Locale(identifier: identifier), table: .app)
            for key in StringTable.app.keys where StringTable.app.arguments[key] == nil && !StringTable.app.plural.contains(key) {
                let text = l10n(key)
                #expect(!text.isEmpty && text != key, "\(identifier): \(key) did not resolve")
            }
        }
    }

    @Test("every failure kind has an explanation in both languages")
    func failureDescriptions() {
        let chinese = L10n(locale: Locale(identifier: "zh-Hans_CN"), table: .app)
        for kind in PageLampFailure.Kind.allCases {
            let failure = PageLampFailure(kind: kind, message: "backend text")
            #expect(StringTable.app.keys.contains(failure.descriptionKey))
            #expect(failure.localizedDescription(in: chinese) != failure.descriptionKey)
        }
        #expect(PageLampFailure(kind: .busy, message: "x").localizedDescription(in: chinese) == chinese("common.errors.busy"))
    }
}

/// Regression tests for the page-switch jank: every lookup used to reload and re-parse the
/// whole .strings table through `String(localized:)` with an `.atURL` bundle.
@Suite("String table cache") @MainActor
struct StringTableCacheTests {
    private static let plainKeys: [String] = StringTable.app.keys
        .subtracting(StringTable.app.plural)
        .filter { (StringTable.app.arguments[$0] ?? []).isEmpty }
        .sorted()

    @Test("the cached table gives exactly Foundation's text for every plain key, in both languages")
    func cacheMatchesFoundation() {
        for identifier in ["en", "zh-Hans"] {
            let l10n = L10n(locale: Locale(identifier: identifier), table: .app)
            for key in Self.plainKeys {
                #expect(l10n(key) == String(localized: l10n.resource(key)), "\(identifier): \(key)")
            }
        }
    }

    @Test("plural keys give exactly Foundation's text for every count form, in both languages")
    func pluralsMatchFoundation() {
        for identifier in ["en", "zh-Hans"] {
            let l10n = L10n(locale: Locale(identifier: identifier), table: .app)
            for key in StringTable.app.plural.sorted() {
                let names = StringTable.app.arguments[key] ?? []
                for count in [0, 1, 2, 5, 21] {
                    var args: [String: String] = [:]
                    for name in names where name != "count" { args[name] = "X" }
                    let pattern = String(localized: l10n.resource(key))
                    let values: [any CVarArg] = names.map { name -> any CVarArg in name == "count" ? count as any CVarArg : "X" as any CVarArg }
                    let expected = String(format: pattern, locale: l10n.locale, arguments: values)
                    #expect(l10n.plural(key, count: count, args) == expected, "\(identifier): \(key) × \(count)")
                }
            }
        }
    }

    @Test("lookups don't reload the table (5,000 lookups stay far below a frame budget per page)")
    func lookupsAreCached() {
        let l10n = L10n(locale: Locale(identifier: "zh-Hans"), table: .app)
        let keys = Array(Self.plainKeys.prefix(500))
        let elapsed = ContinuousClock().measure {
            for _ in 0..<10 { for key in keys { _ = l10n(key) } }
        }
        // Cached: a few ms. Re-parsing the table per lookup took well over a second here.
        #expect(elapsed < .milliseconds(250), "5,000 lookups took \(elapsed)")
    }
}
