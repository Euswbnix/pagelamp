// The shell's cross-page behaviour: who holds the window's one tint (the capsule's fix bubble
// or the page), where a fix leads (Sources & Sync, scrolled to and highlighting the source),
// overlapping refreshes, menu commands with the main window closed, and the crash notice.

import Foundation
import PageLampKit
import PageLampModel
import Testing

@Suite("Shell behaviour") @MainActor
struct ShellBehaviourTests {
    private let canvas = "canvas:canvas.demo.test"

    // MARK: One tinted action per window (spec §3.0, §6.2)

    @Test("the capsule's fix bubble holds the tint; the page's own fix goes bordered until it leaves")
    func capsuleHoldsTint() async throws {
        let (model, _) = makeModel(scenario: .expired)
        await model.refresh()
        let summary = try #require(model.courses.first { $0.course.sourceId == canvas })
        let source = model.sources.first { $0.id == canvas }
        // As it will be once the Replace sheet exists (M2); M1 has no fix candidate on the page.
        let coursePage = CourseDetailModel.primaryActionCandidates(source: source, timeline: summary.timeline, canReplaceSecrets: true)
        #expect(coursePage.first == .fixSource(canvas))
        model.destination = .course(summary.course.id)

        // Attention: the bubble shows and wins over the course header's Replace Token….
        #expect(model.showsCapsuleFix)
        #expect(model.primaryActionWinner(for: coursePage) == .capsuleFix)
        #expect(PrimaryActionArbiter.winner(page: coursePage, capsule: model.capsule, destination: model.destination) == .capsuleFix)

        // × on the capsule: the page gets the tint back.
        model.dismissAttention()
        #expect(!model.showsCapsuleFix)
        #expect(model.primaryActionWinner(for: coursePage) == .page(.fixSource(canvas)))

        // Nothing on the page: nobody is tinted (This Week has no action of its own).
        #expect(model.primaryActionWinner(for: []) == nil)
    }

    @Test("on Sources & Sync the bubble is hidden: the source's callout is the fix surface")
    func bubbleHiddenOnSources() {
        let attention = CapsuleState.attention(.init(sourceId: "canvas:x", sourceLabel: "Canvas", fix: .replaceToken))
        for destination in [Destination.thisWeek, .connect, .course("c")] {
            #expect(PrimaryActionArbiter.showsCapsuleFix(capsule: attention, destination: destination))
        }
        #expect(!PrimaryActionArbiter.showsCapsuleFix(capsule: attention, destination: .sources))
        #expect(PrimaryActionArbiter.winner(page: [.pagePrimary], capsule: attention, destination: .sources) == .page(.pagePrimary))
        // Only attention has a fix bubble.
        for capsule in [CapsuleState.hidden, .finished(.init(problems: 0)), .failed(.busy), .syncing(.init(sourceIndex: 1, sourceCount: 2))] {
            #expect(!PrimaryActionArbiter.showsCapsuleFix(capsule: capsule, destination: .thisWeek))
            #expect(PrimaryActionArbiter.winner(page: [.pagePrimary], capsule: capsule, destination: .thisWeek) == .page(.pagePrimary))
        }
    }

    @Test("the arbiter picks one tinted action: policy > source fix > term dates > page")
    func ranking() {
        #expect(PrimaryActionArbiter.winner([]) == nil)
        #expect(PrimaryActionArbiter.winner([.pagePrimary]) == .pagePrimary)
        #expect(PrimaryActionArbiter.winner([.pagePrimary, .setTermDates]) == .setTermDates)
        #expect(PrimaryActionArbiter.winner([.pagePrimary, .fixSource("b"), .fixSource("a")]) == .fixSource("b"))
        #expect(PrimaryActionArbiter.winner([.fixSource("a"), .saveAIPolicy, .setTermDates]) == .saveAIPolicy)
    }

    // MARK: Where a fix leads (S3)

