// Plan your study: the mock writes plans like the Tauri mock (the gate, the stages, Stop between
// them, the draft's layout, accepting once); the form's rules; a run's events as state, Stop
// sending one cancel per press; "≈ $x" and its blocks; and the words of the draft and its label.

import Foundation
import PageLamp
import PageLampKit
import PageLampModel
import Testing

private let en = L10n(locale: Locale(identifier: "en_US"), table: .app)
private let zh = L10n(locale: Locale(identifier: "zh-Hans_CN"), table: .app)

private func mock(_ scenario: MockScenario, gate: SyncStepGate? = nil) -> MockService {
    MockService(
        scenario: scenario, timing: MockService.Timing(latency: .zero, syncStep: .zero, gate: gate),
        calendar: TestClock.calendar, now: { TestClock.now }
    )
}

private func request(
    horizon: UInt32? = 14, hours: UInt32? = 10, daysOff: [DayOfWeek] = [], courses: [String] = [],
    overrideBudget: Bool = false
) -> StudyPlanRequest {
    StudyPlanRequest(
        horizonDays: horizon, hoursPerWeek: hours, daysOff: daysOff, courses: courses, note: nil,
        overrideBudget: overrideBudget
    )
}

/// The events of one run, in order.
private func events(_ stream: GenEventStream) async -> [GenEvent] {
    stream.finish()
    var all: [GenEvent] = []
    for await event in stream.events { all.append(event) }
    return all
}

/// Counts cancels; the first `failing` of them fail.
private actor CancelLog {
    private(set) var count = 0
    let failing: Int

    init(failing: Int = 0) {
        self.failing = failing
    }

    func record() -> Bool {
        count += 1
        return count <= failing
    }
}

private struct CancelDouble: ForwardingService {
    let base: any PageLampService
    let log: CancelLog

    func cancelGeneration(generationId: String) async throws(PageLampFailure) {
        if await log.record() {
            throw PageLampFailure(kind: .internal, message: "cancel failed")
        }
        try await base.cancelGeneration(generationId: generationId)
    }
}

/// Waits (briefly) until `condition` holds.
@MainActor
private func eventually(_ condition: () async -> Bool) async -> Bool {
    for _ in 0..<2_000 {
        if await condition() { return true }
        try? await Task.sleep(for: .milliseconds(1))
    }
    return await condition()
}

// MARK: - The mock

@Suite("Study plans in the mock")
struct MockPlanTests {
    @Test("a plan needs a model: the gate refuses before any event")
    func needsModel() async throws {
        let stream = GenEventStream()
        do {
            _ = try await mock(.demo).generateStudyPlan(request: request(), generationId: "g1", observer: stream)
            Issue.record("expected blocked")
        } catch {
            #expect(error.kind == .blocked && error.blocked == .noModelChosen)
        }
        #expect(await events(stream).isEmpty)
    }

