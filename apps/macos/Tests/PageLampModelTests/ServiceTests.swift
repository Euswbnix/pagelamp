// The services: MockService behaves like the facade where the UI can tell; LiveService maps the
// facade's errors (over a temp data folder with in-memory secrets, never the real one).

import Foundation
import PageLampKit
import PageLampModel
import Testing

@Suite("MockService")
struct MockServiceTests {
    func mock(_ scenario: MockScenario) -> MockService {
        MockService(scenario: scenario, timing: .instant, calendar: TestClock.calendar, now: { TestClock.now })
    }

    @Test("demo courses: weeks, policies, readable counts")
    func courses() async throws {
        let courses = try await mock(.demo).listCourses()
        let byCode = Dictionary(uniqueKeysWithValues: courses.map { ($0.course.code ?? "", $0) })
        #expect(byCode["DEMO101"]?.timeline.currentWeek == 4)
        #expect(byCode["DEMO101"]?.counts.materials == 13)
        #expect(byCode["DEMO101"]?.counts.indexedMaterials == 10)
        #expect(byCode["DEMO101"]?.nextDeadline?.event.title == "Problem Set 2")
        #expect(byCode["DEMO205"]?.sourceLabel == "Demo Canvas")
        // No AI: materials withheld, nothing counted as readable.
        #expect(byCode["DEMO310"]?.aiMaterials == .withheldByPolicy)
        #expect(byCode["DEMO310"]?.counts.indexedMaterials == 0)
        #expect(byCode["DEMO310"]?.timeline.currentWeek == nil)
        #expect(byCode["DEMO099"]?.course.hidden == true)
        #expect(byCode["DEMO099"]?.course.enrollmentActive == false)
    }

    @Test("course lookups by id or code; unknown ones are notFound")
    func lookups() async throws {
        let service = mock(.demo)
        let overview = try await service.courseOverview(course: "DEMO205")
        #expect(overview.downloadableFiles == 1)
        #expect(overview.currentModules.map(\.name) == ["Unit C: Rows"])
        let week = try await service.weekMaterials(course: "folder:demo-courses/course/DEMO101", week: 2)
        #expect(week.materials.count == 2)
        #expect(week.availableWeeks == [1, 2, 3, 4])
        do {
            _ = try await service.courseOverview(course: "NOPE")
            Issue.record("expected notFound")
        } catch {
            #expect(error.kind == .notFound)
        }
    }

    @Test("sync streams events per source; an expired token fails that source only")
    func syncEvents() async throws {
        let service = mock(.expired)
        let recorder = EventRecorder()
        let summary = try await service.syncAll(request: SyncRequest(), observer: recorder)
        #expect(!summary.ok)
        #expect(summary.results.map(\.ok) == [true, true, false])
        #expect(summary.results[2].errorKind == .authExpiredOrRevoked)
        let events = recorder.events
        let starts = events.compactMap { if case .sourceStarted(let id, _) = $0 { id } else { nil } }
        let ends = events.compactMap { if case .sourceFinished(let id, let ok, _, _) = $0 { "\(id):\(ok)" } else { nil } }
        #expect(starts == ["folder:demo-courses", "ical:demo-calendar", "canvas:canvas.demo.test"])
        #expect(ends == ["folder:demo-courses:true", "ical:demo-calendar:true", "canvas:canvas.demo.test:false"])
        #expect(events.contains { if case .warning = $0 { true } else { false } })
        #expect(events.contains { if case .progress(_, _, let current, let total, _, _) = $0 { current == total } else { false } })
    }

    @Test("busy while another process syncs; unknown sources are notFound")
    func busyAndNotFound() async throws {
        do {
            _ = try await mock(.busy).syncAll(request: SyncRequest(), observer: EventRecorder())
            Issue.record("expected busy")
        } catch {
            #expect(error.kind == .busy)
        }
        do {
            _ = try await mock(.demo).syncSource(sourceId: "nope", request: SyncRequest(), observer: EventRecorder())
            Issue.record("expected notFound")
        } catch {
            #expect(error.kind == .notFound)
        }
    }

