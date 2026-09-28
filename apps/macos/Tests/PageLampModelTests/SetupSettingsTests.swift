// Settings basics (spec §3.5): language names, Reopen Now, the Data tab's counts and the version
// fallback. Settings live in memory; nothing touches the user's preferences.

import Foundation
import PageLamp
import PageLampKit
import PageLampModel
import Testing

@Suite("Settings") @MainActor
struct SetupSettingsTests {
    @Test("each language is named in itself; System names what the system prefers")
    func languageTitles() {
        let (model, _) = makeModel(language: .english)
        let en = model.l10n
        #expect(AppLanguage.english.title(l10n: en, preferredLanguages: ["en-US"]) == "English")
        #expect(AppLanguage.simplifiedChinese.title(l10n: en, preferredLanguages: ["en-US"]) == "简体中文")
        #expect(AppLanguage.system.title(l10n: en, preferredLanguages: ["zh-Hans-CN", "en"]) == "System (简体中文)")
        #expect(AppLanguage.system.title(l10n: en, preferredLanguages: ["fr-FR", "en"]) == "System (English)")

        model.language = .simplifiedChinese
        let zh = model.l10n
        #expect(AppLanguage.system.title(l10n: zh, preferredLanguages: ["en-US"]) == "跟随系统（English）")
        #expect(AppAppearance.allCases.map { $0.title(l10n: zh) } == ["跟随系统", "浅色", "深色"])
        #expect(AppAppearance.allCases.map { $0.title(l10n: en) } == ["System", "Light", "Dark"])
    }

    @Test("Reopen Now is offered while the menus speak another language than the content")
    func reopen() {
        let settings = InMemorySettingsStore(language: .system)
        let (model, _) = makeModel(preferredLanguages: ["en-US"], settings: settings)
        #expect(!model.menusNeedReopen)
        model.language = .english
        #expect(!model.menusNeedReopen)
        model.language = .simplifiedChinese
        #expect(model.menusNeedReopen)
        model.applyLanguageToMenus()
        #expect(settings.appleLanguages == ["zh-Hans"])
        // Still needed until the app actually reopens (the menus keep the launch language).
        #expect(model.menusNeedReopen)
    }

    @Test("the Data tab lists the Tauri counts; readable is indexed_materials")
    func counts() async throws {
        let (model, mock) = makeModel(scenario: .demo)
        await model.refresh()
        let status = try #require(model.status)
        let rows = StoredCount.rows(status.counts)
        #expect(rows.map(\.labelKey) == [
            "settings.data.counts.courses", "settings.data.counts.hiddenCourses", "settings.data.counts.materials",
            "settings.data.counts.readable", "settings.data.counts.events", "settings.data.counts.studyPlans",
        ])
        #expect(rows[3].value == status.counts.indexedMaterials)
        #expect(rows.allSatisfy { model.l10n.has($0.labelKey) })

        let doctor = try await mock.doctor()
        #expect(StoredCount.rows(doctor).map(\.value) == [doctor.courses, doctor.hiddenCourses, doctor.materials, doctor.events])
    }

    @Test("the version comes from status(), else from doctor()")
    func version() async {
        let (model, mock) = makeModel(scenario: .demo)
        let settings = SettingsModel()
        #expect(settings.version(status: nil) == nil)
        await settings.loadDoctor(mock)
        #expect(!settings.doctorFailed)
        #expect(settings.version(status: nil) == "0.1.0-mock")
        await model.refresh()
        #expect(settings.version(status: model.status) == model.status?.version)
    }
}
