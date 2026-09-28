// When the sidebar's selection capsule may move (spec §2.3 "Motion order"), without the views.
//
// A page switch costs 60–126 ms of main thread, and its follow-up work (the course's data, the
// toolbar) lands in the frames after it. A slide that shares those frames stutters, so the page
// always goes first and the capsule starts only when the main thread is free:
//
// 1. The first display frame after a page switch means the page has been built and committed
//    (`.pageDrawn`; the navigation commits a queued row then). The slide never starts in that
//    frame.
// 2. A later frame that arrives on time (within 1.5 display frames of the one before) proves the
//    render server had a frame for the page and that nothing held the main thread for a whole
//    frame: the slide starts (`.startSlide`). A late frame resets the wait; after 0.25 s the slide
//    starts anyway (a main thread that never idles).
// 3. A released preview (key repeat, drag) commits its page once the capsule is within 1 pt of
//    the previewed row, or after 0.2 s (`.commitPreview`), so the page build never overlaps the
//    glide.
//
// The capsule's host view feeds it one call per display-link frame (`SidebarCapsuleHostView`,
// whose link runs only while `isAwake`); the tests feed it timestamps.

import Foundation

public struct SidebarMotionGate: Equatable, Sendable {
    /// A frame is on time within 1.5 display frames of the previous one.
    public static let onTimeFactor = 1.5
    /// Start the slide anyway this long after it was first requested.
    public static let maxSlideWait: TimeInterval = 0.25
    /// The capsule has settled within this distance of its row, in points.
    public static let settleTolerance = 1.0
    /// A released preview commits after this long even if the capsule is still moving (the
    /// ideal `motion.quick` spring is within 1 pt after 192–225 ms).
    public static let maxSettleWait: TimeInterval = 0.2

    public enum Action: Equatable, Sendable {
        /// The first frame after a page switch was drawn.
        case pageDrawn
        /// Slide the capsule to its current target now.
        case startSlide
        /// Commit the released preview.
        case commitPreview
    }

    /// When a slide was first requested and hasn't started yet.
    public private(set) var slideArmedAt: TimeInterval?
    /// A page switch went out and its first frame hasn't been drawn yet.
    public private(set) var awaitingPageDrawn = false
    /// A key-repeat preview is held until this time (then it counts as released).
    public private(set) var previewDeadline: TimeInterval?
    /// A preview was released at this time and waits for the capsule to settle.
    public private(set) var previewReleasedAt: TimeInterval?
    private var lastFrame: TimeInterval?

    public init() {}

    /// Whether frames are needed (the host pauses its display link otherwise).
    public var isAwake: Bool {
        slideArmedAt != nil || awaitingPageDrawn || previewDeadline != nil || previewReleasedAt != nil
    }

    /// Requests a slide; the 0.25 s cap counts from the first request until the slide starts.
    public mutating func armSlide(at now: TimeInterval) {
        if slideArmedAt == nil { slideArmedAt = now }
    }

    public mutating func cancelSlide() {
        slideArmedAt = nil
    }

    /// A page switch went out in this update.
    public mutating func pageCommitted() {
        awaitingPageDrawn = true
    }

    /// Holds a key-repeat preview until `deadline` (each repeat moves it on).
    public mutating func holdPreview(until deadline: TimeInterval) {
        previewDeadline = deadline
    }

    /// Key-up, mouse-up: commit once the capsule settles.
    public mutating func releasePreview(at now: TimeInterval) {
        previewDeadline = nil
        previewReleasedAt = now
    }

    /// The preview was committed or dropped some other way.
    public mutating func cancelPreview() {
        previewDeadline = nil
        previewReleasedAt = nil
    }

    /// The view left its window: everything pending happens now (`.pageDrawn`, `.commitPreview`;
    /// the host turns `.startSlide` into a jump).
    public mutating func flush() -> [Action] {
        var actions: [Action] = []
        if awaitingPageDrawn { actions.append(.pageDrawn) }
        if slideArmedAt != nil { actions.append(.startSlide) }
        if previewDeadline != nil || previewReleasedAt != nil { actions.append(.commitPreview) }
        self = SidebarMotionGate()
        return actions
    }

    /// One display frame: `now` is its timestamp, `duration` the display's frame length, and
    /// `capsuleOffset` how far the capsule's on-screen position still is from its row (points).
    public mutating func frame(at now: TimeInterval, duration: TimeInterval, capsuleOffset: Double) -> [Action] {
        var actions: [Action] = []
        // The first frame after a quiet arm counts from the arm.
        let reference = lastFrame ?? slideArmedAt
        let onTime = reference.map { now - $0 <= duration * Self.onTimeFactor } ?? false
        lastFrame = now
        let drew = awaitingPageDrawn
        if drew {
            awaitingPageDrawn = false
            actions.append(.pageDrawn)
        }
        let slideWasArmed = slideArmedAt != nil
        // Never in the same frame as .pageDrawn: its callback may commit a queued page in this turn.
        if !drew, let armed = slideArmedAt, onTime || now - armed >= Self.maxSlideWait {
            slideArmedAt = nil
            actions.append(.startSlide)
        }
        if let deadline = previewDeadline, now >= deadline {
            previewDeadline = nil
            previewReleasedAt = now
        }
        // Not while a slide waits or starts in this frame: the capsule hasn't reached the row.
        if !slideWasArmed, let released = previewReleasedAt,
           capsuleOffset <= Self.settleTolerance || now - released >= Self.maxSettleWait {
            previewReleasedAt = nil
            actions.append(.commitPreview)
        }
        if !isAwake { lastFrame = nil }
        return actions
    }
}