    @Test("a draft: the events in order, tasks on study days only, at most the day's share")
    func draft() async throws {
        let service = mock(.aiKey)
        let stream = GenEventStream()
        let draft = try await service.generateStudyPlan(
            request: request(daysOff: [.saturday, .sunday]), generationId: "g1", observer: stream
        )
        let names = await events(stream).map { event -> String in
            switch event {
            case .started(let id, let backend, let model, _): "started \(id) \(backend) \(model)"
            case .stage(let stage): AiCodes.name(stage)
            case .usage: "usage"
            case .finished(let ok): "finished \(ok)"
            default: "other"
            }
        }
        #expect(names == [
            "started g1 OpenAI gpt-6-luna", "building_context", "waiting_for_model", "scheduling", "usage", "finished true",
        ])
        #expect(draft.meta.generationId == "g1" && draft.meta.feature == .studyPlan && !draft.meta.onDevice)
        #expect(draft.meta.usage.inputTokens == 9_800 && draft.meta.usage.outputTokens == 1_600)
        #expect(draft.plan.horizonStart == "2026-09-25" && draft.plan.horizonEnd == "2026-10-08")
        #expect(!draft.plan.items.isEmpty)
        // No study on the weekend; at most 10 h / 5 days = 2 h a day.
        var minutes: [String: UInt32] = [:]
        for item in draft.plan.items {
            let date = try #require(IsoDate.date(from: item.date, calendar: TestClock.calendar))
            #expect(!TestClock.calendar.isDateInWeekend(date), "\(item.date) is a weekend")
            minutes[item.date, default: 0] += item.minutes ?? 0
        }
        #expect(minutes.values.allSatisfy { $0 <= 120 })
        #expect(draft.warnings == [PlanWarning(code: .gradedWorkLeftOut, count: 1)])
    }

    @Test("a short plan: what doesn't fit is listed with why")
    func unscheduled() async throws {
        let draft = try await mock(.aiKey).generateStudyPlan(
            request: request(horizon: 2, hours: 2), generationId: "g1", observer: GenEventStream()
        )
        #expect(!draft.unscheduled.isEmpty)
        #expect(draft.unscheduled.allSatisfy { $0.reason == .noTimeBeforeLatest })
    }

    @Test("the request's limits")
    func limits() async throws {
        let service = mock(.aiKey)
        for bad in [request(horizon: 0), request(horizon: 57), request(hours: 0), request(hours: 81),
                    request(daysOff: [.sunday, .monday, .tuesday, .wednesday, .thursday, .friday, .saturday])] {
            do {
                _ = try await service.generateStudyPlan(request: bad, generationId: "g1", observer: GenEventStream())
                Issue.record("expected invalid")
            } catch {
                #expect(error.kind == .invalid)
            }
        }
    }

    @Test("accepting saves PageLamp's plan with its label; a draft is used once")
    func accept() async throws {
        let service = mock(.aiKey)
        let draft = try await service.generateStudyPlan(request: request(), generationId: "g1", observer: GenEventStream())
        let stored = try await service.acceptStudyPlan(generationId: "g1")
        #expect(stored.origin == .pageLamp && stored.generationId == "g1" && stored.plan == draft.plan)
        #expect(stored.aiLabel?.backendLabel == "OpenAI" && stored.aiLabel?.model == "gpt-6-luna")
        #expect(try await service.latestStudyPlan() == stored)
        for (id, kind) in [("g1", PageLampFailure.Kind.invalid), ("nope", .notFound)] {
            do {
                _ = try await service.acceptStudyPlan(generationId: id)
                Issue.record("expected \(kind)")
            } catch {
                #expect(error.kind == kind)
            }
        }
    }

    @Test("Stop: the run ends at its next step, cancelled")
    func stop() async throws {
        let gate = SyncStepGate()
        let service = mock(.aiKey, gate: gate)
        let run = Task {
            try await service.generateStudyPlan(request: request(), generationId: "g1", observer: GenEventStream())
        }
        #expect(await gate.held() == SyncStepPosition(sourceId: "g1", step: 0))
        try await service.cancelGeneration(generationId: "g1")
        await gate.open()
        do {
            _ = try await run.value
            Issue.record("expected cancelled")
        } catch {
            #expect((error as? PageLampFailure)?.kind == .cancelled)
        }
    }
}

// MARK: - The sheet's model

@Suite("Plan your study") @MainActor
struct PlanModelTests {
    func plan(_ service: any PageLampService) async throws -> PlanModel {
        let courses = try await service.listCourses()
        let plan = PlanModel(service: service, courses: courses, debounce: .zero, newId: { "g1" })
        await plan.estimate.settle()
        return plan
    }

    @Test("the form: two weeks, ten hours, the active courses; what's missing, in order")
    func form() async throws {
        let service = mock(.aiKey)
        let plan = try await plan(service)
        let active = try await service.listCourses().filter { !$0.course.hidden && $0.lifecycle.isActive }
        #expect(!active.isEmpty && plan.courses.map(\.course.id) == active.map(\.course.id))
        #expect(plan.horizon == "14" && plan.hours == "10" && plan.problem == nil)
        #expect(plan.request == request(courses: active.map(\.course.id)))
        #expect(plan.estimateRequest == .studyPlan(horizonDays: 14, courses: active.map(\.course.id)))

        for text in ["0", "57", "two", " ", "-1", "1.5"] {
            plan.horizon = text
            #expect(plan.problem == .invalidHorizon, "\(text)")
            #expect(plan.request == nil && plan.estimate.request == nil)
        }
        plan.horizon = " 7 "
        plan.hours = "81"
        #expect(plan.problem == .invalidHours)
        plan.hours = "12"
        for day in ReminderSettingsEditor.weekdays { plan.setDayOff(day, true) }
        #expect(plan.problem == .noStudyDays)
        plan.setDayOff(.wednesday, false)
        for course in active { plan.setPicked(course.course.id, false) }
        #expect(plan.problem == .noCourses)
        plan.setPicked(active[0].course.id, true)
        plan.note = "  the midterm on Friday  "
        let sent = try #require(plan.request)
        #expect(sent.horizonDays == 7 && sent.hoursPerWeek == 12 && sent.courses == [active[0].course.id])
        #expect(sent.daysOff == [.sunday, .monday, .tuesday, .thursday, .friday, .saturday])
        #expect(sent.note == "the midterm on Friday")
        plan.note = String(repeating: "a", count: 600)
        #expect(plan.request?.note?.count == StudyPlanLimits.noteMaxChars)
    }