    @Test("the fix bubble opens Sources & Sync at its source, highlighted until the timer ends it")
    func fixHighlightsSource() async {
        let timers = ManualTimers()
        let (model, _) = makeModel(scenario: .expired, timers: timers)
        await model.refresh()
        #expect(model.sourceHighlight == nil)

        model.performCapsuleFix()
        #expect(model.destination == .sources)
        let first = model.sourceHighlight
        #expect(first?.sourceId == canvas)
        // On Sources the bubble is gone; the capsule still says what is wrong.
        #expect(!model.showsCapsuleFix)
        #expect(model.capsule != .hidden)

        // Asking again (another fix button) is a new request: the page scrolls and flashes again.
        model.showSource(canvas)
        #expect(model.sourceHighlight?.sourceId == canvas)
        #expect(model.sourceHighlight != first)

        // The highlight ends when its timer fires; an outdated request never ends a newer one.
        if let first { model.endSourceHighlight(first) }
        #expect(model.sourceHighlight != nil)
        await timers.waitForTimer(.seconds(2))
        await timers.fire(.seconds(2))
        #expect(await until { model.sourceHighlight == nil })
    }

    // MARK: Overlapping refreshes (S5)

    @Test("a refresh that finishes after a newer one never puts its stale data back")
    func staleRefreshDropped() async throws {
        let hold = CallHold()
        let (model, _) = makeModel(scenario: .demo) { HeldService(base: $0, statusHold: hold) }
        await model.refresh()
        let before = try #require(model.status?.lastSyncedAt)
        #expect(before < TestClock.now)

        // An activation refresh reads the status (before the sync) and is held right there.
        await hold.arm()
        let stale = Task { await model.refresh() }
        await hold.waitUntilHeld()
        let staleSequence = model.refreshCount

        // Our sync runs, and its own refresh applies the fresh data.
        await model.syncAll()
        #expect(model.status?.lastSyncedAt == TestClock.now)
        #expect(model.appliedRefresh > staleSequence)
        let fresh = model.appliedRefresh

        // The held refresh finishes last: dropped.
        await hold.release()
        await stale.value
        #expect(model.appliedRefresh == fresh)
        #expect(model.status?.lastSyncedAt == TestClock.now)
        #expect(model.footerStatus == .synced(TestClock.now))
        #expect(model.phase == .ready)
    }

    @Test("a stale failure doesn't take the window down either")
    func staleFailureDropped() async {
        let hold = CallHold()
        let (model, _) = makeModel(scenario: .demo) { HeldService(base: $0, statusHold: hold) }
        await model.refresh()
        await hold.arm(failing: PageLampFailure(kind: .internal, message: "database locked"))
        let stale = Task { await model.refresh() }
        await hold.waitUntilHeld()
        await model.refresh()
        await hold.release()
        await stale.value
        #expect(model.phase == .ready)
    }

    // MARK: Menu commands with the main window closed (S8)

    @Test("menu commands open the main window first, then do their thing there")
    func commandsOpenMainWindow() async throws {
        let (model, _) = makeModel(scenario: .demo)
        await model.refresh()
        var log: [String] = []

        #expect(!model.restoredWindowState)
        await model.perform(.show(.sources)) { log.append("open") }
        #expect(log == ["open"])
        #expect(model.destination == .sources)
        // The window the command opened keeps this destination (no older stored one wins).
        #expect(model.restoredWindowState)

        await model.perform(.show(.connect)) { log.append("open:\(model.destination)") }
        // Opened before navigating.
        #expect(log.last == "open:sources")
        #expect(model.destination == .connect)

        await model.perform(.diagnosticReport) { log.append("open") }
        #expect(model.diagnosticReportHost == .main)
        guard case .loaded = model.diagnosticReport else {
            Issue.record("expected the report, got \(String(describing: model.diagnosticReport))")
            return
        }

        await model.perform(.liveData) { log.append("open") }
        #expect(model.confirmingLiveData)
        #expect(model.dataMode == .mock(.demo))

        let course = try #require(model.visibleCourses.first { $0.course.code == "DEMO101" }).course.id
        await model.perform(.show(.course(course))) { log.append("open") }
        _ = try await model.weekMaterials(for: course)
        await model.perform(.stepWeek(-1)) { log.append("open") }
        #expect(model.ui(for: course).selectedWeek == 3)
        await model.perform(.currentWeek) { log.append("open") }
        #expect(model.ui(for: course).selectedWeek == nil)
        #expect(log.count == 7)
    }

    // MARK: Crash notice (S6)

