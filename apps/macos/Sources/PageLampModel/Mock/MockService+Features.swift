// The mock's M1–M3 calls (AI, course calendars, removal, reminders). Settings, removing and
// restoring courses and reminders behave like the facade on the mock's own data. The mock has
// no model: every generation and syllabus reading ends `blocked` like the facade without one,
// and installing or signing in to Codex fails like a machine that can't reach it.

import Foundation
import PageLampKit

/// The M1–M3 state the mock keeps.
struct MockFeatures: Sendable {
    var featureModels: [AiFeature: ModelChoice] = [:]
    var monthlyBudget: UInt64?
    var sharing: [String: MaterialSharing] = [:]
    var outputLanguage: OutputLanguage = .ui
    var weeklyCap: UInt32?
    var codexSource: CodexSource = .managed
    var reminderSettings = ReminderSettings(
        deadlineSoon: true, weeklyDigest: true, digestDay: .monday, digestTime: "09:00",
        planToday: false, planTodayTime: "08:00", runInBackground: false
    )
    var shownReminders: Set<String> = []
    /// Removed courses, with the course to put back on restore.
    var removed: [MockRemoval] = []
}

struct MockRemoval: Sendable {
    var record: RemovedCourse
    var course: MockCourse
}

extension MockService {
    static let noModel = PageLampFailure(kind: .blocked, message: "Set up a model first: the preview's mock data has none.")

    // MARK: - AI setup

    public func modelProviderPresets() async throws(PageLampFailure) -> [ProviderPreset] {
        await respond("modelProviderPresets")
        return []
    }

    public func detectLocalServers() async throws(PageLampFailure) -> [LocalServer] {
        await respond("detectLocalServers")
        return []
    }

    public func aiStatus() async throws(PageLampFailure) -> AiStatus {
        await respond("aiStatus")
        let features = db.features.featureModels
            .map { FeatureRouting(feature: $0.key, choice: $0.value) }
            .sorted { "\($0.feature)" < "\($1.feature)" }
        return AiStatus(backends: [], providers: [], features: features, budget: budget())
    }

    public func addModelProvider(preset: String, baseUrl: String?, apiKey: String?) async throws(PageLampFailure) -> ModelProviderRecord {
        await respond("addModelProvider")
        throw PageLampFailure(kind: .network, message: "The preview's mock can't reach \(preset).")
    }

    public func updateModelProviderKey(providerId: String, apiKey: String) async throws(PageLampFailure) -> ModelProviderRecord {
        await respond("updateModelProviderKey")
        throw Self.noProvider(providerId)
    }

    public func removeModelProvider(providerId: String) async throws(PageLampFailure) {
        await respond("removeModelProvider")
        throw Self.noProvider(providerId)
    }

    public func listModels(backend: BackendRef) async throws(PageLampFailure) -> [ModelInfo] {
        await respond("listModels")
        throw Self.noModel
    }

    public func testModel(backend: BackendRef, model: String) async throws(PageLampFailure) -> ProbeReport {
        await respond("testModel")
        throw Self.noModel
    }

    public func setFeatureModel(feature: AiFeature, choice: ModelChoice?) async throws(PageLampFailure) {
        await respond("setFeatureModel")
        db.features.featureModels[feature] = choice
    }

    public func acknowledgeAiDisclosure(backend: BackendRef, version: UInt32) async throws(PageLampFailure) {
        await respond("acknowledgeAiDisclosure")
    }

    public func acknowledgeUnpricedModel(backend: BackendRef, model: String) async throws(PageLampFailure) {
        await respond("acknowledgeUnpricedModel")
    }

    public func setMonthlyBudget(microUsd: UInt64?) async throws(PageLampFailure) {
        await respond("setMonthlyBudget")
        db.features.monthlyBudget = microUsd
    }

    public func estimateGeneration(request: EstimateRequest) async throws(PageLampFailure) -> CostEstimate {
        await respond("estimateGeneration")
        throw Self.noModel
    }

    public func usageSummary(month: String?) async throws(PageLampFailure) -> UsageSummary {
        await respond("usageSummary")
        let first = month.map { String($0.prefix(7)) + "-01" } ?? String(isoDay(daysFromToday: 0).prefix(7)) + "-01"
        return UsageSummary(month: first, rows: [], totalMicroUsd: 0, budget: budget(), modeA: nil)
    }

