// Settings basics (spec §3.5, M1): the language and appearance choices, what the Data tab lists,
// and the core's health report for when the data can't be opened (S2) or the version is needed.

import Foundation
import Observation
import PageLampKit

extension AppLanguage {
    /// The choice's name in the picker: "System (English)" / "跟随系统（简体中文）", "English",
    /// "简体中文" (each language named in itself). `preferredLanguages` are the system's.
    public func title(l10n: L10n, preferredLanguages: [String]) -> String {
        switch self {
        case .english: l10n("common.language.en")
        case .simplifiedChinese: l10n("common.language.zh-CN")
        case .system:
            l10n("mac.settings.language.system", [
                "language": AppLanguage.system.localization(preferredLanguages: preferredLanguages) == "zh-Hans"
                    ? l10n("common.language.zh-CN") : l10n("common.language.en"),
            ])
        }
    }
}

extension AppAppearance {
    public func title(l10n: L10n) -> String {
        switch self {
        case .system: l10n("settings.appearance.themeOption.system")
        case .light: l10n("settings.appearance.themeOption.light")
        case .dark: l10n("settings.appearance.themeOption.dark")
        }
    }
}

extension AppModel {
    /// The menus still speak the launch language while the content speaks another: Settings
    /// offers Reopen Now (spec §3.5, §7.3).
    public var menusNeedReopen: Bool {
        menuL10n.locale.language.languageCode != l10n.locale.language.languageCode
    }
}

/// One line of Settings ▸ Data ▸ What's stored.
public struct StoredCount: Equatable, Sendable, Identifiable {
    /// The label's string key (`settings.data.counts.*`).
    public var labelKey: String
    public var value: UInt32
    public var id: String { labelKey }

    /// The counts the Tauri app shows, in its order (`chunks` and `modules` are internal).
    /// "Readable by your AI app" is `indexed_materials`.
    public static func rows(_ counts: StoreCounts) -> [StoredCount] {
        [
            StoredCount(labelKey: "settings.data.counts.courses", value: counts.courses),
            StoredCount(labelKey: "settings.data.counts.hiddenCourses", value: counts.hiddenCourses),
            StoredCount(labelKey: "settings.data.counts.materials", value: counts.materials),
            StoredCount(labelKey: "settings.data.counts.readable", value: counts.indexedMaterials),
            StoredCount(labelKey: "settings.data.counts.events", value: counts.events),
            StoredCount(labelKey: "settings.data.counts.studyPlans", value: counts.studyPlans),
        ]
    }

    /// When only `doctor()` answered (S2): the counts it has.
    public static func rows(_ report: DoctorReport) -> [StoredCount] {
        [
            StoredCount(labelKey: "settings.data.counts.courses", value: report.courses),
            StoredCount(labelKey: "settings.data.counts.hiddenCourses", value: report.hiddenCourses),
            StoredCount(labelKey: "settings.data.counts.materials", value: report.materials),
            StoredCount(labelKey: "settings.data.counts.events", value: report.events),
        ]
    }
}

/// The core's health report for Settings: the version when `status()` is unavailable, and the
/// data folder in S2. Loaded on demand; works without the database.
@Observable @MainActor
public final class SettingsModel {
    public private(set) var doctor: DoctorReport?
    public private(set) var doctorFailed = false

    public init(doctor: DoctorReport? = nil) {
        self.doctor = doctor
    }

    public func loadDoctor(_ service: any PageLampService) async {
        do throws(PageLampFailure) {
            doctor = try await service.doctor()
            doctorFailed = false
        } catch {
            doctorFailed = true
        }
    }

    /// `status().version`, else `doctor().version` (spec §3.5 About).
    public func version(status: AppStatus?) -> String? {
        status?.version ?? doctor?.version
    }
}
