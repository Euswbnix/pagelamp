// Shared helpers for the model tests. Nothing here touches the real data folder, the keychain,
// the user's preferences (settings live in memory) or the network.

import Foundation
import Observation
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

/// A model over the mock. Its timers (the capsule's, the busy poll, the source highlight) never
/// fire by themselves: they wait on `timers` until the test fires them. A `gate` holds the mock's
/// sync before every progress step (`SyncStepGate`), so a test can look at the sync mid-run.
@MainActor
func makeModel(
    scenario: MockScenario = .demo,
    gate: SyncStepGate? = nil,
    timers: ManualTimers = ManualTimers(),
    language: AppLanguage = .english,
    preferredLanguages: [String] = ["en-US"],
    settings: InMemorySettingsStore? = nil,
    notificationCenter: NotificationCenter = NotificationCenter(),
    wrap: (MockService) -> any PageLampService = { $0 }
) -> (AppModel, MockService) {
    let mock = MockService(
        scenario: scenario,
        timing: MockService.Timing(latency: .zero, syncStep: .zero, gate: gate),
        calendar: TestClock.calendar,
        now: { TestClock.now }
    )
    let model = AppModel(
        dataMode: .mock(scenario),
        strings: .app,
        settings: settings ?? InMemorySettingsStore(language: language),
        timing: AppModel.Timing(
            finishedCapsule: .seconds(4), failedCapsule: .seconds(6), mock: .instant,
            busyPoll: .seconds(5), sourceHighlight: .seconds(2), sleep: timers.sleep
        ),
        calendar: TestClock.calendar,
        clock: { TestClock.now },
        notificationCenter: notificationCenter,
        preferredLanguages: { preferredLanguages },
        service: wrap(mock)
    )
    return (model, mock)
}

/// Waits until `condition` holds, waking only when something it reads changes (Observation),
/// never on a polling timer. `timeout` only ends a test that would otherwise hang.
@MainActor
func until(timeout: Duration = .seconds(10), _ condition: @escaping @MainActor () -> Bool) async -> Bool {
    let clock = ContinuousClock()
    let deadline = clock.now + timeout
    while true {
        let (changes, continuation) = AsyncStream<Void>.makeStream(bufferingPolicy: .bufferingNewest(1))
        let holds = withObservationTracking { condition() } onChange: { continuation.yield() }
        if holds {
            continuation.finish()
            return true
        }
        let remaining = deadline - clock.now
        guard remaining > .zero else {
            continuation.finish()
            return false
        }
        let changed = await withTaskGroup(of: Bool.self) { group in
            group.addTask {
                for await _ in changes { return true }
                return false
            }
            group.addTask {
                try? await Task.sleep(for: remaining)
                return false
            }
            let first = await group.next() ?? false
            group.cancelAll()
            return first
        }
        continuation.finish()
        if !changed { return condition() }
    }
}

/// The model's timers under the test's control (`AppModel.Timing.sleep`): a timer waits until
/// `fire` (or until its task is cancelled, like `Task.sleep`).
actor ManualTimers {
    private struct Waiter {
        let id: Int
        let duration: Duration
        let continuation: CheckedContinuation<Void, Never>
    }

    private var waiters: [Waiter] = []
    private var arrivals: [CheckedContinuation<Void, Never>] = []
    private var nextID = 0

    /// For `AppModel.Timing(sleep:)`.
    nonisolated var sleep: AppModel.Timing.Sleep {
        { [self] duration in await self.wait(duration) }
    }

    private func wait(_ duration: Duration) async {
        nextID += 1
        let id = nextID
        await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                if Task.isCancelled {
                    continuation.resume()
                    return
                }
                waiters.append(Waiter(id: id, duration: duration, continuation: continuation))
                for arrival in arrivals { arrival.resume() }
                arrivals = []
            }
        } onCancel: {
            Task { await self.cancel(id) }
        }
    }

    private func cancel(_ id: Int) {
        guard let index = waiters.firstIndex(where: { $0.id == id }) else { return }
        waiters.remove(at: index).continuation.resume()
    }

    /// The durations of the timers waiting now.
    var pending: [Duration] {
        waiters.map(\.duration)
    }

    /// Waits until a timer of `duration` is waiting.
    func waitForTimer(_ duration: Duration) async {
        while !waiters.contains(where: { $0.duration == duration }) {
            await withCheckedContinuation { arrivals.append($0) }
        }
    }

    /// Fires the waiting timers of `duration` (every waiting timer when nil).
    func fire(_ duration: Duration? = nil) {
        let due = waiters.filter { duration == nil || $0.duration == duration }
        waiters.removeAll { waiter in due.contains { $0.id == waiter.id } }
        for waiter in due { waiter.continuation.resume() }
    }
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