    public func setCourseMaterialSharing(course reference: String, answer: MaterialSharing) async throws(PageLampFailure) {
        await respond("setCourseMaterialSharing")
        db.features.sharing[db.courses[try courseIndex(reference)].course.id] = answer
    }

    public func deleteGenerated(course reference: String?) async throws(PageLampFailure) -> UInt32 {
        await respond("deleteGenerated")
        if let reference { _ = try courseIndex(reference) }
        return 0
    }

    public func removeAllAiData() async throws(PageLampFailure) -> RemoveAiDataReport {
        await respond("removeAllAiData")
        db.features.featureModels = [:]
        db.features.monthlyBudget = nil
        db.features.sharing = [:]
        return RemoveAiDataReport(providersRemoved: 0, generationsRemoved: 0, usageRowsRemoved: 0, backupRemoved: false)
    }

    private func budget() -> BudgetStatus {
        BudgetStatus(monthlyMicroUsd: db.features.monthlyBudget, spentMicroUsd: 0, warnAtPercent: 80)
    }

    private static func noProvider(_ id: String) -> PageLampFailure {
        PageLampFailure(kind: .notFound, message: "No model provider with id \(id)")
    }

    // MARK: - The ChatGPT plan through Codex (not installed, can't be downloaded here)

    public func codexStatus() async throws(PageLampFailure) -> CodexStatus {
        await respond("codexStatus")
        return codex()
    }

    public func installCodex(installId: String, observer: any CodexInstallObserver) async throws(PageLampFailure) -> CodexStatus {
        await respond("installCodex")
        throw PageLampFailure(kind: .network, message: "The preview's mock can't download Codex.")
    }

    public func cancelCodexInstall(installId: String) async throws(PageLampFailure) {
        await respond("cancelCodexInstall")
    }

    public func removeCodex() async throws(PageLampFailure) {
        await respond("removeCodex")
    }

    public func codexLogin(method: CodexLoginMethod, observer: any CodexLoginObserver) async throws(PageLampFailure) -> CodexStatus {
        await respond("codexLogin")
        throw PageLampFailure(kind: .blocked, message: "Install Codex first: the preview's mock has none.")
    }

    public func cancelCodexLogin() async throws(PageLampFailure) {
        await respond("cancelCodexLogin")
    }

    public func codexLogout() async throws(PageLampFailure) -> CodexStatus {
        await respond("codexLogout")
        return codex()
    }

    public func setModeAWeeklyCap(runs: UInt32?) async throws(PageLampFailure) {
        await respond("setModeAWeeklyCap")
        db.features.weeklyCap = runs
    }

    public func setCodexSource(source: CodexSource) async throws(PageLampFailure) -> CodexStatus {
        await respond("setCodexSource")
        db.features.codexSource = source
        return codex()
    }

    private func codex() -> CodexStatus {
        CodexStatus(
            runtime: CodexRuntime(
                state: .notInstalled, source: db.features.codexSource, installedVersion: nil,
                pinnedVersion: "0.0.0-mock", downloadBytes: 0, untestedPlatform: false
            ),
            outdatedAction: .none,
            login: CodexLogin(state: .signedOut, planType: nil),
            execAvailable: nil,
            weeklyCap: db.features.weeklyCap,
            runsThisWeek: 0,
            systemCodex: nil
        )
    }

    // MARK: - Explanations and study plans (no model: blocked, like the facade without one)

    public func explainWeek(
        course reference: String, week: UInt32?, generationId: String, options: ExplainOptions, observer: any GenObserver
    ) async throws(PageLampFailure) -> WeeklyExplanation {
        await respond("explainWeek")
        _ = try courseIndex(reference)
        throw Self.noModel
    }

    public func savedExplanations(course reference: String, week: UInt32?) async throws(PageLampFailure) -> [WeeklyExplanation] {
        await respond("savedExplanations")
        _ = try courseIndex(reference)
        return []
    }

    public func deleteExplanation(generationId: String) async throws(PageLampFailure) {
        await respond("deleteExplanation")
        throw PageLampFailure(kind: .notFound, message: "No explanation with id \(generationId)")
    }

    public func aiOutputLanguage() async throws(PageLampFailure) -> OutputLanguage {
        await respond("aiOutputLanguage")
        return db.features.outputLanguage
    }

