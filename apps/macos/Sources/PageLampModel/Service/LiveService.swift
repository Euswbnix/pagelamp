// The real PageLamp core: the Rust facade through the UniFFI bindings (PageLampKit).

import Foundation
import PageLampKit
import Synchronization

/// `PageLampService` over the Rust facade. Every call runs on the core's own worker threads
/// (`spawn_blocking` behind UniFFI's async), so the main actor never blocks on the database.
///
/// Opening it with `openDefault()` uses the **real** data folder and keychain, shared with the
/// installed PageLamp app and its CLI. The preview app reaches it only from the Debug menu after
/// a confirmation; tests build it with `init(core:)` over `PageLamp.openWithMemorySecrets` in a
/// temp folder.
public final class LiveService: PageLampService {
    public let core: PageLamp

    public init(core: PageLamp) {
        self.core = core
    }

    /// Opens the default data folder (`~/Library/Application Support/dev.PageLamp.PageLamp`) with
    /// the keychain, after starting the core's diagnostics (logs, panic hook) once per process.
    /// Syncs read files in the bundled `pagelamp` executable's resource-limited worker
    /// (`bundledExtractWorker`); without one (`swift run`) they read them in this process.
    public static func openDefault() async throws(PageLampFailure) -> LiveService {
        try startDiagnostics()
        do {
            let core = try await PageLamp.open(dataDir: nil)
            try await core.setExtractWorker(path: bundledExtractWorker)
            return LiveService(core: core)
        } catch {
            throw PageLampFailure.from(error)
        }
    }

    /// The bundled CLI (`Contents/MacOS/pagelamp`), which runs `pagelamp extract-worker`.
    public static var bundledExtractWorker: String? {
        Bundle.main.url(forAuxiliaryExecutable: "pagelamp")?.path(percentEncoded: false)
    }

    /// `initDiagnostics` for the default data folder; later calls do nothing (like the core's).
    public static func startDiagnostics() throws(PageLampFailure) {
        let first = diagnosticsStarted.withLock { started in
            defer { started = true }
            return !started
        }
        guard first else { return }
        do {
            try initDiagnostics(verbose: false)
        } catch {
            throw PageLampFailure.from(error)
        }
    }

    private static let diagnosticsStarted = Mutex(false)

    /// Runs one facade call and maps its error.
    private func call<T>(_ body: () async throws -> T) async throws(PageLampFailure) -> T {
        do {
            return try await body()
        } catch {
            throw PageLampFailure.from(error)
        }
    }

    public func status() async throws(PageLampFailure) -> AppStatus {
        try await call { try await core.status() }
    }

    public func listCourses() async throws(PageLampFailure) -> [CourseSummary] {
        try await call { try await core.listCourses() }
    }

    public func listSources() async throws(PageLampFailure) -> [SourceRecord] {
        try await call { try await core.listSources() }
    }

    public func listDeadlines(course: String?, daysAhead: UInt32, daysBack: UInt32) async throws(PageLampFailure) -> [Deadline] {
        try await call { try await core.listDeadlines(course: course, daysAhead: daysAhead, daysBack: daysBack) }
    }

    public func latestStudyPlan() async throws(PageLampFailure) -> StoredStudyPlan? {
        try await call { try await core.latestStudyPlan() }
    }

    public func courseOverview(course: String) async throws(PageLampFailure) -> CourseOverview {
        try await call { try await core.courseOverview(course: course) }
    }

    public func weekMaterials(course: String, week: UInt32?) async throws(PageLampFailure) -> WeekMaterials {
        try await call { try await core.weekMaterials(course: course, week: week) }
    }

    public func courseTimeline(course: String) async throws(PageLampFailure) -> CourseTimeline {
        try await call { try await core.courseTimeline(course: course) }
    }

    public func lifecycleSummary() async throws(PageLampFailure) -> LifecycleSummary {
        try await call { try await core.lifecycleSummary() }
    }

    public func keepCourseCurrent(course: String, until: String?) async throws(PageLampFailure) -> Course {
        try await call { try await core.keepCourseCurrent(course: course, until: until) }
    }

