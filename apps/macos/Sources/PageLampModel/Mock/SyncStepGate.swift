// A turnstile for the mock's sync (tests): instead of waiting `syncStep` between two progress
// events, the mock stops before each step until the test lets it through. The test knows exactly
// how far the sync got, without timing.

/// Where the mock's sync is held: before progress step `step` of `sourceId` (1-based).
public struct SyncStepPosition: Equatable, Sendable {
    public var sourceId: String
    public var step: UInt32

    public init(sourceId: String, step: UInt32) {
        self.sourceId = sourceId
        self.step = step
    }
}

/// Holds the mock's sync before every progress step (`MockService.Timing.gate`). Everything the
/// mock emitted before the held step (the source's start, earlier steps, warnings, earlier
/// sources' results) has reached the sync observer when `held()` returns.
public actor SyncStepGate {
    private var position: SyncStepPosition?
    private var holding: CheckedContinuation<Void, Never>?
    private var watchers: [CheckedContinuation<SyncStepPosition, Never>] = []
    private var isOpen = false

    public init() {}

    /// The mock, before a step: waits here until `advance()` or `open()`.
    func pass(_ step: SyncStepPosition) async {
        guard !isOpen else { return }
        await withCheckedContinuation { continuation in
            position = step
            holding = continuation
            for watcher in watchers { watcher.resume(returning: step) }
            watchers = []
        }
    }

    /// Where the sync is held; waits until it reaches the gate.
    public func held() async -> SyncStepPosition {
        if let position { return position }
        return await withCheckedContinuation { watchers.append($0) }
    }

    /// Lets the held step through (the mock then stops before the next one).
    public func advance() {
        let continuation = holding
        position = nil
        holding = nil
        continuation?.resume()
    }

    /// Advances until the sync is held at `target` (or the sync ends without reaching it, which
    /// would hang: only ask for a step the scenario has).
    public func advance(until target: SyncStepPosition) async {
        while await held() != target {
            advance()
        }
    }

    /// Lets every step through from now on.
    public func open() {
        isOpen = true
        advance()
    }
}
