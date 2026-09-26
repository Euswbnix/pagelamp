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
    public static func openDefault() async throws(PageLampFailure) -> LiveService {
        try startDiagnostics()
        do {
            return LiveService(core: try await PageLamp.open(dataDir: nil))
        } catch {
            throw PageLampFailure.from(error)
        }
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
