// Shared helpers for the model tests. Nothing here touches the real data folder, the keychain,
// the user's preferences (settings live in memory) or the network.

import Foundation
import PageLamp
import PageLampKit
import PageLampModel

/// Friday 2026-09-25 10:00 in a fixed time zone (the Tauri tests' clock).
enum TestClock {
    static let calendar: Calendar = {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "America/Toronto") ?? .gmt
        calendar.locale = Locale(identifier: "en_US")
        return calendar
    }()

    static let now: Date = calendar.date(from: DateComponents(year: 2026, month: 9, day: 25, hour: 10)) ?? Date()

    /// `days` calendar days after `now`, at `hour`:`minute`.
    static func at(_ days: Int, _ hour: Int, _ minute: Int = 0) -> Date {
        let day = calendar.date(byAdding: .day, value: days, to: calendar.startOfDay(for: now)) ?? now
        return calendar.date(bySettingHour: hour, minute: minute, second: 0, of: day) ?? day
    }
}

@MainActor
func makeModel(
    scenario: MockScenario = .demo,
    syncStep: Duration = .zero,
    language: AppLanguage = .english,
    preferredLanguages: [String] = ["en-US"],
    settings: InMemorySettingsStore? = nil,
    notificationCenter: NotificationCenter = NotificationCenter()
) -> (AppModel, MockService) {
    let mock = MockService(
        scenario: scenario,
        timing: MockService.Timing(latency: .zero, syncStep: syncStep),
        calendar: TestClock.calendar,
        now: { TestClock.now }
    )
    let model = AppModel(
        dataMode: .mock(scenario),
        strings: .app,
        settings: settings ?? InMemorySettingsStore(language: language),
        timing: AppModel.Timing(finishedCapsule: .milliseconds(60), failedCapsule: .milliseconds(60), mock: .instant),
        calendar: TestClock.calendar,
        clock: { TestClock.now },
        notificationCenter: notificationCenter,
        preferredLanguages: { preferredLanguages },
        service: mock
    )
    return (model, mock)
}

/// Polls `condition` on the main actor until it holds or `timeout` passes.
@MainActor
func eventually(timeout: Duration = .seconds(5), _ condition: () -> Bool) async -> Bool {
    let clock = ContinuousClock()
    let deadline = clock.now + timeout
    while !condition() {
        if clock.now > deadline { return false }
        try? await Task.sleep(for: .milliseconds(2))
    }
    return true
}

/// Records every sync event (thread-safe; Rust or the mock may call from any thread).
final class EventRecorder: SyncObserver, @unchecked Sendable {
    private let lock = NSLock()
    private var recorded: [SyncEvent] = []

    func onEvent(event: SyncEvent) {
        lock.withLock { recorded.append(event) }
    }

    var events: [SyncEvent] {
        lock.withLock { recorded }
    }
}

func deadline(_ title: String, _ kind: EventKind, day: Int, hour: Int) -> Deadline {
    let when = TestClock.at(day, hour)
    let isDue = kind != .classEvent && kind != .exam
    return Deadline(
        event: Event(
            id: title, sourceId: "ical:test", courseId: nil, kind: kind, title: title,
            startsAt: isDue ? nil : when, endsAt: nil, dueAt: isDue ? when : nil,
            url: nil, updatedAt: TestClock.now, courseHint: nil
        ),
        courseCode: nil,
        courseName: nil
    )
}