    @Test("course lane: timeline, lifecycle summary, I'm still taking this, snoozes")
    func courseLane() async throws {
        let service = mock(.demo)
        #expect(try await service.courseTimeline(course: "DEMO101").currentWeek == 4)
        let kept = try await service.keepCourseCurrent(course: "DEMO101", until: "2027-01-31")
        #expect(kept.code == "DEMO101")
        var summary = try await service.lifecycleSummary()
        let courseCount = try await service.listCourses().count
        #expect(summary.courses.count == courseCount)
        let entry = summary.courses.first { $0.code == "DEMO101" }
        #expect(entry?.lifecycle.keptCurrentUntil == "2027-01-31")
        #expect(entry?.lifecycle.state == .current && entry?.lifecycle.confidence == .high)
        #expect(summary.suggested.isEmpty && !summary.showBanner)
        _ = try await service.clearKeepCourseCurrent(course: "DEMO101")
        let listed = try await service.listCourses().first { $0.course.code == "DEMO101" }
        #expect(listed?.lifecycle.keptCurrentUntil == nil)
        // Without a date: today + keepCurrentDays() (the mock has no term dates).
        _ = try await service.keepCourseCurrent(course: "DEMO205", until: nil)
        summary = try await service.lifecycleSummary()
        #expect(summary.courses.first { $0.code == "DEMO205" }?.lifecycle.keptCurrentUntil?.hasPrefix("2027-01-") == true)

        try await service.snoozeRemovalSuggestions(courses: ["DEMO099"], kind: .keep)
        try await service.clearRemovalSnooze(courses: ["DEMO099"])
        try await service.snoozeLifecycleBanner()
        #expect(try await service.lifecycleSummary().bannerSnoozedUntil == "2026-10-09")
        #expect(try await service.confirmCourseDates(course: "DEMO101").currentWeek == 4)
        do {
            _ = try await service.courseTimeline(course: "NOPE")
            Issue.record("expected notFound")
        } catch {
            #expect(error.kind == .notFound)
        }
    }

    @Test("updates: a fresh install checks after the disclosure; an upgrade shows What's new first")
    func updates() async throws {
        let service = mock(.demo)
        let now = TestClock.now
        #expect(try await service.effectiveUpdateChannel() == .beta, "0.1.0-mock is a pre-release")
        var tasks = try await service.startupTasks(now: now)
        #expect(tasks.whatsNew == nil && tasks.updatedFrom == nil && !tasks.updateCheckDue)
        try await service.acknowledgeUpdateDisclosure()
        #expect(try await service.startupTasks(now: now).updateCheckDue)

        let record = UpdateCheckRecord(at: now, channel: .beta, outcome: .upToDate)
        try await service.recordUpdateCheck(record: record)
        #expect(try await service.lastUpdateCheck() == record)
        #expect(try await !service.startupTasks(now: now).updateCheckDue)
        #expect(try await service.startupTasks(now: now.addingTimeInterval(24 * 3600)).updateCheckDue)

        await service.simulateUpgrade(from: "0.3.0-alpha.1")
        tasks = try await service.startupTasks(now: now.addingTimeInterval(24 * 3600))
        #expect(tasks.whatsNew?.topics == [.updateCheck, .courseWeeks])
        #expect(tasks.updatedFrom == "0.3.0-alpha.1")
        #expect(!tasks.updateCheckDue, "not while What's new waits")
        try await service.acknowledgeWhatsNew()
        tasks = try await service.startupTasks(now: now.addingTimeInterval(24 * 3600))
        #expect(tasks.whatsNew == nil && tasks.updateCheckDue)

        try await service.setUpdatePrefs(prefs: UpdatePrefs(autoCheck: false, channel: .stable))
        #expect(try await service.updatePrefs() == UpdatePrefs(autoCheck: false, channel: .stable))
        #expect(try await service.effectiveUpdateChannel() == .stable)
        #expect(try await !service.startupTasks(now: now.addingTimeInterval(48 * 3600)).updateCheckDue)
    }