    public func setAiOutputLanguage(language: OutputLanguage) async throws(PageLampFailure) {
        await respond("setAiOutputLanguage")
        db.features.outputLanguage = language
    }

    public func generateStudyPlan(
        request: StudyPlanRequest, generationId: String, observer: any GenObserver
    ) async throws(PageLampFailure) -> GeneratedStudyPlan {
        await respond("generateStudyPlan")
        throw Self.noModel
    }

    public func acceptStudyPlan(generationId: String) async throws(PageLampFailure) -> StoredStudyPlan {
        await respond("acceptStudyPlan")
        throw PageLampFailure(kind: .notFound, message: "No study plan draft with id \(generationId)")
    }

    public func setStudyPlanItemDone(planId: Int64, itemIndex: UInt32, done: Bool) async throws(PageLampFailure) -> StoredStudyPlan {
        await respond("setStudyPlanItemDone")
        guard let stored = db.studyPlan, stored.id == planId else {
            throw PageLampFailure(kind: .notFound, message: "No study plan with id \(planId)")
        }
        let index = Int(itemIndex)
        guard stored.plan.items.indices.contains(index) else {
            throw PageLampFailure(kind: .invalid, message: "The plan has no item \(itemIndex)")
        }
        var items = stored.plan.items
        let item = items[index]
        items[index] = StudyPlanItem(
            date: item.date, courseId: item.courseId, title: item.title, description: item.description,
            materialIds: item.materialIds, minutes: item.minutes, done: done
        )
        let updated = StoredStudyPlan(
            id: stored.id, createdAt: stored.createdAt,
            plan: StudyPlan(
                horizonStart: stored.plan.horizonStart, horizonEnd: stored.plan.horizonEnd, items: items,
                notes: stored.plan.notes
            ),
            origin: stored.origin, generationId: stored.generationId, aiLabel: stored.aiLabel
        )
        db.studyPlan = updated
        return updated
    }

    public func cancelGeneration(generationId: String) async throws(PageLampFailure) {
        await respond("cancelGeneration")
    }

    // MARK: - Course calendars (none read or proposed in the mock)

    public func courseCalendar(course reference: String) async throws(PageLampFailure) -> CourseCalendarView {
        await respond("courseCalendar")
        return calendarView(db.courses[try courseIndex(reference)])
    }

    public func calendarCandidates(course reference: String) async throws(PageLampFailure) -> [CalendarCandidate] {
        await respond("calendarCandidates")
        _ = try courseIndex(reference)
        return []
    }

    public func setCalendarSources(course reference: String, include: [String], exclude: [String]) async throws(PageLampFailure) -> [CalendarCandidate] {
        await respond("setCalendarSources")
        _ = try courseIndex(reference)
        return []
    }

    public func downloadMaterialFiles(
        course reference: String, materialIds: [String], observer: any SyncObserver
    ) async throws(PageLampFailure) -> SourceSyncResult {
        await respond("downloadMaterialFiles")
        _ = try courseIndex(reference)
        throw PageLampFailure(kind: .invalid, message: "The preview's mock has no files to download.")
    }

    public func scanCourseCalendar(course reference: String) async throws(PageLampFailure) -> CalendarProposal? {
        await respond("scanCourseCalendar")
        _ = try courseIndex(reference)
        return nil
    }

    public func setCourseDates(course reference: String, dates: CourseDatesInput?) async throws(PageLampFailure) -> CourseCalendarView {
        await respond("setCourseDates")
        return calendarView(db.courses[try courseIndex(reference)])
    }

    public func acceptCalendarProposal(proposalId: Int64, edits: CourseDatesInput?) async throws(PageLampFailure) -> CourseCalendarView {
        await respond("acceptCalendarProposal")
        throw Self.noProposal(proposalId)
    }

    public func acceptPassingProposals(proposalIds: [Int64]) async throws(PageLampFailure) -> [CourseCalendarView] {
        await respond("acceptPassingProposals")
        if let first = proposalIds.first { throw Self.noProposal(first) }
        return []
    }

    public func dismissCalendarProposal(proposalId: Int64) async throws(PageLampFailure) {
        await respond("dismissCalendarProposal")
        throw Self.noProposal(proposalId)
    }

    public func syllabusReadingOffers() async throws(PageLampFailure) -> [SyllabusOffer] {
        await respond("syllabusReadingOffers")
        return []
    }

