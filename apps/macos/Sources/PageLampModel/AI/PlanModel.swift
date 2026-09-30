// Plan your study (design §5.1, §7; the Tauri app's PlanPage): what to plan, the run with its
// stages and Stop, then the draft to use, write again or discard. Nothing is saved until "Use
// This Plan". The form's rules are the Tauri app's (the facade checks the same limits); the
// courses offered are the active ones, the same set the facade plans when given none.

import Foundation
import Observation
import PageLampKit

@Observable @MainActor
public final class PlanModel {
    /// What's still missing before anything can be sent (a hint, not an error).
    public enum Problem: Equatable, Sendable {
        case invalidHorizon
        case invalidHours
        case noStudyDays
        case noCourses
    }

    // MARK: The form

    /// "Days to plan" as typed.
    public var horizon: String { didSet { formChanged() } }
    /// "Study hours per week" as typed.
    public var hours: String { didSet { formChanged() } }
    public var daysOff: Set<DayOfWeek> { didSet { formChanged() } }
    /// The courses ticked (by id); nil: every active course.
    public var picked: [String]? { didSet { formChanged() } }
    /// "Anything to focus on?" (at most `StudyPlanLimits.noteMaxChars` characters are sent).
    public var note: String

    /// The active courses, in the app's order.
    public let courses: [CourseSummary]

    // MARK: The run and its outcome

    public let run: GenerationRun<GeneratedStudyPlan>
    /// "≈ $x" for Write My Plan, and for Write Again on a draft.
    public let estimate: CostEstimateModel
    public let againEstimate: CostEstimateModel
    /// The request of the draft shown (Write Again sends it again).
    public private(set) var draftRequest: StudyPlanRequest?
    /// The draft was discarded (said once, where the draft was).
    public private(set) var discarded = false
    public private(set) var accepting = false
    public private(set) var acceptFailure: PageLampFailure?

    @ObservationIgnored private let service: any PageLampService

    /// - Parameters:
    ///   - courses: every course (the active, visible ones are offered).
    ///   - initial: the last run's request, to start from.
    public init(
        service: any PageLampService,
        courses: [CourseSummary],
        initial: StudyPlanRequest? = nil,
        debounce: Duration = .milliseconds(300),
        newId: @escaping @Sendable () -> String = randomGenerationId
    ) {
        self.service = service
        self.courses = courses.filter { !$0.course.hidden && $0.lifecycle.isActive }
        horizon = String(initial?.horizonDays ?? StudyPlanLimits.defaultHorizonDays)
        hours = String(initial?.hoursPerWeek ?? StudyPlanLimits.defaultHoursPerWeek)
        daysOff = Set(initial?.daysOff ?? [])
        picked = initial.map(\.courses).flatMap { $0.isEmpty ? nil : $0 }
        note = initial?.note ?? ""
        run = GenerationRun(service: service, newId: newId)
        estimate = CostEstimateModel(service: service, debounce: debounce)
        againEstimate = CostEstimateModel(service: service, debounce: debounce)
        estimate.update(estimateRequest)
    }

    /// No course is active: there's nothing to plan.
    public var nothingToPlan: Bool { courses.isEmpty }

    /// The courses the plan covers: the ticked ones, else every active one.
    public var chosen: [String] { picked ?? courses.map(\.course.id) }

    public var horizonDays: UInt32? {
        Self.wholeNumber(horizon, StudyPlanLimits.minHorizonDays...StudyPlanLimits.maxHorizonDays)
    }

    public var hoursPerWeek: UInt32? {
        Self.wholeNumber(hours, StudyPlanLimits.minHoursPerWeek...StudyPlanLimits.maxHoursPerWeek)
    }

    public var problem: Problem? {
        if horizonDays == nil { return .invalidHorizon }
        if hoursPerWeek == nil { return .invalidHours }
        if daysOff.count >= 7 { return .noStudyDays }
        if chosen.isEmpty { return .noCourses }
        return nil
    }

