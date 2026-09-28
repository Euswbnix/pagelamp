// Sources & Sync (spec §3.3): the typed source config, each source's status/problem/progress
// rows through a sync, the S17 busy state and the arbiter's candidates.

import Foundation
import PageLampKit
import PageLampModel
import Testing

@Suite("Sources & Sync") @MainActor
struct SetupSourcesTests {
    @Test("the source config reads only known, non-empty strings and web addresses")
    func sourceConfig() {
        let canvas = SourceConfig(json: #"{"base_url":"https://canvas.demo.test","account_name":"Demo Student"}"#)
        #expect(canvas.baseURL?.absoluteString == "https://canvas.demo.test")
        #expect(canvas.accountName == "Demo Student")
        #expect(canvas.path == nil)

        let folder = SourceConfig(json: #"{"path":"/Users/demo/Documents/Courses","term_start":"2026-09-02","extra":1}"#)
        #expect(folder.path == "/Users/demo/Documents/Courses")
        #expect(folder.termStart == "2026-09-02")

        #expect(SourceConfig(json: "{}") == SourceConfig())
        #expect(SourceConfig(json: "not json") == SourceConfig())
        #expect(SourceConfig(json: #"{"path":"   ","account_name":7}"#) == SourceConfig())
        // Only web addresses become links.
        #expect(SourceConfig(json: #"{"base_url":"javascript:alert(1)"}"#).baseURL == nil)
        #expect(SourceConfig(json: #"{"base_url":"http://canvas.example.edu"}"#).baseURL != nil)
    }

    @Test("an expired Canvas token is the page's problem; Sync All keeps the tint until Replace can run")
    func expiredToken() async {
        let (model, _) = makeModel(scenario: .expired)
        await model.refresh()
        let rows = model.sourceRows
        #expect(rows.map(\.id) == ["folder:demo-courses", "ical:demo-calendar", "canvas:canvas.demo.test"])
        #expect(rows.map(\.status) == [.ok, .ok, .failed(.authExpiredOrRevoked)])
        #expect(rows.map(\.problem) == [nil, nil, .expired(.replaceToken)])
        #expect(rows[2].config.accountName == "Demo Student")
        #expect(rows.map(\.hasSecret) == [false, true, true])
        #expect(rows.allSatisfy { $0.progress == nil && $0.lastRun == nil })

        // No Replace sheet in this build: Replace… shows disabled and plain, never a candidate;
        // Sync All keeps the tint (spec §3.0 never nominates a fix that can't run).
        #expect(!SourceRow.canReplaceSecrets)
        let candidates = SourceRow.primaryActionCandidates(rows)
        #expect(candidates == [.pagePrimary])
        #expect(PrimaryActionArbiter.winner(candidates) == .pagePrimary)
        // Once the sheet exists, the first rejected secret's Replace… outranks Sync All.
        let withSheet = SourceRow.primaryActionCandidates(rows, canReplaceSecrets: true)
        #expect(withSheet == [.fixSource("canvas:canvas.demo.test"), .pagePrimary])
        #expect(PrimaryActionArbiter.winner(withSheet) == .fixSource("canvas:canvas.demo.test"))
        // On Sources the capsule keeps its words but not its fix bubble: the page holds the tint.
        model.destination = .sources
        #expect(!model.showsCapsuleFix)
        #expect(model.primaryActionWinner(for: candidates) == .page(.pagePrimary))
        #expect(model.canStartSync)
    }

    @Test("with nothing to fix, Sync All is the page's primary action")
    func allFine() async {
        let (model, _) = makeModel(scenario: .demo)
        await model.refresh()
        #expect(model.sourceRows.allSatisfy { $0.status == .ok && $0.problem == nil })
        #expect(PrimaryActionArbiter.winner(SourceRow.primaryActionCandidates(model.sourceRows)) == .pagePrimary)
    }

    @Test("a rejected feed address asks for a new address; a missing folder is a plain failure")
    func problems() {
        let feed = SourceRecord(
            id: "ical:x", kind: .ical, label: "Feed", config: "{}", lastSyncedAt: nil,
            lastError: "401", lastErrorKind: .authExpiredOrRevoked
        )
        let folder = SourceRecord(
            id: "folder:x", kind: .folder, label: "Folder", config: "{}", lastSyncedAt: TestClock.now,
            lastError: "gone", lastErrorKind: .notFound
        )
        let odd = SourceRecord(
            id: "folder:y", kind: .folder, label: "Folder", config: "{}", lastSyncedAt: nil,
            lastError: "denied", lastErrorKind: .authExpiredOrRevoked
        )
        let fresh = SourceRecord(
            id: "folder:z", kind: .folder, label: "New", config: "{}", lastSyncedAt: nil,
            lastError: nil, lastErrorKind: nil
        )
        let rows = SourceRow.rows(sources: [feed, folder, odd, fresh], progress: nil, lastRun: nil)
        #expect(rows.map(\.problem) == [.expired(.replaceFeed), .failed(.notFound), .failed(.authExpiredOrRevoked), nil])
        #expect(rows.map(\.status) == [
            .failed(.authExpiredOrRevoked), .failed(.notFound), .failed(.authExpiredOrRevoked), .neverSynced,
        ])
        // Only secrets have a fix, so only the feed would be an arbiter candidate.
        #expect(SourceRow.primaryActionCandidates(rows, canReplaceSecrets: true) == [.fixSource("ical:x"), .pagePrimary])
        #expect(SourceRow.primaryActionCandidates(rows) == [.pagePrimary])
    }

