// The sidebar's list behaviour (spec §2.3 "Keyboard and pointer") without the views. The rules
// are the ones measured on the system sidebar (the SwiftUI sidebar List and a plain source-list
// NSTableView agree on all of them, macOS 27.2):
//
// - ↑/↓ (also with ⇧, ⌃ or ⌘) select the row above/below, skipping section headers, and stop at
//   either end; ⌥↑/⌥↓ select the first/last row. The detail column follows at once.
// - Home/End scroll to the top/bottom without selecting; Page Up/Down pass on (the system list
//   ignores them).
// - Type-select: a letter starts a search after the selected row (wrapping); more letters within
//   the timeout extend the prefix and match again from the selected row; case, diacritics and
//   width don't matter; a space only extends a search; no match changes nothing.
// - A press selects its row at once (mouse-down, like NSTableView).
//
// On top of those, what keeps the sidebar free of jank (spec §2.3 "Motion order"): a page build
// costs 60–126 ms of main thread, so the selection never queues page switches behind it.
//
// - A choice made while a page is still being built (before its first frame is drawn) waits as
//   one queued entry, committed when that frame is drawn: the latest choice wins, at most one
//   page waits, and nothing is left to replay after the keys stop.
// - Holding ↑/↓ (key repeat) and dragging across rows preview: the bold title and the capsule
//   move, no page is built; the preview commits on release (key-up, mouse-up, focus loss, or a
//   repeat that doesn't come) once the capsule has settled.
//
// The selection itself is `AppModel.destination`; this type holds only the transient state. The
// view forwards every event with the current time and carries out the outcome.

import Foundation

public struct SidebarNavigation: Equatable, Sendable {
    /// A row the sidebar shows as selected (bold title, capsule) before the page follows.
    public enum Pending: Equatable, Sendable {
        /// Chosen while a page was still being built: committed at the next `.pageDrawn`.
        case queued(Destination, announce: Bool)
        /// Key repeat or a drag: shown (bold title + capsule), committed on release once the
        /// capsule settles.
        case preview(Destination, announce: Bool)

        public var destination: Destination {
            switch self {
            case .queued(let destination, _), .preview(let destination, _): destination
            }
        }

        /// Whether VoiceOver hears the row when it is committed (keyboard changes only).
        public var announce: Bool {
            switch self {
            case .queued(_, let announce), .preview(_, let announce): announce
            }
        }
    }

    /// A page counts as drawn after this long even without a display frame (an occluded window,
    /// no display link): the navigation can never stay stuck with a page in flight.
    public static let maxPageWait: TimeInterval = 0.5

    /// Type-select resets after this long without a key. AppKit's own value measured
    /// 1.15 s < t ≤ 1.2 s with the default key repeat (delay 0.5 s, interval 0.083 s), which is
    /// 2 × (delay + interval) = 1.167 s.
    public let typeSelectTimeout: TimeInterval
    /// A key-repeat preview commits if no repeat and no key-up follows within this long
    /// (2 × the key-repeat interval).
    public let previewHold: TimeInterval
    public private(set) var pending: Pending?
    /// Keyboard focus is shown (the capsule's focus ring) only after keyboard use: Tab into the
    /// list, or a key while it has focus. A click that focuses the list shows none.
    public private(set) var showsFocusRing = false
    public private(set) var typeSelectBuffer = ""
    private var lastTypedAt: TimeInterval = 0
    /// When the last page switch went out, while its first frame hasn't been drawn yet.
    private var inFlightSince: TimeInterval?
    private var pressed = false
    private var pointerFocusing = false

    /// `SidebarList` passes the student's key-repeat settings (`NSEvent.keyRepeatDelay`,
    /// `keyRepeatInterval`); the defaults are the system's.
    public init(keyRepeatDelay: TimeInterval = 0.5, keyRepeatInterval: TimeInterval = 1.0 / 12) {
        typeSelectTimeout = 2 * (keyRepeatDelay + keyRepeatInterval)
        previewHold = 2 * keyRepeatInterval
    }

    /// What the sidebar shows as selected (bold title, capsule target): a pending row, else the
    /// destination.
    public func highlighted(_ selection: Destination) -> Destination {
        pending?.destination ?? selection
    }

