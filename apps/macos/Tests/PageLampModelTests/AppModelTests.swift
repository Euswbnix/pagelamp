// AppModel over MockService: loading, the sync flow and the status capsule, navigation, the data
// mode and refresh on activation.

import AppKit
import Foundation
import PageLampKit
import PageLampModel
import Testing

@Suite("AppModel") @MainActor
struct AppModelTests {
    @Test("refresh loads the shell data and the This Week inputs")
    func refreshLoads() async {
        let (model, _) = makeModel(scenario: .demo)
        #expect(model.phase == .loading)
        await model.refresh()
        #expect(model.phase == .ready)
        #expect(model.courses.count == 4)
        // Hidden DEMO099 (also a past course) is not in the sidebar.
        #expect(model.visibleCourses.map { $0.course.code ?? "" } == ["DEMO101", "DEMO205", "DEMO310"])
        #expect(model.sources.map(\.id) == ["folder:demo-courses", "ical:demo-calendar", "canvas:canvas.demo.test"])
        #expect(model.studyPlan?.plan.items.count == 8)
        #expect(model.thisWeek.dueCount == 3)
        #expect(model.failingSources.isEmpty)
        #expect(model.footerStatus == .synced(TestClock.now.addingTimeInterval(-2 * 3600)))
        #expect(model.capsule == .hidden)
        #expect(model.sectionErrors.isEmpty)
        #expect(model.canSync)
    }

    @Test("a sync streams progress into the capsule, finishes, then hides")
    func syncFlow() async {
        let (model, mock) = makeModel(scenario: .demo, syncStep: .milliseconds(3))
        await model.refresh()

        var seen: [CapsuleState] = []
        let sync = Task { await model.syncAll() }
        let sawProgress = await eventually {
            if seen.last != model.capsule { seen.append(model.capsule) }
            if case .syncing(let syncing) = model.capsule, syncing.sourceIndex >= 2 { return true }
            return false
        }
        #expect(sawProgress)
        #expect(model.isSyncing)
        #expect(model.footerStatus == .syncing)
        #expect(!model.canSync)
        if case .syncing(let syncing) = model.capsule {
            #expect(syncing.sourceCount == 3)
            #expect(syncing.sourceLabel != nil)
        }
        #expect(model.syncProgress?.sourceCount == 3)

        // A second request while syncing is ignored (the button is disabled anyway).
        await model.syncAll()
        #expect(await mock.callCount("sync") == 1)

        await sync.value
        #expect(!model.isSyncing)
        #expect(model.syncProgress == nil)
        #expect(model.capsule == .finished(.init(problems: 0)))
        #expect(model.lastRun?.results.map(\.ok) == [true, true, true])
        // The folder source reported a warning along the way.
        #expect(model.lastRun?.result(for: "folder:demo-courses")?.warnings.count == 1)
        #expect(seen.first == .hidden)
        #expect(seen.contains { if case .syncing(let s) = $0 { s.sourceIndex == 1 } else { false } })

        // "Sync finished" stays for `finishedCapsule`, then the capsule leaves.
        #expect(await eventually { model.capsule == .hidden })
        model.hideResults()
        #expect(model.lastRun == nil)
    }

    @Test("an expired token needs attention at launch; × dismisses it until it fails again")
    func attention() async {
        let (model, _) = makeModel(scenario: .expired)
        await model.refresh()
        let expected = CapsuleState.attention(.init(
            sourceId: "canvas:canvas.demo.test", sourceLabel: "Demo Canvas", fix: .replaceToken
        ))
        #expect(model.capsule == expected)
        #expect(model.footerStatus == .needsAttention)
        #expect(model.isSourceFailing("canvas:canvas.demo.test"))

        model.dismissAttention()
        #expect(model.capsule == .hidden)
        await model.refresh()
        #expect(model.capsule == .hidden)

        // Syncing again: Canvas rejects the token again, a new problem.
        await model.syncAll()
        #expect(model.capsule == expected)
        #expect(model.lastRun?.result(for: "canvas:canvas.demo.test")?.errorKind == .authExpiredOrRevoked)

        // The fix bubble opens Sources & Sync (the Replace sheet is M2).
        model.performCapsuleFix()
        #expect(model.destination == .sources)
    }

    @Test("Debug ▸ mock sync with a rejected token turns a healthy demo into attention")
    func rejectedTokenDebugAction() async {
        let (model, _) = makeModel(scenario: .demo)
        await model.refresh()
        #expect(model.capsule == .hidden)
        await model.runMockSyncWithRejectedToken()
        guard case .attention(let attention) = model.capsule else {
            Issue.record("expected attention, got \(model.capsule)")
            return
        }
        #expect(attention.sourceLabel == "Demo Canvas")
        #expect(model.failingSources.map(\.id) == ["canvas:canvas.demo.test"])
    }

    @Test("a sync refused as busy shows a short notice")
    func busy() async {
        let (model, _) = makeModel(scenario: .busy)
        await model.refresh()
        // The CLI holds the lock: the footer says so even though this app isn't syncing.
        #expect(model.footerStatus == .syncing)
        await model.syncAll()
        #expect(model.capsule == .failed(.busy))
        #expect(model.lastRun == nil)
        #expect(await eventually { model.capsule == .hidden })
    }