    public func snoozeCalendarOffers() async throws(PageLampFailure) {
        await respond("snoozeCalendarOffers")
    }

    public func readCourseCalendar(
        course reference: String, generationId: String, options: ReadCalendarOptions, observer: any GenObserver
    ) async throws(PageLampFailure) -> CalendarProposal {
        await respond("readCourseCalendar")
        _ = try courseIndex(reference)
        throw Self.noModel
    }

    public func readCourseCalendars(
        courses: [String], batchId: String, options: ReadCalendarOptions, observer: any CalendarBatchObserver
    ) async throws(PageLampFailure) -> [CalendarRunOutcome] {
        await respond("readCourseCalendars")
        throw Self.noModel
    }

    private func calendarView(_ course: MockCourse) -> CourseCalendarView {
        CourseCalendarView(
            courseId: course.course.id, accepted: nil, proposals: [], status: course.timeline.calendar,
            candidates: [], blocked: nil
        )
    }

    private static func noProposal(_ id: Int64) -> PageLampFailure {
        PageLampFailure(kind: .notFound, message: "No calendar proposal \(id)")
    }

    // MARK: - Removing finished courses (7 days to undo, like the facade)

    public func removalPreview(courses: [String]) async throws(PageLampFailure) -> RemovalPreview {
        await respond("removalPreview")
        var items: [RemovalPreviewItem] = []
        for reference in courses {
            let course = db.courses[try courseIndex(reference)]
            let kind = db.sources.first { $0.id == course.course.sourceId }?.kind ?? .folder
            items.append(RemovalPreviewItem(
                courseId: course.course.id, code: course.course.code, name: course.course.name, sourceKind: kind,
                lifecycle: summary(course).lifecycle, materials: UInt32(course.materials.count),
                downloadedFiles: 0, downloadedBytes: 0, deadlines: UInt32(course.deadlines.count),
                generatedItems: 0, customSettings: false, ownFolderUntouched: kind == .folder,
                cannotSyncAgain: !course.course.enrollmentActive, lostAfterPurge: []
            ))
        }
        return RemovalPreview(items: items, backup: nil)
    }

    public func removeCourses(courses: [String], options: RemoveOptions) async throws(PageLampFailure) -> RemovalReport {
        await respond("removeCourses")
        var indices: [Int] = []
        for reference in courses { indices.append(try courseIndex(reference)) }
        let removedAt = now()
        let purgeAfter = options.purgeNow ? nil : removedAt.addingTimeInterval(7 * 86_400)
        var removed: [RemovedCourse] = []
        for index in indices.sorted(by: >) {
            let course = db.courses.remove(at: index)
            let kind = db.sources.first { $0.id == course.course.sourceId }?.kind ?? .folder
            let record = RemovedCourse(
                removedId: "removed-\(course.course.id)", sourceId: course.course.sourceId, sourceKind: kind,
                externalId: course.course.externalId, courseId: course.course.id, code: course.course.code,
                name: course.course.name, reason: options.reason ?? .other,
                state: options.purgeNow ? .purged : .pending, removedAt: removedAt, purgeAfter: purgeAfter,
                purgedAt: options.purgeNow ? removedAt : nil, purgeInDays: options.purgeNow ? nil : 7,
                keepFiles: options.keepDownloadedFiles, filesPending: false
            )
            db.features.removed.append(MockRemoval(record: record, course: course))
            removed.insert(record, at: 0)
        }
        return RemovalReport(removed: removed, purgedNow: options.purgeNow, backupDeleted: false, backupFailed: false)
    }

    public func removedCourses() async throws(PageLampFailure) -> [RemovedCourse] {
        await respond("removedCourses")
        return db.features.removed.map(\.record)
    }

    public func restoreCourse(removedId: String) async throws(PageLampFailure) -> RestoreOutcome {
        await respond("restoreCourse")
        guard let index = db.features.removed.firstIndex(where: { $0.record.removedId == removedId }) else {
            throw PageLampFailure(kind: .notFound, message: "No removed course \(removedId)")
        }
        let removal = db.features.removed[index]
        guard removal.record.state == .pending else {
            return RestoreOutcome(restored: false, courseId: nil, failure: .other)
        }
        db.features.removed.remove(at: index)
        db.courses.append(removal.course)
        return RestoreOutcome(restored: true, courseId: removal.course.course.id, failure: nil)
    }