    @Test("no active course: nothing to plan")
    func nothingToPlan() {
        let plan = PlanModel(service: mock(.aiKey), courses: [], debounce: .zero)
        #expect(plan.nothingToPlan && plan.problem == .noCourses)
    }

    @Test("Write My Plan: its estimate, then the draft; Write Again has its own; Discard drops it")
    func writeAndDiscard() async throws {
        let plan = try await plan(mock(.aiKey))
        #expect(plan.estimate.canGenerate && plan.estimate.showsCost)
        let line = try #require(en.estimateLine(try #require(plan.estimate.estimate), backendKind: plan.estimate.backendKind))
        #expect(line.text.hasPrefix("≈ $") && line.spoken.hasPrefix("Estimated cost: ≈ $"))
        await plan.generate()
        guard case .finished(let draft) = plan.run.phase else {
            Issue.record("expected a draft")
            return
        }
        #expect(draft.meta.generationId == "g1" && plan.draftRequest == plan.request)
        #expect(plan.againEstimate.request == plan.estimateRequest)
        await plan.againEstimate.settle()
        #expect(plan.againEstimate.canGenerate)
        plan.discard()
        guard case .idle = plan.run.phase else {
            Issue.record("expected the form")
            return
        }
        #expect(plan.discarded && plan.draftRequest == nil && plan.againEstimate.request == nil)
    }

    @Test("Use This Plan saves it as PageLamp's plan")
    func useThisPlan() async throws {
        let service = mock(.aiKey)
        let plan = try await plan(service)
        await plan.generate()
        let stored = try #require(await plan.accept())
        #expect(stored.origin == .pageLamp && stored.aiLabel != nil)
        #expect(try await service.latestStudyPlan()?.id == stored.id)
        // A draft is used once: the facade says so, and the sheet keeps the reason.
        #expect(await plan.accept() == nil && plan.acceptFailure?.kind == .invalid)
    }

    @Test("no model: Write My Plan stays off and says why, with no amount")
    func noModel() async throws {
        let plan = try await plan(mock(.demo))
        #expect(plan.estimate.block == .noModelChosen && !plan.estimate.canGenerate && !plan.estimate.showsCost)
        await plan.generate()
        guard case .idle = plan.run.phase else {
            Issue.record("nothing should run")
            return
        }
    }

    @Test("over the budget: off until going over is ticked, for this request only")
    func overBudget() async throws {
        let service = mock(.aiBudget)
        try await service.setMonthlyBudget(microUsd: 4_962_000)
        let plan = try await plan(service)
        #expect(plan.estimate.block == .budgetReached && plan.estimate.showsCost && !plan.estimate.canGenerate)
        plan.estimate.overrideBudget = true
        #expect(plan.estimate.canGenerate)
        // Another request: choose again.
        plan.horizon = "7"
        #expect(!plan.estimate.overrideBudget)
        await plan.estimate.settle()
        plan.estimate.overrideBudget = true
        await plan.generate()
        guard case .finished = plan.run.phase else {
            Issue.record("the run should go over the budget")
            return
        }
    }