    public mutating func handle(
        _ event: SidebarEvent, in outline: SidebarOutline, selection: Destination, now: TimeInterval
    ) -> SidebarOutcome {
        switch event {
        case .focusEntered:
            showsFocusRing = !pointerFocusing
            pointerFocusing = false
            return .handled
        case .focusLeft:
            showsFocusRing = false
            pointerFocusing = false
            typeSelectBuffer = ""
            if case .preview(let destination, let announce) = pending {
                return select(destination, announce: announce, selection: selection, now: now)
            }
            return .handled

        case .pointerDown(let row):
            showsFocusRing = false
            pointerFocusing = true
            pressed = true
            guard let row, outline.contains(row), row != highlighted(selection) else { return .handled }
            return select(row, announce: false, selection: selection, now: now)
        case .pointerMoved(let row):
            guard pressed, let row, outline.contains(row), row != highlighted(selection) else { return .handled }
            pending = row == selection ? nil : .preview(row, announce: false)
            return .highlight(hold: nil)
        case .pointerUp:
            pressed = false
            if case .preview(let destination, _) = pending, destination != selection { return .releasePreview }
            return .handled
        case .pointerCancelled:
            pressed = false
            if case .preview = pending { pending = nil }
            return .highlight(hold: nil)

        case .step(let delta, let isRepeat):
            showsFocusRing = true
            let rows = outline.rows.map(\.destination)
            guard !rows.isEmpty, delta != 0 else { return .handled }
            let current = highlighted(selection)
            let target: Destination
            if let index = rows.firstIndex(of: current) {
                target = rows[min(max(index + delta.signum(), 0), rows.count - 1)]
            } else {
                target = delta > 0 ? rows[0] : rows[rows.count - 1]
            }
            guard isRepeat else { return select(target, announce: true, selection: selection, now: now) }
            // Key repeat: glide the capsule, build no page.
            if target == current { return .handled }
            if target == selection {
                pending = nil
                return .highlight(hold: nil)
            }
            pending = .preview(target, announce: true)
            return .highlight(hold: previewHold)
        case .first, .last:
            showsFocusRing = true
            let rows = outline.rows.map(\.destination)
            guard let target = event == .first ? rows.first : rows.last else { return .handled }
            return select(target, announce: true, selection: selection, now: now)
        case .keyUp:
            // Arrow keys only; a drag's preview waits for the mouse.
            if case .preview = pending, !pressed { return .releasePreview }
            return .ignored

        case .home:
            showsFocusRing = true
            return .scroll(.top)
        case .end:
            showsFocusRing = true
            return .scroll(.bottom)

        case .typed(let text):
            return typeSelect(text, in: outline, selection: selection, now: now)
        case .activate(let row):
            return select(row, announce: false, selection: selection, now: now)

        case .destinationChanged:
            inFlightSince = now
            // Whatever changed the destination (a menu command, a cross-link, our own commit)
            // is the latest choice.
            guard pending != nil else { return .handled }
            pending = nil
            return .highlight(hold: nil)
        case .pageDrawn:
            inFlightSince = nil
            if case .queued(let destination, let announce) = pending {
                return select(destination, announce: announce, selection: selection, now: now)
            }
            return .handled
        case .previewSettled:
            guard case .preview(let destination, let announce) = pending else { return .handled }
            return select(destination, announce: announce, selection: selection, now: now)
        }
    }

    /// The only way to a commit: the page switches now, unless one is still being built, in which
    /// case `target` waits as the one queued entry (the latest choice wins).
    private mutating func select(
        _ target: Destination, announce: Bool, selection: Destination, now: TimeInterval
    ) -> SidebarOutcome {
        if target == selection {
            guard pending != nil else { return .handled }
            pending = nil
            return .highlight(hold: nil)
        }
        if let since = inFlightSince, now - since < Self.maxPageWait {
            pending = .queued(target, announce: announce)
            return .highlight(hold: nil)
        }
        pending = nil
        return .commit(target, announce: announce)
    }

    private mutating func typeSelect(
        _ text: String, in outline: SidebarOutline, selection: Destination, now: TimeInterval
    ) -> SidebarOutcome {
        guard !text.isEmpty else { return .ignored }
        let continuing = !typeSelectBuffer.isEmpty && now - lastTypedAt <= typeSelectTimeout
        if !continuing {
            // A space starts nothing (Quick Look and buttons may want it); it only extends.
            if text.allSatisfy(\.isWhitespace) { return .ignored }
            typeSelectBuffer = ""
        }
        typeSelectBuffer += text
        lastTypedAt = now
        showsFocusRing = true

        let rows = outline.rows
        guard !rows.isEmpty else { return .handled }
        let current = rows.firstIndex { $0.destination == highlighted(selection) }
        // A new search starts after the highlighted row; a longer prefix keeps it if it still matches.
        let start = current.map { continuing ? $0 : $0 + 1 } ?? 0
        for step in 0..<rows.count {
            let row = rows[(start + step) % rows.count]
            if row.title.range(of: typeSelectBuffer, options: [.anchored, .caseInsensitive, .diacriticInsensitive, .widthInsensitive]) != nil {
                return select(row.destination, announce: true, selection: selection, now: now)
            }
        }
        return .handled
    }
}

/// A key, pointer, focus or drawing event on the sidebar.
public enum SidebarEvent: Equatable, Sendable {
    case focusEntered
    case focusLeft
    /// The pointer went down on a row (nil: a header, a gap).
    case pointerDown(Destination?)
    /// The pointer moved while down; over a row or not (nil).
    case pointerMoved(Destination?)
    case pointerUp
    /// The press ended without a release (the window lost the pointer): a preview reverts.
    case pointerCancelled
    /// ↑ (-1) / ↓ (+1), with or without ⇧, ⌃, ⌘; `isRepeat` for the key's auto-repeat.
    case step(Int, isRepeat: Bool)
    /// ⌥↑ / ⌥↓.
    case first
    case last
    /// ↑/↓ released.
    case keyUp
    case home
    case end
    /// Printable text for type-select (never with ⌘ or ⌃).
    case typed(String)
    /// VoiceOver's press on a row.
    case activate(Destination)
    /// `AppModel.destination` changed, whatever changed it: a page switch went out.
    case destinationChanged(Destination)
    /// The first display frame after that change was drawn.
    case pageDrawn
    /// The capsule reached the previewed row (within 1 pt), or 0.2 s passed.
    case previewSettled
}

/// What the sidebar does after an event.
public enum SidebarOutcome: Equatable, Sendable {
    /// Not the sidebar's key: let it go on (to the window, a default button).
    case ignored
    /// Taken, nothing changes.
    case handled
    /// Set `AppModel.destination` now. `announce`: VoiceOver should hear the new row (keyboard
    /// changes; VoiceOver itself follows pointer and menu changes).
    case commit(Destination, announce: Bool)
    /// `pending` changed without a commit. `hold`: a key-repeat preview's timeout (it commits
    /// if nothing follows within it).
    case highlight(hold: TimeInterval?)
    /// Commit the preview once the capsule settles (`.previewSettled`).
    case releasePreview
    case scroll(SidebarScroll)
}

public enum SidebarScroll: Equatable, Sendable {
    case top, bottom
}