    public func purgeRemovedCourses(removedIds: [String]?, permanentIfNoTrash: Bool) async throws(PageLampFailure) -> PurgeReport {
        await respond("purgeRemovedCourses")
        let at = now()
        var purged: [String] = []
        for index in db.features.removed.indices {
            let record = db.features.removed[index].record
            let chosen = removedIds.map { $0.contains(record.removedId) } ?? ((record.purgeAfter ?? .distantFuture) <= at)
            guard chosen, record.state == .pending else { continue }
            db.features.removed[index].record = RemovedCourse(
                removedId: record.removedId, sourceId: record.sourceId, sourceKind: record.sourceKind,
                externalId: record.externalId, courseId: record.courseId, code: record.code, name: record.name,
                reason: record.reason, state: .purged, removedAt: record.removedAt, purgeAfter: record.purgeAfter,
                purgedAt: at, purgeInDays: nil, keepFiles: record.keepFiles, filesPending: false
            )
            purged.append(record.removedId)
        }
        return PurgeReport(purged: purged, filesPending: [], backupDeleted: false, backupFailed: false)
    }

    public func forgetRemovedCourse(removedId: String) async throws(PageLampFailure) {
        await respond("forgetRemovedCourse")
        guard let index = db.features.removed.firstIndex(where: { $0.record.removedId == removedId }) else {
            throw PageLampFailure(kind: .notFound, message: "No removed course \(removedId)")
        }
        db.features.removed.remove(at: index)
    }

    // MARK: - Reminders and the weekly digest

    public func weeklyDigest() async throws(PageLampFailure) -> WeeklyDigest {
        await respond("weeklyDigest")
        return WeeklyDigest(generatedAt: now(), courses: [], plan: nil)
    }

    public func reminderSettings() async throws(PageLampFailure) -> ReminderSettings {
        await respond("reminderSettings")
        return db.features.reminderSettings
    }

    public func setReminderSettings(settings: ReminderSettings) async throws(PageLampFailure) {
        await respond("setReminderSettings")
        db.features.reminderSettings = settings
    }

    public func reminders(from: Date, to: Date) async throws(PageLampFailure) -> [Reminder] {
        await respond("reminders")
        return deadlineReminders().filter { $0.fireAt >= from && $0.fireAt <= to }
    }

    public func dueReminders(now date: Date) async throws(PageLampFailure) -> [Reminder] {
        await respond("dueReminders")
        let catchUp = date.addingTimeInterval(-3 * 86_400)
        return deadlineReminders().filter {
            $0.fireAt <= date && $0.fireAt >= catchUp && !db.features.shownReminders.contains($0.id)
        }
    }

    public func markRemindersShown(ids: [String]) async throws(PageLampFailure) {
        await respond("markRemindersShown")
        db.features.shownReminders.formUnion(ids)
    }

    /// "Due soon" 24 hours before each visible course's deadline, when that reminder is on.
    private func deadlineReminders() -> [Reminder] {
        guard db.features.reminderSettings.deadlineSoon else { return [] }
        let zone = calendar.timeZone.identifier
        return db.courses.filter { !$0.course.hidden }.flatMap { course in
            course.deadlines.compactMap { deadline -> Reminder? in
                guard let due = deadline.event.dueAt, deadline.event.kind != .classEvent else { return nil }
                let fire = due.addingTimeInterval(-24 * 3600)
                let parts = calendar.dateComponents([.hour, .minute], from: fire)
                return Reminder(
                    id: "\(deadline.event.id)@\(Int(due.timeIntervalSince1970))", kind: .deadlineSoon,
                    localTime: String(format: "%02d:%02d", parts.hour ?? 0, parts.minute ?? 0), timeZone: zone,
                    fireAt: fire, title: deadline.event.title, courseId: course.course.id,
                    courseCode: course.course.code, courseName: course.course.name, dueAt: due,
                    hoursBefore: 24, count: nil
                )
            }
        }
        .sorted { $0.fireAt < $1.fireAt }
    }

    // MARK: - Files and syncs

    public func materialLocalFile(materialId: String, purpose: LocalFileUse) async throws(PageLampFailure) -> String? {
        await respond("materialLocalFile")
        return nil
    }

    public func cancelSync() async throws(PageLampFailure) {
        await respond("cancelSync")
    }
}