    @Test("Stop sends one cancel per press; the run ends stopped")
    func stop() async throws {
        let gate = SyncStepGate()
        let log = CancelLog()
        let plan = try await plan(CancelDouble(base: mock(.aiKey, gate: gate), log: log))
        let run = Task { await plan.generate() }
        _ = await gate.held()
        #expect(await eventually {
            if case .running(let progress) = plan.run.phase {
                return progress.stage == .buildingContext && progress.backend == "OpenAI" && progress.model == "gpt-6-luna"
            }
            return false
        })
        await plan.stop()
        await plan.stop()
        #expect(await log.count == 1)
        if case .running(let progress) = plan.run.phase { #expect(progress.stopping) }
        await gate.open()
        await run.value
        guard case .stopped = plan.run.phase else {
            Issue.record("expected stopped")
            return
        }
        #expect(plan.draftRequest == nil)
    }

    @Test("a cancel that fails lets Stop be pressed again")
    func failedCancel() async throws {
        let gate = SyncStepGate()
        let log = CancelLog(failing: 1)
        let plan = try await plan(CancelDouble(base: mock(.aiKey, gate: gate), log: log))
        let run = Task { await plan.generate() }
        _ = await gate.held()
        #expect(await eventually { plan.run.isRunning })
        await plan.stop()
        if case .running(let progress) = plan.run.phase { #expect(!progress.stopping) }
        await plan.stop()
        #expect(await log.count == 2)
        await gate.open()
        await run.value
        guard case .stopped = plan.run.phase else {
            Issue.record("expected stopped")
            return
        }
    }

    @Test("closing the sheet cancels a run in flight")
    func close() async throws {
        let gate = SyncStepGate()
        let log = CancelLog()
        let plan = try await plan(CancelDouble(base: mock(.aiKey, gate: gate), log: log))
        let run = Task { await plan.generate() }
        _ = await gate.held()
        plan.close()
        #expect(await eventually { await log.count == 1 })
        await gate.open()
        await run.value
    }
}

// MARK: - A run

@Suite("Generation runs") @MainActor
struct GenerationRunTests {
    @Test("events become progress (never text); a second run waits its turn; a failure is kept")
    func progress() async throws {
        let run = GenerationRun<String>(service: mock(.demo), newId: { "g1" })
        let (release, released) = AsyncStream<Void>.makeStream()
        let first = Task {
            await run.run { _, id, observer async throws(PageLampFailure) in
                observer.onEvent(event: .started(generationId: id, backendLabel: "Ollama", model: "qwen3.5:9b", onDevice: true))
                observer.onEvent(event: .context(
                    summary: ContextSummary(courses: [], materialsIncluded: 5, materialsTrimmed: 0, leftOut: []),
                    inputTokens: nil
                ))
                observer.onEvent(event: .textDelta(text: "secret draft"))
                observer.onEvent(event: .stage(stage: .validating))
                for await _ in released { break }
                return "done"
            }
        }
        #expect(await eventually {
            if case .running(let progress) = run.phase { return progress.stage == .validating }
            return false
        })
        if case .running(let progress) = run.phase {
            #expect(progress.backend == "Ollama" && progress.model == "qwen3.5:9b" && progress.materialsIncluded == 5)
        }
        // One run at a time: this one never starts.
        await run.run { _, _, _ async throws(PageLampFailure) in "second" }
        #expect(run.isRunning)
        release.yield()
        await first.value
        guard case .finished(let output) = run.phase else {
            Issue.record("expected finished")
            return
        }
        #expect(output == "done" && !run.isRunning)

        await run.run { _, _, _ async throws(PageLampFailure) in
            throw PageLampFailure(kind: .model, message: "slow", modelError: .timeout)
        }
        guard case .failed(let failure) = run.phase else {
            Issue.record("expected failed")
            return
        }
        #expect(failure.modelError == .timeout)
        run.reset()
        guard case .idle = run.phase else {
            Issue.record("expected idle")
            return
        }
    }
}

// MARK: - Words

@Suite("Plan and estimate text")
struct PlanTextTests {
    let text = PlanText(l10n: en, calendar: TestClock.calendar)

    func estimate(_ upper: UInt64?, input: UInt64 = 45_000) -> CostEstimate {
        CostEstimate(
            microUsdUpper: upper, inputTokens: input, maxOutputTokens: 2_000, reasoningAllowance: 500,
            repairPossible: false, priceKnown: upper != nil, wouldBlock: nil
        )
    }

    @Test("the amount is rounded up to the cent; under a cent, 'less than'")
    func amounts() {
        #expect(en.estimateAmount(microUsd: 69_750) == "≈ $0.07 at most")
        #expect(en.estimateAmount(microUsd: 60_001) == "≈ $0.07 at most")
        #expect(en.estimateAmount(microUsd: 10_000) == "≈ $0.01 at most")
        #expect(en.estimateAmount(microUsd: 9_999) == "≈ less than $0.01")
        #expect(en.estimateLine(estimate(0), backendKind: .local)?.text == "Free, on this computer")
        #expect(en.estimateLine(estimate(69_750), backendKind: .apiKey)?.spoken == "Estimated cost: ≈ $0.07 at most")
        #expect(en.estimateLine(estimate(nil), backendKind: .apiKey)?.text == "No price for this model")
        #expect(en.estimateLine(estimate(nil), backendKind: .local)?.text == "Runs in the cloud; no cost estimate")
        #expect(en.estimateLine(estimate(nil), backendKind: .codex)?.text == "Uses your ChatGPT plan")
        #expect(en.estimateLine(estimate(nil), backendKind: nil) == nil)
        #expect(en.estimateDetails(estimate(3_800)) == "Up to 45K tokens in and 2.5K out")
        #expect(en.estimateDetails(estimate(nil, input: 0)) == nil)
    }

