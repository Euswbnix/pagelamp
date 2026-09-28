// The course section picker's behaviour (spec §3.2.1 "Pointer", "Keyboard", "VoiceOver")
// without the views. The rules are the system tabs control's, measured on macOS 27.2
// (`NSSegmentedControl`, role `.tabs`):
//
// - A press moves the thumb to the pressed segment at once; the selection changes only on
//   mouse-up, to the segment under the pointer (clamped: a release far outside still commits;
//   there is no cancel). Dragging makes the thumb follow the pointer.
// - ← / → (with any modifiers) move a key segment, the one the focus ring and VoiceOver's focus
//   are on, and wrap at both ends; the selection stays. Space (without modifiers) selects the key
//   segment. ↓ is taken and does nothing (the system control would open a segment's menu); every
//   other key goes on.
// - VoiceOver's press selects its segment; setting VoiceOver's focus on a segment makes it the
//   key segment.
//
// On top of those the thumb follows every change of the selection, whatever made it (a menu
// command, a cross-link), except while a press is down: the press wins until it is released.
//
// The selection itself is the page's `CourseUIState.section`; this type holds the transient
// state. The view forwards every event and carries out the outcome.

import CoreGraphics
import Foundation

public struct SegmentedNavigation: Equatable, Sendable {
    public let count: Int
    /// The focus ring's segment (VoiceOver's focus while the control is first responder). Like
    /// the system control's cell state it survives focus changes.
    public private(set) var keySegment: Int
    /// The segment under a press that hasn't been released (nil: no press).
    public private(set) var pressedSegment: Int?

    public init(count: Int, selection: Int) {
        self.count = count
        keySegment = min(max(selection, 0), max(count - 1, 0))
    }

    public mutating func handle(_ event: SegmentedEvent, in layout: SegmentedLayout, selection: Int) -> SegmentedOutcome {
        switch event {
        case .pointerDown(let x):
            let pressed = layout.segment(atX: x)
            pressedSegment = pressed
            return .thumb(.segment(pressed))
        case .pointerDragged(let x):
            guard pressedSegment != nil else { return .handled }
            pressedSegment = layout.segment(atX: x)
            return .thumb(.following(x: x))
        case .pointerUp(let x):
            guard pressedSegment != nil else { return .handled }
            let target = layout.segment(atX: x)
            pressedSegment = nil
            keySegment = target
            return target == selection ? .thumb(.segment(selection)) : .commit(target)
        case .pointerCancelled:
            guard pressedSegment != nil else { return .handled }
            pressedSegment = nil
            return .thumb(.segment(selection))

        case .left, .right:
            guard count > 0 else { return .handled }
            keySegment = (keySegment + (event == .left ? count - 1 : 1)) % count
            return .keySegment(keySegment)
        case .space:
            guard count > 0, keySegment != selection else { return .handled }
            return .commit(keySegment)
        case .down:
            return .handled
        case .otherKey:
            return .ignored

        case .accessibilityPress(let index):
            guard (0..<count).contains(index) else { return .handled }
            guard index != selection else {
                // The selected segment: nothing to commit, but the focus moves to it (measured).
                let moved = keySegment != index
                keySegment = index
                return moved ? .keySegment(index) : .handled
            }
            keySegment = index
            return .commit(index)
        case .accessibilityFocus(let index):
            guard (0..<count).contains(index) else { return .handled }
            keySegment = index
            return .keySegment(index)

        case .selectionChanged(let index):
            guard (0..<count).contains(index) else { return .handled }
            keySegment = index
            return pressedSegment == nil ? .thumb(.segment(index)) : .handled
        }
    }
}

/// A pointer, key, VoiceOver or model event on the picker.
public enum SegmentedEvent: Equatable, Sendable {
    /// x in the control's coordinates.
    case pointerDown(x: CGFloat)
    case pointerDragged(x: CGFloat)
    case pointerUp(x: CGFloat)
    /// The press ended without a release (a lost mouse-up, a new press, the control left its
    /// window): the thumb goes back.
    case pointerCancelled
    /// ← / →, with or without modifiers, key repeat included.
    case left
    case right
    /// Space without ⇧, ⌃, ⌥ or ⌘.
    case space
    /// ↓, with or without modifiers.
    case down
    /// Any other key while the control is first responder.
    case otherKey
    /// VoiceOver's press on a segment.
    case accessibilityPress(Int)
    /// VoiceOver set its focus on a segment.
    case accessibilityFocus(Int)
    /// The selection changed from outside the control (the binding): a commit coming back, the
    /// pop-up menu, the Go menu, a cross-link.
    case selectionChanged(Int)
}

/// What the picker does after an event.
public enum SegmentedOutcome: Equatable, Sendable {
    public enum Thumb: Equatable, Sendable {
        /// Over a segment.
        case segment(Int)
        /// Following the pointer (`SegmentedLayout.thumbFrame(followingX:)`).
        case following(x: CGFloat)
    }

    /// Not the picker's key: let it go on.
    case ignored
    /// Taken, nothing moves.
    case handled
    /// Move the thumb; the selection stays.
    case thumb(Thumb)
    /// Move the thumb to the segment in this same turn, then write the selection.
    case commit(Int)
    /// The key segment moved: the focus ring and VoiceOver's focus follow.
    case keySegment(Int)
}
