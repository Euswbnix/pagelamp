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

/// Settings ▸ Help's file reader lines (the desktop's `FileReaderStatus`): nothing while the
/// reader works and every file could be read; else a warning (often security software blocks
/// it) and the files that couldn't be read, counted by why.
public struct FileReaderNotice: Equatable, Sendable {
    /// `settings.help.fileReader.blocked` or `.mismatch`; nil while the reader works.
    public var warningKey: String?
    /// Why files couldn't be read, with their counts (none with 0), in the report's order.
    public var unreadable: [UnreadableFiles]

    /// Nil when there is nothing to say.
    public init?(_ report: DoctorReport) {
        let unreadable = report.unreadableFiles.filter { $0.count > 0 }
        let warningKey: String? = switch report.extractWorker.status {
        case .ok: nil
        case .protocolMismatch: "settings.help.fileReader.mismatch"
        case .notSet, .spawnFailed, .failed: "settings.help.fileReader.blocked"
        }
        guard warningKey != nil || !unreadable.isEmpty else { return nil }
        self.warningKey = warningKey
        self.unreadable = unreadable
    }

    /// "Files PageLamp couldn't read: 2 took too long · 1 crashed the reader", or nil.
    public func unreadableLine(l10n: L10n) -> String? {
        guard !unreadable.isEmpty else { return nil }
        let list = unreadable
            .map { l10n(Self.key($0.kind), ["count": l10n.number($0.count)]) }
            .joined(separator: l10n("settings.help.fileReader.separator"))
        return l10n("settings.help.fileReader.unreadable", ["list": list])
    }

    /// `settings.help.fileReader.kinds.<kind>` (the facade's snake_case names).
    public static func key(_ kind: TextErrorKind) -> String {
        let name = switch kind {
        case .timedOut: "timed_out"
        case .cpuLimit: "cpu_limit"
        case .memoryLimit: "memory_limit"
        case .crashed: "crashed"
        case .badOutput: "bad_output"
        case .spawnFailed: "spawn_failed"
        case .protocolMismatch: "protocol_mismatch"
        }
        return "settings.help.fileReader.kinds.\(name)"
    }
}

/// The core's health report for Settings: the file reader lines (Help), and the version, the
/// data folder and the counts when `status()` is unavailable (S2). Works without the database.
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