    @Test("the AI label: prefix, service, model, local day, tokens (≈ when estimated)")
    func label() throws {
        let created = try #require(TestClock.calendar.date(from: DateComponents(year: 2026, month: 10, day: 1, hour: 12)))
        func meta(estimated: Bool) -> GenerationMeta {
            GenerationMeta(
                generationId: "g1", feature: .studyPlan, backendLabel: "OpenAI", model: "gpt-6-luna", onDevice: false,
                createdAt: created,
                usage: TokenUsage(inputTokens: 40_000, cachedInputTokens: 0, outputTokens: 3_600, reasoningTokens: nil),
                estCostMicroUsd: nil, estimated: estimated,
                context: ContextSummary(courses: [], materialsIncluded: 0, materialsTrimmed: 0, leftOut: []), promptVersion: 1
            )
        }
        #expect(en.aiLabel(meta(estimated: false), calendar: TestClock.calendar) == "AI-generated · OpenAI · gpt-6-luna · Oct 1, 2026 · 43,600 tokens")
        #expect(en.aiLabel(meta(estimated: true), calendar: TestClock.calendar).hasSuffix(" · ≈ 43,600 tokens"))
        #expect(zh.aiLabel(meta(estimated: true), calendar: TestClock.calendar) == "AI 生成 · OpenAI · gpt-6-luna · 2026年10月1日 · ≈ 43,600 个 token")
        let saved = AiLabel(backendLabel: "Ollama", model: "qwen3.5:9b", createdAt: created, onDevice: true)
        #expect(en.aiLabel(saved, calendar: TestClock.calendar) == "AI-generated · Ollama · qwen3.5:9b · Oct 1, 2026")
    }

    @Test("the draft's words")
    func draftWords() {
        let plan = StudyPlan(horizonStart: "2026-09-25", horizonEnd: "2026-10-08", items: [
            StudyPlanItem(date: "2026-09-26", courseId: "c1", title: "B", description: nil, materialIds: [], minutes: 45, done: false),
            StudyPlanItem(date: "2026-09-25", courseId: "c1", title: "A", description: nil, materialIds: [], minutes: 30, done: false),
            StudyPlanItem(date: "2026-09-26", courseId: "c2", title: "C", description: nil, materialIds: [], minutes: nil, done: false),
        ], notes: nil)
        #expect(text.summary(plan) == "3 tasks, Sep 25 to Oct 8")
        #expect(text.day("2026-09-25") == "Fri, Sep 25")
        #expect(text.minutes(45) == "45 min")
        #expect(text.warning(PlanWarning(code: .gradedWorkLeftOut, count: 1)) == "1 task was left out because it looked like an answer to graded work.")
        #expect(text.reason(.noTimeBeforeLatest) == "no time left before it's due")
        #expect(text.structureOnly(["DEMO101", "DEMO205"]) == "Planned from structure only (no material text was shared): DEMO101 and DEMO205.")
        #expect(text.structureOnly([]) == nil)
        #expect(PlanText.days(plan.items).map(\.date) == ["2026-09-25", "2026-09-26"])
        #expect(PlanText.days(plan.items).last?.items.map(\.title) == ["B", "C"])
        #expect(text.stage(nil) == "Writing your plan" && text.stage(.scheduling) == "Placing tasks on your study days")
        for stage in [GenStage.buildingContext, .waitingForModel, .validating, .repairing, .scheduling] {
            for feature in ["plan", "explain", "weeklyNote"] {
                #expect(en.has("\(feature).running.stage.\(AiCodes.name(stage))"))
            }
        }
        for reason in [UnscheduledReason.outsideHorizon, .noStudyDays, .noTimeBeforeLatest, .tooManyItems] {
            #expect(en.has("plan.unscheduled.reason.\(AiCodes.name(reason))"))
        }
    }
}
