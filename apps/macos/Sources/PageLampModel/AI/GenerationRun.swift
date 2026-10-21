// One model run (a study plan, an explanation, a weekly note; design §7): its GenEvents as
// state, Stop, and how it ended. Only the backend, the model, the stage and (explanations) the
// materials read are kept from the events: never text as it arrives. The run's own screen
// announces stages and the end; this only records them.

import Foundation
import Observation
import PageLampKit

@Observable @MainActor
public final class GenerationRun<Output: Sendable> {
    /// What a running run has said so far.
    public struct Progress: Equatable, Sendable {
        public var backend: String?
        public var model: String?
        /// Whether the model runs on this computer (nil until the run has said): a cloud run of a
        /// course whose materials may not be shared is stopped.
        public var onDevice: Bool?
        public var stage: GenStage?
        /// Explanations: how many materials the model reads (the `context` event).
        public var materialsIncluded: UInt32?
        /// Stop was pressed; the run ends at its next step.
        public var stopping = false

        public init() {}
    }

    public enum Phase {
        case idle
        case running(Progress)
        case finished(Output)
        /// Stopped on request (nothing was saved).
        case stopped
        case failed(PageLampFailure)
    }

    /// A run of the facade: its call with the run's id and the observer for its events.
    public typealias Call = @Sendable (
        _ service: any PageLampService, _ generationId: String, _ observer: GenEventStream
    ) async throws(PageLampFailure) -> Output

    public private(set) var phase: Phase = .idle
    /// The run in flight, if any.
    public private(set) var generationId: String?
    /// What the last run had said when it ended (nil until one ended): e.g. whether it started.
    public private(set) var lastProgress: Progress?

    @ObservationIgnored private let service: any PageLampService
    @ObservationIgnored private let newId: @Sendable () -> String

    /// - Parameter newId: each run's id (tests pass their own).
    public init(service: any PageLampService, newId: @escaping @Sendable () -> String = randomGenerationId) {
        self.service = service
        self.newId = newId
    }

    public var isRunning: Bool { generationId != nil }

    /// Runs `call` unless a run is in flight; returns once it has ended.
    /// - Parameters:
    ///   - onFailure: where a failure (not a Stop) leaves the run, instead of `.failed`
    ///     (Monday's weekly note ends quietly); nil: `.failed`.
    ///   - onNotice: the run's notices as they come, with what it has said so far: one may outlast
    ///     the run's end (the question (b) reminder comes however the run ends).
    public func run(
        _ call: Call,
        onFailure: ((PageLampFailure) -> Phase)? = nil,
        onNotice: (@MainActor (GenNoticeCode, Progress) -> Void)? = nil
    ) async {
        guard generationId == nil else { return }
        let id = newId()
        generationId = id
        phase = .running(Progress())
        let progress = GenEventStream()
        let events = Task { @MainActor [weak self] in
            for await event in progress.events {
                self?.apply(event, to: id, onNotice: onNotice)
            }
        }
        let result: Result<Output, PageLampFailure>
        let service = self.service
        do throws(PageLampFailure) {
            result = .success(try await call(service, id, progress))
        } catch {
            result = .failure(error)
        }
        progress.finish()
        await events.value
        if case .running(let said) = phase { lastProgress = said }
        generationId = nil
        switch result {
        case .success(let output): phase = .finished(output)
        case .failure(let failure) where failure.kind == .cancelled: phase = .stopped
        case .failure(let failure): phase = onFailure?(failure) ?? .failed(failure)
        }
    }

    /// Stop: one cancel per press; if the cancel fails, Stop can be pressed again.
    public func stop() async {
        guard let id = generationId, case .running(var progress) = phase, !progress.stopping else { return }
        progress.stopping = true
        phase = .running(progress)
        do throws(PageLampFailure) {
            try await service.cancelGeneration(generationId: id)
        } catch {
            guard generationId == id, case .running(var current) = phase else { return }
            current.stopping = false
            phase = .running(current)
        }
    }

    /// The run's screen closed: a run in flight is cancelled (nothing keeps writing, or costing,
    /// out of sight). A failed cancel is ignored: the screen is gone.
    public func cancelInFlight() {
        guard let id = generationId else { return }
        let service = self.service
        Task { try? await service.cancelGeneration(generationId: id) }
    }

    /// Back to the start (a result dismissed); never while running.
    public func reset() {
        guard generationId == nil else { return }
        phase = .idle
    }

    private func apply(_ event: GenEvent, to id: String, onNotice: (@MainActor (GenNoticeCode, Progress) -> Void)?) {
        guard generationId == id, case .running(var progress) = phase else { return }
        switch event {
        case .started(_, let backendLabel, let model, let onDevice):
            progress.backend = backendLabel
            progress.model = model
            progress.onDevice = onDevice
        case .stage(let stage):
            progress.stage = stage
        case .context(let summary, _):
            progress.materialsIncluded = summary.materialsIncluded
        case .notice(let code):
            onNotice?(code, progress)
            return
        case .textDelta, .usage, .finished:
            // Not shown: never text as it arrives.
            return
        }
        phase = .running(progress)
    }
}

/// A new run's id: a random UUID, lowercased like the Tauri app's.
public func randomGenerationId() -> String {
    UUID().uuidString.lowercased()
}
