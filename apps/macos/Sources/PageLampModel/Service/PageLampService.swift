// What the Mac UI asks of PageLamp's core: every facade call the M1 screens need (spec §2.8),
// behind one protocol so the preview can run on synthetic data (MockService) or the real facade
// (LiveService).

import Foundation
import PageLampKit

/// PageLamp's core as the UI sees it. Every call is async and throws `PageLampFailure`.
///
/// Implementations: `LiveService` (the Rust facade through UniFFI), `MockService` (synthetic
/// demo data, the preview's default) and `UnavailableService` (the facade could not open; only
/// diagnostics work, spec S2).
///
/// Screens load their own data through `AppModel.service`; the shell data (status, courses,
/// sources, the This Week inputs) is loaded and kept by `AppModel`.
public protocol PageLampService: Sendable {
    // Shell (spec §2.8)
    func status() async throws(PageLampFailure) -> AppStatus
    /// All courses, hidden ones included (check `course.hidden`).
    func listCourses() async throws(PageLampFailure) -> [CourseSummary]
    func listSources() async throws(PageLampFailure) -> [SourceRecord]

    // This Week and course detail
    /// With a course: its events. Without: every visible course's events plus unlinked events.
    func listDeadlines(course: String?, daysAhead: UInt32, daysBack: UInt32) async throws(PageLampFailure) -> [Deadline]
    func latestStudyPlan() async throws(PageLampFailure) -> StoredStudyPlan?
    /// `course` is an id or a code.
    func courseOverview(course: String) async throws(PageLampFailure) -> CourseOverview
    /// The materials of `week` (nil: the current week).
    func weekMaterials(course: String, week: UInt32?) async throws(PageLampFailure) -> WeekMaterials

    // Course weeks, phases and the Past group (v0.3 course lane; `course` is an id or a code)
    /// Where one course is: week, phase, the dates used and not used, evidence.
    func courseTimeline(course: String) async throws(PageLampFailure) -> CourseTimeline
    /// Every course's lifecycle, the removal suggestions and whether the banner shows.
    func lifecycleSummary() async throws(PageLampFailure) -> LifecycleSummary
    /// "I'm still taking this" until `until` ("YYYY-MM-DD"; nil: the end of the course's term
    /// when that is ahead, else today + `keepCurrentDays()`).
    func keepCourseCurrent(course: String, until: String?) async throws(PageLampFailure) -> Course
    func clearKeepCourseCurrent(course: String) async throws(PageLampFailure) -> Course
    /// "Not now" (`notNowDays()`) or "Keep" (never again) on these courses' removal suggestion.
    func snoozeRemovalSuggestions(courses: [String], kind: SnoozeKind) async throws(PageLampFailure)
    func clearRemovalSnooze(courses: [String]) async throws(PageLampFailure)
    /// "Not now" on the "N courses look finished" banner.
    func snoozeLifecycleBanner() async throws(PageLampFailure)
    /// "These dates are right" for dates set in PageLamp 0.1.
    func confirmCourseDates(course: String) async throws(PageLampFailure) -> CourseTimeline

    // Updates and launch. Sparkle installs; the facade decides What's new and when to check.
    /// What's new (upgraders), whether the update check is due, the version this launch
    /// updated from. `now` is the caller's clock (the app asks again on a timer).
    func startupTasks(now: Date) async throws(PageLampFailure) -> StartupTasks
    func updatePrefs() async throws(PageLampFailure) -> UpdatePrefs
    func setUpdatePrefs(prefs: UpdatePrefs) async throws(PageLampFailure)
    /// The student's channel, else Beta for a pre-release build and Stable otherwise.
    func effectiveUpdateChannel() async throws(PageLampFailure) -> UpdateChannel
    func acknowledgeWhatsNew() async throws(PageLampFailure)
    /// The student saw what the update check sends.
    func acknowledgeUpdateDisclosure() async throws(PageLampFailure)
    func recordUpdateCheck(record: UpdateCheckRecord) async throws(PageLampFailure)
    func lastUpdateCheck() async throws(PageLampFailure) -> UpdateCheckRecord?
    /// Syncs and downloads running in this app, and whether another process syncs (ask before
    /// installing an update).
    func activity() async throws(PageLampFailure) -> Activity

    // Sources & Sync. Events go to `observer` (see `SyncEventStream`) while the call runs.
    /// Syncs every source; `.busy` when another sync runs. A failing source is reported in the
    /// summary (`ok: false`), not thrown.
    func syncAll(request: SyncRequest, observer: any SyncObserver) async throws(PageLampFailure) -> SyncSummary
    func syncSource(sourceId: String, request: SyncRequest, observer: any SyncObserver) async throws(PageLampFailure) -> SourceSyncResult

    // Connect your AI app (never writes an AI app's config)
    /// `pagelampBinary`: the bundled CLI (`AppModel.sidecarPath`).
    func mcpClientConfigs(pagelampBinary: String) async throws(PageLampFailure) -> [McpClientConfig]
    func mcpLaunch(pagelampBinary: String) async throws(PageLampFailure) -> McpLaunch
    /// Which AI apps are configured (`mcpClients`), keychain and database health, the version.
    func doctor() async throws(PageLampFailure) -> DoctorReport

    // Help and diagnostics (work in S2 too)
    /// Markdown, redacted and pseudonymised; always shown to the student before it is copied.
    func diagnosticReport() async throws(PageLampFailure) -> String
    /// The logs folder, created if missing.
    func logsDir() async throws(PageLampFailure) -> String
    func lastCrash() async throws(PageLampFailure) -> CrashReport?
    func clearLastCrash() async throws(PageLampFailure)
}