    @Test("during Sync All each source shows done, syncing with progress, or waiting")
    func liveProgress() async {
        let gate = SyncStepGate()
        let (model, _) = makeModel(scenario: .demo, gate: gate)
        await model.refresh()
        let sync = Task { await model.syncAll() }

        // Held before the feed's second step: the folder is done, the feed reported step 1.
        await gate.advance(until: SyncStepPosition(sourceId: "ical:demo-calendar", step: 2))
        let reachedFeed = await until {
            guard let progress = model.syncProgress else { return false }
            return progress.sourceIndex == 2 && progress.current == 1
        }
        #expect(reachedFeed)
        let rows = model.sourceRows
        #expect(rows[0].status == .ok)
        if case .syncing(let current, let total) = rows[1].status {
            #expect((current ?? 0) >= 1)
            #expect(total == 3)
        } else {
            Issue.record("the feed should be syncing, got \(rows[1].status)")
        }
        #expect(rows[2].status == .waiting)
        #expect(rows[0].progress == nil)
        #expect(rows[1].progress?.message != nil)
        #expect(rows[1].progress?.fraction != nil)
        #expect(rows[2].progress == nil)
        // The folder's skipped video is a live warning (shown while it runs, then in Last run).
        #expect(rows[0].liveWarnings.count == 1)
        // Busy: no sync buttons while any sync runs.
        #expect(!model.canStartSync)

        await gate.open()
        await sync.value
        let done = model.sourceRows
        #expect(done.allSatisfy { $0.progress == nil && $0.status == .ok })
        #expect(done[0].lastRun?.ok == true)
        #expect(done[0].lastRun?.warnings.count == 1)
        #expect(done[1].lastRun?.warnings.isEmpty == true)
        #expect(model.canStartSync)

        model.hideResults()
        #expect(model.sourceRows.allSatisfy { $0.lastRun == nil })
    }

    @Test("a single-source sync doesn't mark the other sources as waiting")
    func singleSource() async {
        let gate = SyncStepGate()
        let (model, _) = makeModel(scenario: .demo, gate: gate)
        await model.refresh()
        let sync = Task { await model.syncSource("canvas:canvas.demo.test") }
        #expect(await gate.held() == SyncStepPosition(sourceId: "canvas:canvas.demo.test", step: 1))
        let started = await until { model.syncProgress?.sourceId == "canvas:canvas.demo.test" }
        #expect(started)
        let rows = model.sourceRows
        #expect(rows[0].status == .ok)
        #expect(rows[1].status == .ok)
        if case .syncing = rows[2].status {} else { Issue.record("Canvas should be syncing, got \(rows[2].status)") }
        await gate.open()
        await sync.value
    }

    @Test("the last run keeps each source's failure and the core's English until Hide Results")
    func lastRun() async {
        let (model, _) = makeModel(scenario: .error)
        await model.refresh()
        await model.syncAll()
        let rows = model.sourceRows
        let folder = rows[0]
        #expect(folder.status == .failed(.notFound))
        #expect(folder.problem == .failed(.notFound))
        #expect(folder.lastRun?.ok == false)
        #expect(folder.lastRun?.errorKind == .notFound)
        #expect(folder.lastRun?.error?.contains("was not found") == true)
        #expect(rows[1].lastRun?.ok == true)
        #expect(rows[1].lastRun?.errorKind == nil)
    }

    @Test("another process syncing blocks every sync button (S17)")
    func busy() async {
        let (model, mock) = makeModel(scenario: .busy)
        await model.refresh()
        #expect(model.externalSyncRunning)
        #expect(!model.canStartSync)
        // Sync Now / Sync All too (S17), not only the per-source buttons.
        #expect(!model.canSync)
        await mock.setExternalSyncRunning(false)
        await model.refresh()
        #expect(!model.externalSyncRunning)
        #expect(model.canStartSync)
        #expect(model.canSync)
    }

    @Test("while another process syncs, the app looks again by itself (S17)")
    func busyPoll() async {
        let timers = ManualTimers()
        let (model, mock) = makeModel(scenario: .busy, timers: timers)
        await model.refresh()
        #expect(model.externalSyncRunning)
        #expect(model.isBusyPollScheduled)
        await timers.waitForTimer(.seconds(5))
        let before = model.refreshCount
        await mock.setExternalSyncRunning(false)
        // No explicit refresh: the poll (its timer fired here) finds the lock released.
        await timers.fire(.seconds(5))
        #expect(await until { model.canSync })
        #expect(model.refreshCount == before + 1)
        // Idle again: nothing is scheduled, and no timer waits.
        #expect(!model.isBusyPollScheduled)
        #expect(await timers.pending.isEmpty)
    }

    @Test("no sources, no rows")
    func empty() async {
        let (model, _) = makeModel(scenario: .empty)
        await model.refresh()
        #expect(model.sourceRows.isEmpty)
        #expect(SourceRow.primaryActionCandidates([]) == [.pagePrimary])
    }
}
