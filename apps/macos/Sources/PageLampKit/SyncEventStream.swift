// Bridges sync progress from PageLamp's worker threads to any actor.

/// A `SyncObserver` that forwards every `SyncEvent` into an `AsyncStream`.
///
/// Rust calls `onEvent` on its own worker threads (`pagelamp-rt`) and waits while it runs, so
/// the observer only buffers the event; iterate `events` wherever the UI state lives, e.g. on
/// the `@MainActor` model:
///
/// ```swift
/// let progress = SyncEventStream()
/// Task { @MainActor in for await event in progress.events { apply(event) } }
/// defer { progress.finish() }
/// let summary = try await pageLamp.syncAll(request: SyncRequest(), observer: progress)
/// ```
public final class SyncEventStream: SyncObserver {
    /// Every event of the run, in order; ends after `finish()`.
    public let events: AsyncStream<SyncEvent>
    private let continuation: AsyncStream<SyncEvent>.Continuation

    public init() {
        (events, continuation) = AsyncStream.makeStream(
            of: SyncEvent.self,
            bufferingPolicy: .unbounded
        )
    }

    public func onEvent(event: SyncEvent) {
        continuation.yield(event)
    }

    /// Ends `events`; call once the sync call has returned or thrown.
    public func finish() {
        continuation.finish()
    }
}