    @Test("the crash notice shows the last crash until Dismiss clears it")
    func crashNotice() async throws {
        let (model, mock) = makeModel(scenario: .crashed)
        await model.refresh()
        #expect(model.lastCrash != nil)
        await model.dismissCrash()
        #expect(model.lastCrash == nil)
        #expect(model.crashDismissFailure == nil)
        #expect(try await mock.lastCrash() == nil)
        await model.refresh()
        #expect(model.lastCrash == nil)
    }

    @Test("a failed Dismiss keeps the notice and says why")
    func crashDismissFails() async {
        let (model, _) = makeModel(scenario: .crashed) { FixtureService(base: $0, failing: [.clearCrash]) }
        await model.refresh()
        await model.dismissCrash()
        #expect(model.lastCrash != nil)
        #expect(model.crashDismissFailure?.kind == .internal)
    }
}

// MARK: - Test services

/// Holds the answer of a service call when armed (`HeldService`): the answer is read first, then
/// held, so the held caller gets data from before whatever happens meanwhile.
actor CallHold {
    private var armed = false
    private var failure: PageLampFailure?
    private var held: CheckedContinuation<Void, Never>?
    private var watchers: [CheckedContinuation<Void, Never>] = []

    /// The next call is held (and fails with `failing`, if given).
    func arm(failing: PageLampFailure? = nil) {
        armed = true
        failure = failing
    }

    func waitUntilHeld() async {
        if held != nil { return }
        await withCheckedContinuation { watchers.append($0) }
    }

    func release() {
        held?.resume()
        held = nil
    }

    /// Called by the service with the answer it read; returns the failure to throw instead.
    func pass() async -> PageLampFailure? {
        guard armed else { return nil }
        armed = false
        let failure = self.failure
        await withCheckedContinuation { continuation in
            held = continuation
            for watcher in watchers { watcher.resume() }
            watchers = []
        }
        return failure
    }
}

/// The mock, with `status()` and `mcpClientConfigs(…)` going through a `CallHold` each (if given).
struct HeldService: PageLampService {
    let base: any PageLampService
    var statusHold: CallHold?
    var configsHold: CallHold?

    func status() async throws(PageLampFailure) -> AppStatus {
        let status = try await base.status()
        if let failure = await statusHold?.pass() { throw failure }
        return status
    }

    func listCourses() async throws(PageLampFailure) -> [CourseSummary] { try await base.listCourses() }
    func listSources() async throws(PageLampFailure) -> [SourceRecord] { try await base.listSources() }
    func listDeadlines(course: String?, daysAhead: UInt32, daysBack: UInt32) async throws(PageLampFailure) -> [Deadline] {
        try await base.listDeadlines(course: course, daysAhead: daysAhead, daysBack: daysBack)
    }
    func latestStudyPlan() async throws(PageLampFailure) -> StoredStudyPlan? { try await base.latestStudyPlan() }
    func courseOverview(course: String) async throws(PageLampFailure) -> CourseOverview { try await base.courseOverview(course: course) }
    func weekMaterials(course: String, week: UInt32?) async throws(PageLampFailure) -> WeekMaterials {
        try await base.weekMaterials(course: course, week: week)
    }
    func syncAll(request: SyncRequest, observer: any SyncObserver) async throws(PageLampFailure) -> SyncSummary {
        try await base.syncAll(request: request, observer: observer)
    }
    func syncSource(sourceId: String, request: SyncRequest, observer: any SyncObserver) async throws(PageLampFailure) -> SourceSyncResult {
        try await base.syncSource(sourceId: sourceId, request: request, observer: observer)
    }
    func mcpClientConfigs(pagelampBinary: String) async throws(PageLampFailure) -> [McpClientConfig] {
        let configs = try await base.mcpClientConfigs(pagelampBinary: pagelampBinary)
        if let failure = await configsHold?.pass() { throw failure }
        return configs
    }
    func mcpLaunch(pagelampBinary: String) async throws(PageLampFailure) -> McpLaunch { try await base.mcpLaunch(pagelampBinary: pagelampBinary) }
    func doctor() async throws(PageLampFailure) -> DoctorReport { try await base.doctor() }
    func diagnosticReport() async throws(PageLampFailure) -> String { try await base.diagnosticReport() }
    func logsDir() async throws(PageLampFailure) -> String { try await base.logsDir() }
    func lastCrash() async throws(PageLampFailure) -> CrashReport? { try await base.lastCrash() }
    func clearLastCrash() async throws(PageLampFailure) { try await base.clearLastCrash() }
}