    public func clearKeepCourseCurrent(course: String) async throws(PageLampFailure) -> Course {
        try await call { try await core.clearKeepCourseCurrent(course: course) }
    }

    public func snoozeRemovalSuggestions(courses: [String], kind: SnoozeKind) async throws(PageLampFailure) {
        try await call { try await core.snoozeRemovalSuggestions(courses: courses, kind: kind) }
    }

    public func clearRemovalSnooze(courses: [String]) async throws(PageLampFailure) {
        try await call { try await core.clearRemovalSnooze(courses: courses) }
    }

    public func snoozeLifecycleBanner() async throws(PageLampFailure) {
        try await call { try await core.snoozeLifecycleBanner() }
    }

    public func confirmCourseDates(course: String) async throws(PageLampFailure) -> CourseTimeline {
        try await call { try await core.confirmCourseDates(course: course) }
    }

    public func startupTasks(now: Date) async throws(PageLampFailure) -> StartupTasks {
        try await call { try await core.startupTasks(now: now) }
    }

    public func updatePrefs() async throws(PageLampFailure) -> UpdatePrefs {
        try await call { try await core.updatePrefs() }
    }

    public func setUpdatePrefs(prefs: UpdatePrefs) async throws(PageLampFailure) {
        try await call { try await core.setUpdatePrefs(prefs: prefs) }
    }

    public func effectiveUpdateChannel() async throws(PageLampFailure) -> UpdateChannel {
        try await call { try await core.effectiveUpdateChannel() }
    }

    public func acknowledgeWhatsNew() async throws(PageLampFailure) {
        try await call { try await core.acknowledgeWhatsNew() }
    }

    public func acknowledgeUpdateDisclosure() async throws(PageLampFailure) {
        try await call { try await core.acknowledgeUpdateDisclosure() }
    }

    public func recordUpdateCheck(record: UpdateCheckRecord) async throws(PageLampFailure) {
        try await call { try await core.recordUpdateCheck(record: record) }
    }

    public func lastUpdateCheck() async throws(PageLampFailure) -> UpdateCheckRecord? {
        try await call { try await core.lastUpdateCheck() }
    }

    public func activity() async throws(PageLampFailure) -> Activity {
        try await call { try await core.activity() }
    }

    public func syncAll(request: SyncRequest, observer: any SyncObserver) async throws(PageLampFailure) -> SyncSummary {
        try await call { try await core.syncAll(request: request, observer: observer) }
    }

    public func syncSource(sourceId: String, request: SyncRequest, observer: any SyncObserver) async throws(PageLampFailure) -> SourceSyncResult {
        try await call { try await core.syncSource(sourceId: sourceId, request: request, observer: observer) }
    }

    public func mcpClientConfigs(pagelampBinary: String) async throws(PageLampFailure) -> [McpClientConfig] {
        try await call { try await core.mcpClientConfigs(pagelampBinary: pagelampBinary) }
    }

    public func mcpLaunch(pagelampBinary: String) async throws(PageLampFailure) -> McpLaunch {
        try await call { try await core.mcpLaunch(pagelampBinary: pagelampBinary) }
    }

    public func doctor() async throws(PageLampFailure) -> DoctorReport {
        try await call { try await core.doctor() }
    }

    public func diagnosticReport() async throws(PageLampFailure) -> String {
        try await call { try await core.diagnosticReport() }
    }

    public func logsDir() async throws(PageLampFailure) -> String {
        try await call { try await core.logsDir() }
    }

    public func lastCrash() async throws(PageLampFailure) -> CrashReport? {
        try await call { try await core.lastCrash() }
    }

    public func clearLastCrash() async throws(PageLampFailure) {
        try await call { try await core.clearLastCrash() }
    }
}

/// The facade could not open (spec S2): data calls fail with the opening error; diagnostics
/// still work through the core's free functions, which use the default data folder.
public struct UnavailableService: PageLampService {
    public let failure: PageLampFailure

    public init(failure: PageLampFailure) {
        self.failure = failure
    }

