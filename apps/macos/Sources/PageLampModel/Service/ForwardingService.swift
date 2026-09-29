// A service that wraps another one (`FixtureService`, test doubles): every call goes to `base`
// unless the wrapper implements it itself, so wrappers write only what they change.

import Foundation
import PageLampKit

/// A `PageLampService` that forwards every call it doesn't implement to `base`.
public protocol ForwardingService: PageLampService {
    var base: any PageLampService { get }
}

extension ForwardingService {
    public func status() async throws(PageLampFailure) -> AppStatus {
        try await base.status()
    }

    public func listCourses() async throws(PageLampFailure) -> [CourseSummary] {
        try await base.listCourses()
    }

    public func listSources() async throws(PageLampFailure) -> [SourceRecord] {
        try await base.listSources()
    }

    public func listDeadlines(course: String?, daysAhead: UInt32, daysBack: UInt32) async throws(PageLampFailure) -> [Deadline] {
        try await base.listDeadlines(course: course, daysAhead: daysAhead, daysBack: daysBack)
    }

    public func latestStudyPlan() async throws(PageLampFailure) -> StoredStudyPlan? {
        try await base.latestStudyPlan()
    }

    public func courseOverview(course: String) async throws(PageLampFailure) -> CourseOverview {
        try await base.courseOverview(course: course)
    }

    public func weekMaterials(course: String, week: UInt32?) async throws(PageLampFailure) -> WeekMaterials {
        try await base.weekMaterials(course: course, week: week)
    }

    public func courseTimeline(course: String) async throws(PageLampFailure) -> CourseTimeline {
        try await base.courseTimeline(course: course)
    }

    public func lifecycleSummary() async throws(PageLampFailure) -> LifecycleSummary {
        try await base.lifecycleSummary()
    }

    public func keepCourseCurrent(course: String, until: String?) async throws(PageLampFailure) -> Course {
        try await base.keepCourseCurrent(course: course, until: until)
    }

    public func clearKeepCourseCurrent(course: String) async throws(PageLampFailure) -> Course {
        try await base.clearKeepCourseCurrent(course: course)
    }

    public func snoozeRemovalSuggestions(courses: [String], kind: SnoozeKind) async throws(PageLampFailure) {
        try await base.snoozeRemovalSuggestions(courses: courses, kind: kind)
    }

    public func clearRemovalSnooze(courses: [String]) async throws(PageLampFailure) {
        try await base.clearRemovalSnooze(courses: courses)
    }

    public func snoozeLifecycleBanner() async throws(PageLampFailure) {
        try await base.snoozeLifecycleBanner()
    }

    public func confirmCourseDates(course: String) async throws(PageLampFailure) -> CourseTimeline {
        try await base.confirmCourseDates(course: course)
    }

    public func startupTasks(now: Date) async throws(PageLampFailure) -> StartupTasks {
        try await base.startupTasks(now: now)
    }

    public func updatePrefs() async throws(PageLampFailure) -> UpdatePrefs {
        try await base.updatePrefs()
    }

    public func setUpdatePrefs(prefs: UpdatePrefs) async throws(PageLampFailure) {
        try await base.setUpdatePrefs(prefs: prefs)
    }

    public func effectiveUpdateChannel() async throws(PageLampFailure) -> UpdateChannel {
        try await base.effectiveUpdateChannel()
    }

    public func acknowledgeWhatsNew() async throws(PageLampFailure) {
        try await base.acknowledgeWhatsNew()
    }

    public func acknowledgeUpdateDisclosure() async throws(PageLampFailure) {
        try await base.acknowledgeUpdateDisclosure()
    }

    public func recordUpdateCheck(record: UpdateCheckRecord) async throws(PageLampFailure) {
        try await base.recordUpdateCheck(record: record)
    }

    public func lastUpdateCheck() async throws(PageLampFailure) -> UpdateCheckRecord? {
        try await base.lastUpdateCheck()
    }

    public func activity() async throws(PageLampFailure) -> Activity {
        try await base.activity()
    }

    public func syncAll(request: SyncRequest, observer: any SyncObserver) async throws(PageLampFailure) -> SyncSummary {
        try await base.syncAll(request: request, observer: observer)
    }

    public func syncSource(sourceId: String, request: SyncRequest, observer: any SyncObserver) async throws(PageLampFailure) -> SourceSyncResult {
        try await base.syncSource(sourceId: sourceId, request: request, observer: observer)
    }

    public func mcpClientConfigs(pagelampBinary: String) async throws(PageLampFailure) -> [McpClientConfig] {
        try await base.mcpClientConfigs(pagelampBinary: pagelampBinary)
    }

    public func mcpLaunch(pagelampBinary: String) async throws(PageLampFailure) -> McpLaunch {
        try await base.mcpLaunch(pagelampBinary: pagelampBinary)
    }

    public func doctor() async throws(PageLampFailure) -> DoctorReport {
        try await base.doctor()
    }

    public func diagnosticReport() async throws(PageLampFailure) -> String {
        try await base.diagnosticReport()
    }

    public func logsDir() async throws(PageLampFailure) -> String {
        try await base.logsDir()
    }

    public func lastCrash() async throws(PageLampFailure) -> CrashReport? {
        try await base.lastCrash()
    }

    public func clearLastCrash() async throws(PageLampFailure) {
        try await base.clearLastCrash()
    }
}
