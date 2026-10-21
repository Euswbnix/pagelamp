// The one-time question (b) reminders (D37 option 2) that runs said and the student hasn't
// answered or dismissed yet: the Tauri app's sharingReminders store. The facade says each
// course's only once, however its run ends, so a reminder outlives the section whose run said
// it (a run stopped by leaving the course says it too). In memory only: never persisted, and it
// holds no course content.

import Foundation
import Observation

@Observable @MainActor
public final class SharingReminders {
    /// A reminder still to answer.
    public struct Pending: Equatable, Sendable {
        /// Where the course's text went (the run's service).
        public var service: String

        public init(service: String) {
            self.service = service
        }
    }

    /// By course id.
    public private(set) var pending: [String: Pending] = [:]

    public init() {}

    public func add(_ courseId: String, service: String) {
        pending[courseId] = Pending(service: service)
    }

    /// Answered or dismissed (anywhere): gone everywhere.
    public func settle(_ courseId: String) {
        pending[courseId] = nil
    }
}