    @Test("a course's week stepping follows its available weeks")
    func weekStepping() async throws {
        let (model, _) = makeModel(scenario: .demo)
        await model.refresh()
        let demo101 = try #require(model.visibleCourses.first { $0.course.code == "DEMO101" }).course.id
        #expect(!model.canStepWeek(by: -1))

        model.destination = .course(demo101)
        let week = try await model.weekMaterials(for: demo101)
        #expect(week.week == 4)
        let ui = model.ui(for: demo101)
        #expect(ui.availableWeeks == [1, 2, 3, 4])
        #expect(ui.showsCurrentWeek)
        #expect(model.canStepWeek(by: -1))
        #expect(!model.canStepWeek(by: 1))
        #expect(!model.canShowCurrentWeek)

        model.stepWeek(by: -1)
        #expect(ui.selectedWeek == 3)
        #expect(!ui.showsCurrentWeek)
        #expect(model.canShowCurrentWeek)
        #expect(try await model.weekMaterials(for: demo101).week == 3)

        model.stepWeek(by: 1)
        // Back on the current week is "now" again.
        #expect(ui.selectedWeek == nil)
        #expect(ui.showsCurrentWeek)

        model.stepWeek(by: -1)
        model.showCurrentWeek()
        #expect(ui.selectedWeek == nil)

        // Unknown week: no stepping.
        let demo310 = try #require(model.visibleCourses.first { $0.course.code == "DEMO310" }).course.id
        model.destination = .course(demo310)
        #expect(try await model.weekMaterials(for: demo310).noteKind == .currentWeekUnknown)
        #expect(!model.canStepWeek(by: -1))
        #expect(!model.canStepWeek(by: 1))
        #expect(!model.canShowCurrentWeek)
    }

    @Test("switching the mock scenario reloads everything")
    func dataMode() async {
        let (model, _) = makeModel(scenario: .demo)
        await model.refresh()
        model.destination = .course(model.visibleCourses[0].course.id)
        await model.useMock(.empty)
        #expect(model.dataMode == .mock(.empty))
        #expect(model.phase == .ready)
        #expect(model.courses.isEmpty)
        #expect(model.sources.isEmpty)
        #expect(model.footerStatus == .neverSynced)
        #expect(!model.canSync)
        #expect(model.destination == .thisWeek)
        #expect(model.service is MockService)

        await model.useMock(.crashed)
        #expect(model.lastCrash?.process == .app)
    }

    @Test("the app refreshes when it becomes active")
    func refreshOnActivation() async {
        let center = NotificationCenter()
        let (model, mock) = makeModel(scenario: .demo, notificationCenter: center)
        await model.start()
        let before = model.refreshCount
        let statusCalls = await mock.callCount("status")
        center.post(name: NSApplication.didBecomeActiveNotification, object: nil)
        #expect(await eventually { model.refreshCount > before })
        #expect(await eventually { model.phase == .ready })
        #expect(await mock.callCount("status") > statusCalls)
    }

    @Test("the diagnostic report loads for its preview")
    func diagnosticReport() async {
        let (model, _) = makeModel(scenario: .demo)
        await model.showDiagnosticReport()
        guard case .loaded(let text) = model.diagnosticReport else {
            Issue.record("expected a report, got \(String(describing: model.diagnosticReport))")
            return
        }
        #expect(text.hasPrefix("# PageLamp diagnostic report"))
        model.dismissDiagnosticReport()
        #expect(model.diagnosticReport == nil)
    }

    @Test("the arbiter picks one tinted action: policy > source fix > term dates > page")
    func arbiter() {
        #expect(PrimaryActionArbiter.winner([]) == nil)
        #expect(PrimaryActionArbiter.winner([.pagePrimary]) == .pagePrimary)
        #expect(PrimaryActionArbiter.winner([.pagePrimary, .setTermDates]) == .setTermDates)
        #expect(PrimaryActionArbiter.winner([.pagePrimary, .fixSource("b"), .fixSource("a")]) == .fixSource("b"))
        #expect(PrimaryActionArbiter.winner([.fixSource("a"), .saveAIPolicy, .setTermDates]) == .saveAIPolicy)
    }

    @Test("from an unknown week, ‹ steps to the last available week and Current Week comes back")
    func unknownWeekStepping() {
        let ui = CourseUIState()
        ui.update(availableWeeks: [3, 1, 2], currentWeek: nil)
        #expect(ui.displayedWeek == nil)
        #expect(ui.previousWeek == 3)
        #expect(ui.nextWeek == nil)
        #expect(!ui.isAwayFromDefault)
        ui.step(by: -1)
        #expect(ui.selectedWeek == 3)
        #expect(ui.isAwayFromDefault)
        #expect(ui.previousWeek == 2)
        #expect(!ui.showsCurrentWeek)
        ui.showCurrentWeek()
        #expect(ui.selectedWeek == nil)
        #expect(!ui.isAwayFromDefault)
    }

    @Test("every Replace… leads to one place; the report opens on the window that asked")
    func fixAndReportHost() async {
        let (model, _) = makeModel(scenario: .expired)
        await model.refresh()
        model.fixSource("canvas:canvas.demo.test")
        #expect(model.destination == .sources)

        await model.showDiagnosticReport(in: .settings)
        #expect(model.diagnosticReportHost == .settings)
        model.dismissDiagnosticReport()
        await model.showDiagnosticReport()
        #expect(model.diagnosticReportHost == .main)
    }
}