    @Test("activity: nothing running here; another process's sync shows")
    func activity() async throws {
        let idle = try await mock(.demo).activity()
        #expect(idle.items.isEmpty && !idle.otherProcessSyncing)
        #expect(try await mock(.busy).activity().otherProcessSyncing)
    }

    @Test("empty scenario: no sources; Connect and diagnostics still answer")
    func emptyScenario() async throws {
        let service = mock(.empty)
        let status = try await service.status()
        #expect(status.sources.isEmpty)
        #expect(status.counts.courses == 0)
        #expect(try await service.latestStudyPlan() == nil)
        let configs = try await service.mcpClientConfigs(pagelampBinary: MockService.binaryPath)
        #expect(configs.map(\.client) == [.claudeDesktop, .claudeCode, .codex, .generic])
        #expect(configs[0].content.contains(MockService.binaryPath))
        #expect(try await service.doctor().mcpClients.claudeCode)
    }
}

@Suite("LiveService")
struct LiveServiceTests {
    @Test("every facade error maps to its failure kind")
    func errorMapping() {
        let cases: [(PageLampError, PageLampFailure.Kind)] = [
            (.Auth(message: "a"), .auth),
            (.Network(message: "n"), .network),
            (.Invalid(message: "i"), .invalid),
            (.NotFound(message: "f"), .notFound),
            (.Ambiguous(message: "m"), .ambiguous),
            (.Busy(message: "b"), .busy),
            (.Schema(message: "s"), .schema),
            (.Blocked(message: "k", reason: .courseHidden), .blocked),
            (.Model(message: "o", kind: .authRejected, retryAfterSecs: nil), .model),
            (.Cancelled(message: "c"), .cancelled),
            (.Internal(message: "x"), .internal),
            (.Panic(message: "p"), .panic),
        ]
        for (error, kind) in cases {
            let failure = PageLampFailure.from(error)
            #expect(failure.kind == kind)
            #expect(!failure.message.isEmpty)
        }
        #expect(PageLampFailure.from(CancellationError()).kind == .internal)
    }

    @Test("over the real facade (temp folder, in-memory secrets)")
    func realFacade() async throws {
        let dir = URL(filePath: NSTemporaryDirectory(), directoryHint: .isDirectory)
            .appending(path: "PageLampModelTests-\(UUID().uuidString)", directoryHint: .isDirectory)
        defer { try? FileManager.default.removeItem(at: dir) }
        let core = try await PageLamp.openWithMemorySecrets(dataDir: dir.path(percentEncoded: false))
        let service: any PageLampService = LiveService(core: core)

        let status = try await service.status()
        #expect(!status.version.isEmpty)
        #expect(status.sources.isEmpty)
        #expect(try await service.listCourses().isEmpty)
        #expect(try await service.latestStudyPlan() == nil)
        do {
            _ = try await service.courseOverview(course: "NOPE")
            Issue.record("expected notFound")
        } catch {
            #expect(error.kind == .notFound)
        }
        do {
            _ = try await service.syncSource(sourceId: "folder:nope", request: SyncRequest(), observer: EventRecorder())
            Issue.record("expected notFound")
        } catch {
            #expect(error.kind == .notFound)
        }

        // The course lane and the update facade reach the core.
        let summary = try await service.lifecycleSummary()
        #expect(summary.courses.isEmpty && !summary.showBanner)
        do {
            _ = try await service.courseTimeline(course: "NOPE")
            Issue.record("expected notFound")
        } catch {
            #expect(error.kind == .notFound)
        }
        #expect(try await service.updatePrefs().autoCheck)
        let tasks = try await service.startupTasks(now: Date())
        #expect(tasks.whatsNew == nil, "a fresh data folder is a fresh install")
        try await service.acknowledgeUpdateDisclosure()
        #expect(try await service.startupTasks(now: Date()).updateCheckDue)
        #expect(try await service.lastUpdateCheck() == nil)
        let activity = try await service.activity()
        #expect(activity.items.isEmpty && !activity.otherProcessSyncing)
        #expect(notNowDays() == 14 && keepCurrentDays() == 120 && keepForever() == "9999-12-31")
    }
}
