// The one error type the UI handles (spec §7.3: Swift maps codes, never English).

import Foundation
import PageLampKit

/// A failed PageLamp call: what kind of failure, plus the backend's English message.
///
/// Show `localizedDescription(in:)` (or `errorDescription`) to the student. `message` is the
/// backend's own English text: show it verbatim only where the Tauri app does (tagged English),
/// e.g. a source's last error; never as the headline.
public struct PageLampFailure: Error, Equatable, Hashable, Sendable {
    public enum Kind: String, CaseIterable, Sendable {
        /// Token or feed address rejected (expired, revoked, wrong).
        case auth
        /// Could not reach the server.
        case network
        /// Bad input.
        case invalid
        /// The course, source or material does not exist.
        case notFound = "not_found"
        /// A course reference matched several courses.
        case ambiguous
        /// Another window or command is syncing.
        case busy
        /// The database was written by a newer PageLamp, or is older and not migrated.
        case schema
        /// PageLamp didn't send it to a model (the course's AI settings, no model chosen, the
        /// budget, …); the facade's `BlockReason` says which.
        case blocked
        /// The model or its service failed (key rejected, quota, timeout, …).
        case model
        /// Stopped on request (e.g. a sync the student cancelled).
        case cancelled
        /// Anything else (database, keychain, I/O).
        case `internal`
        /// The core hit a bug (a Rust panic).
        case panic
    }

    public let kind: Kind
    public let message: String

    public init(kind: Kind, message: String) {
        self.kind = kind
        self.message = message
    }

    /// Maps the facade's error (every case, exhaustively).
    public init(_ error: PageLampError) {
        switch error {
        case .Auth(let message): self.init(kind: .auth, message: message)
        case .Network(let message): self.init(kind: .network, message: message)
        case .Invalid(let message): self.init(kind: .invalid, message: message)
        case .NotFound(let message): self.init(kind: .notFound, message: message)
        case .Ambiguous(let message): self.init(kind: .ambiguous, message: message)
        case .Busy(let message): self.init(kind: .busy, message: message)
        case .Schema(let message): self.init(kind: .schema, message: message)
        case .Blocked(let message, _): self.init(kind: .blocked, message: message)
        case .Model(let message, _, _): self.init(kind: .model, message: message)
        case .Cancelled(let message): self.init(kind: .cancelled, message: message)
        case .Internal(let message): self.init(kind: .internal, message: message)
        case .Panic(let message): self.init(kind: .panic, message: message)
        }
    }

    /// Any error from a PageLamp call, as a `PageLampFailure` (unknown errors are `.internal`).
    public static func from(_ error: any Error) -> PageLampFailure {
        switch error {
        case let failure as PageLampFailure: failure
        case let error as PageLampError: PageLampFailure(error)
        default: PageLampFailure(kind: .internal, message: String(describing: error))
        }
    }

    /// The string key that explains this failure to the student.
    public var descriptionKey: String {
        switch kind {
        case .auth: "common.errors.auth"
        case .network: "common.errors.network"
        case .invalid: "common.errors.invalid"
        case .notFound: "common.errors.not_found"
        case .ambiguous: "common.errors.ambiguous"
        case .busy: "common.errors.busy"
        case .schema: "mac.errors.schema"
        case .blocked: "common.errors.blocked"
        case .model: "common.errors.model"
        case .cancelled: "common.errors.cancelled"
        case .internal: "common.errors.internal"
        case .panic: "mac.errors.panic"
        }
    }

    /// The explanation in the given language.
    public func localizedDescription(in l10n: L10n) -> String {
        l10n(descriptionKey)
    }
}

extension PageLampFailure: LocalizedError {
    /// In the app's current language (`L10n.current`); the backend message before the app has
    /// set a language (only in tests and tools).
    public var errorDescription: String? {
        L10n.current.map(localizedDescription(in:)) ?? message
    }
}