    /// What Write My Plan sends; nil while there's a problem.
    public var request: StudyPlanRequest? {
        guard problem == nil, let horizonDays, let hoursPerWeek else { return nil }
        let note = String(note.prefix(StudyPlanLimits.noteMaxChars)).trimmingCharacters(in: .whitespacesAndNewlines)
        return StudyPlanRequest(
            horizonDays: horizonDays, hoursPerWeek: hoursPerWeek,
            daysOff: ReminderSettingsEditor.weekdays.filter(daysOff.contains), courses: chosen,
            note: note.isEmpty ? nil : note, overrideBudget: false
        )
    }

    public var estimateRequest: EstimateRequest? {
        request.map { .studyPlan(horizonDays: $0.horizonDays, courses: $0.courses) }
    }

    public func isPicked(_ course: String) -> Bool { chosen.contains(course) }

    public func setPicked(_ course: String, _ on: Bool) {
        let current = chosen
        picked = on ? (current.contains(course) ? current : current + [course]) : current.filter { $0 != course }
    }

    public func setDayOff(_ day: DayOfWeek, _ off: Bool) {
        if off { daysOff.insert(day) } else { daysOff.remove(day) }
    }

    private func formChanged() {
        estimate.update(estimateRequest)
    }

    // MARK: - Actions

    /// Write My Plan (going over the budget if the student ticked it).
    public func generate() async {
        guard let request, estimate.canGenerate else { return }
        await start(request.with(overrideBudget: estimate.overrideBudget))
    }

    /// Write Again: the draft's request, as a new run with its own "≈ $x".
    public func writeAgain() async {
        guard let draftRequest, againEstimate.canGenerate, !accepting else { return }
        await start(draftRequest.with(overrideBudget: againEstimate.overrideBudget))
    }

    private func start(_ request: StudyPlanRequest) async {
        discarded = false
        acceptFailure = nil
        againEstimate.update(nil)
        await run.run { service, id, observer async throws(PageLampFailure) in
            try await service.generateStudyPlan(request: request, generationId: id, observer: observer)
        }
        if case .finished = run.phase {
            draftRequest = request
            againEstimate.update(.studyPlan(horizonDays: request.horizonDays, courses: request.courses))
        } else {
            draftRequest = nil
        }
        // Usage and the budget changed: the next estimate is new.
        await estimate.refresh()
    }

    public func stop() async {
        await run.stop()
    }

    /// Discard: back to the form, and nothing is saved.
    public func discard() {
        guard case .finished = run.phase, !accepting else { return }
        run.reset()
        draftRequest = nil
        againEstimate.update(nil)
        discarded = true
    }

    /// Use This Plan: saves the draft as the study plan. Returns it, or nil with `acceptFailure`.
    public func accept() async -> StoredStudyPlan? {
        guard case .finished(let draft) = run.phase, !accepting else { return nil }
        accepting = true
        acceptFailure = nil
        defer { accepting = false }
        do throws(PageLampFailure) {
            return try await service.acceptStudyPlan(generationId: draft.meta.generationId)
        } catch {
            acceptFailure = error
            return nil
        }
    }

    /// The sheet closed: a run in flight stops (nothing keeps writing, or costing, out of sight).
    public func close() {
        run.cancelInFlight()
    }

    /// A whole number within the limits, else nil.
    static func wholeNumber(_ text: String, _ limits: ClosedRange<UInt32>) -> UInt32? {
        let trimmed = text.trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty, trimmed.allSatisfy({ $0.isASCII && $0.isNumber }), let value = UInt32(trimmed),
              limits.contains(value)
        else { return nil }
        return value
    }
}

extension StudyPlanRequest {
    /// The same request, going over the budget or not.
    func with(overrideBudget: Bool) -> StudyPlanRequest {
        StudyPlanRequest(
            horizonDays: horizonDays, hoursPerWeek: hoursPerWeek, daysOff: daysOff, courses: courses, note: note,
            overrideBudget: overrideBudget
        )
    }
}
