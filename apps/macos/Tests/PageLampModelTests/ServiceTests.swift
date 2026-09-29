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
        #expect(events.contains { if case .progress(_, _, let current, let total) = $0 { current == total } else { false } })
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
    }
}