    public func status() async throws(PageLampFailure) -> AppStatus { throw failure }
    public func listCourses() async throws(PageLampFailure) -> [CourseSummary] { throw failure }
    public func listSources() async throws(PageLampFailure) -> [SourceRecord] { throw failure }

    public func listDeadlines(course: String?, daysAhead: UInt32, daysBack: UInt32) async throws(PageLampFailure) -> [Deadline] {
        throw failure
    }

    public func latestStudyPlan() async throws(PageLampFailure) -> StoredStudyPlan? { throw failure }
    public func courseOverview(course: String) async throws(PageLampFailure) -> CourseOverview { throw failure }

    public func weekMaterials(course: String, week: UInt32?) async throws(PageLampFailure) -> WeekMaterials {
        throw failure
    }

    public func courseTimeline(course: String) async throws(PageLampFailure) -> CourseTimeline { throw failure }
    public func lifecycleSummary() async throws(PageLampFailure) -> LifecycleSummary { throw failure }

    public func keepCourseCurrent(course: String, until: String?) async throws(PageLampFailure) -> Course {
        throw failure
    }

    public func clearKeepCourseCurrent(course: String) async throws(PageLampFailure) -> Course { throw failure }
    public func snoozeRemovalSuggestions(courses: [String], kind: SnoozeKind) async throws(PageLampFailure) { throw failure }
    public func clearRemovalSnooze(courses: [String]) async throws(PageLampFailure) { throw failure }
    public func snoozeLifecycleBanner() async throws(PageLampFailure) { throw failure }
    public func confirmCourseDates(course: String) async throws(PageLampFailure) -> CourseTimeline { throw failure }

    public func startupTasks(now: Date) async throws(PageLampFailure) -> StartupTasks { throw failure }
    public func updatePrefs() async throws(PageLampFailure) -> UpdatePrefs { throw failure }
    public func setUpdatePrefs(prefs: UpdatePrefs) async throws(PageLampFailure) { throw failure }
    public func effectiveUpdateChannel() async throws(PageLampFailure) -> UpdateChannel { throw failure }
    public func acknowledgeWhatsNew() async throws(PageLampFailure) { throw failure }
    public func acknowledgeUpdateDisclosure() async throws(PageLampFailure) { throw failure }
    public func recordUpdateCheck(record: UpdateCheckRecord) async throws(PageLampFailure) { throw failure }
    public func lastUpdateCheck() async throws(PageLampFailure) -> UpdateCheckRecord? { throw failure }
    public func activity() async throws(PageLampFailure) -> Activity { throw failure }

    public func syncAll(request: SyncRequest, observer: any SyncObserver) async throws(PageLampFailure) -> SyncSummary {
        throw failure
    }

    public func syncSource(sourceId: String, request: SyncRequest, observer: any SyncObserver) async throws(PageLampFailure) -> SourceSyncResult {
        throw failure
    }

    public func mcpClientConfigs(pagelampBinary: String) async throws(PageLampFailure) -> [McpClientConfig] {
        throw failure
    }

    public func mcpLaunch(pagelampBinary: String) async throws(PageLampFailure) -> McpLaunch { throw failure }

    public func doctor() async throws(PageLampFailure) -> DoctorReport {
        do { return try await diagnosticsDoctor() } catch { throw PageLampFailure.from(error) }
    }

    public func diagnosticReport() async throws(PageLampFailure) -> String {
        do { return try await diagnosticsReport() } catch { throw PageLampFailure.from(error) }
    }

    public func logsDir() async throws(PageLampFailure) -> String {
        do { return try await diagnosticsLogsDir() } catch { throw PageLampFailure.from(error) }
    }

    public func lastCrash() async throws(PageLampFailure) -> CrashReport? {
        do { return try await diagnosticsLastCrash() } catch { throw PageLampFailure.from(error) }
    }

    public func clearLastCrash() async throws(PageLampFailure) {
        do { try await diagnosticsClearLastCrash() } catch { throw PageLampFailure.from(error) }
    }
}
