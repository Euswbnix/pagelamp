// This Week's snapshot states (spec §3.9): each catalogue page's model reaches the state it is
// named after. (The PNGs themselves: SnapshotRenderTests.)

import Foundation
import PageLamp
import PageLampKit
import PageLampModel
import Testing

@Suite("This Week snapshot states")
@MainActor
struct ThisWeekSnapshotStateTests {
    private func model(_ state: String) async throws -> AppModel {
        let page = try #require(SnapshotCatalog.page(named: "this-week-\(state)"))
        return await SnapshotRenderer.model(for: page, language: .english, calendar: TestClock.calendar, now: TestClock.now)
    }

    @Test("each snapshot state reaches the state it is named after")
    func fixtureStates() async throws {
        let names = SnapshotCatalog.pages.map(\.name).filter { $0.hasPrefix("this-week-") }
        #expect(names == [
            "default", "next-up", "next-up-morning", "quiet", "errors", "stale-plan",
            "no-sources", "no-courses", "first-sync", "narrow",
        ].map { "this-week-\($0)" })

        let preview = try await model("default")
        #expect(preview.thisWeekPageState == .page)
        #expect(preview.thisWeekFixCandidates == [.fixSource("canvas:canvas.demo.test")])

        let nextUp = try await model("next-up")
        let digest = ThisWeekDigest(
            deadlines: nextUp.upcomingDeadlines, plan: nextUp.studyPlan, now: nextUp.clock(), calendar: nextUp.calendar
        )
        let due = try #require(digest.nextUp.flatMap(ThisWeekDigest.time(of:)))
        #expect(Countdown(from: nextUp.clock(), to: due).minutes == 58)

        let quiet = try await model("quiet")
        #expect(quiet.upcomingDeadlines.isEmpty && quiet.studyPlan == nil && quiet.sectionErrors.isEmpty)

        let errors = try await model("errors")
        #expect(Set(errors.sectionErrors.keys) == [.courses, .deadlines, .studyPlan])
        #expect(errors.thisWeekPageState == .page)

        let stale = try await model("stale-plan")
        let plan = try #require(stale.studyPlan)
        #expect(StudyPlanDigest(plan, now: stale.clock(), calendar: stale.calendar).isStale)

        #expect(try await model("no-sources").thisWeekPageState == .noSources)
        #expect(try await model("no-courses").thisWeekPageState == .noCourses)
        #expect(try await model("first-sync").thisWeekPageState == .firstSync)
        #expect(SnapshotCatalog.page(named: "this-week-nonexistent") == nil)
    }
}
