// When the sidebar's capsule may move (spec §2.3 "Motion order"): never in the frames that build
// and draw a page, only after an on-time frame proves the main thread is free, and a released
// preview commits its page only once the capsule has settled. Frames at 120 Hz (8.33 ms).

import Foundation
import PageLampModel
import Testing

@Suite("Sidebar capsule motion gate")
struct SidebarMotionGateTests {
    private let frame = 1.0 / 120

    @Test("after a page switch: the first (late) frame draws the page, the next on-time frame starts the slide")
    func pageFirst() {
        var gate = SidebarMotionGate()
        gate.pageCommitted()
        gate.armSlide(at: 10)
        #expect(gate.isAwake)
        // The page took 130 ms: its first frame only reports the page.
        #expect(gate.frame(at: 10.13, duration: frame, capsuleOffset: 0) == [.pageDrawn])
        #expect(gate.frame(at: 10.13 + frame, duration: frame, capsuleOffset: 0) == [.startSlide])
        #expect(!gate.isAwake)
    }

    @Test("a quiet arm (the first frame within 1.5 frames) starts at once")
    func quietArm() {
        var gate = SidebarMotionGate()
        gate.armSlide(at: 10)
        #expect(gate.frame(at: 10.005, duration: frame, capsuleOffset: 0) == [.startSlide])
    }

    @Test("late frames reset the wait; 0.25 s after the first request the slide starts anyway")
    func lateFramesAndCap() {
        var gate = SidebarMotionGate()
        gate.pageCommitted()
        gate.armSlide(at: 10)
        #expect(gate.frame(at: 10.1, duration: frame, capsuleOffset: 0) == [.pageDrawn])
        #expect(gate.frame(at: 10.15, duration: frame, capsuleOffset: 0) == [])      // 50 ms late
        #expect(gate.frame(at: 10.2, duration: frame, capsuleOffset: 0) == [])
        #expect(gate.frame(at: 10.25, duration: frame, capsuleOffset: 0) == [.startSlide])
    }

    @Test("pageDrawn comes once per page switch and never together with startSlide")
    func pageDrawnOnce() {
        var gate = SidebarMotionGate()
        gate.armSlide(at: 10)
        gate.pageCommitted()
        // On time, but it is the page's frame.
        #expect(gate.frame(at: 10.004, duration: frame, capsuleOffset: 0) == [.pageDrawn])
        // A queued row committed in that frame: another page switch, the slide keeps waiting.
        gate.pageCommitted()
        #expect(gate.frame(at: 10.09, duration: frame, capsuleOffset: 0) == [.pageDrawn])
        #expect(gate.frame(at: 10.09 + frame, duration: frame, capsuleOffset: 0) == [.startSlide])
        #expect(gate.frame(at: 10.09 + 2 * frame, duration: frame, capsuleOffset: 0) == [])
    }

    @Test("a released preview commits when the capsule is within 1 pt, or after 0.2 s")
    func settle() {
        var gate = SidebarMotionGate()
        gate.releasePreview(at: 10)
        #expect(gate.frame(at: 10.01, duration: frame, capsuleOffset: 12) == [])
        #expect(gate.frame(at: 10.02, duration: frame, capsuleOffset: 1.5) == [])
        #expect(gate.frame(at: 10.03, duration: frame, capsuleOffset: 0.8) == [.commitPreview])
        #expect(!gate.isAwake)
        var slow = SidebarMotionGate()
        slow.releasePreview(at: 20)
        #expect(slow.frame(at: 20.19, duration: frame, capsuleOffset: 6) == [])
        #expect(slow.frame(at: 20.21, duration: frame, capsuleOffset: 6) == [.commitPreview])
    }

    @Test("never commits a preview while its slide waits or starts in the same frame")
    func settleAfterSlide() {
        var gate = SidebarMotionGate()
        gate.armSlide(at: 10)
        gate.releasePreview(at: 10)
        // The slide starts in this frame: the capsule hasn't moved yet, whatever the offset says.
        #expect(gate.frame(at: 10.005, duration: frame, capsuleOffset: 0) == [.startSlide])
        #expect(gate.frame(at: 10.005 + frame, duration: frame, capsuleOffset: 20) == [])
        #expect(gate.frame(at: 10.005 + 2 * frame, duration: frame, capsuleOffset: 0.5) == [.commitPreview])
        // Waiting behind a page: no commit until the slide has started.
        var behindPage = SidebarMotionGate()
        behindPage.pageCommitted()
        behindPage.armSlide(at: 30)
        behindPage.releasePreview(at: 30)
        #expect(behindPage.frame(at: 30.1, duration: frame, capsuleOffset: 0) == [.pageDrawn])
        #expect(behindPage.frame(at: 30.3, duration: frame, capsuleOffset: 0) == [.startSlide])
        #expect(behindPage.frame(at: 30.3 + frame, duration: frame, capsuleOffset: 0) == [.commitPreview])
    }

    @Test("a held preview counts as released at its deadline; each repeat moves the deadline")
    func holdDeadline() {
        var gate = SidebarMotionGate()
        gate.holdPreview(until: 10.1)
        #expect(gate.frame(at: 10.05, duration: frame, capsuleOffset: 0) == [])
        gate.holdPreview(until: 10.2)
        #expect(gate.frame(at: 10.15, duration: frame, capsuleOffset: 0) == [])
        #expect(gate.previewDeadline == 10.2)
        #expect(gate.frame(at: 10.2, duration: frame, capsuleOffset: 0) == [.commitPreview])
        #expect(gate.previewDeadline == nil)
        #expect(gate.previewReleasedAt == nil)
        var cancelled = SidebarMotionGate()
        cancelled.holdPreview(until: 5)
        cancelled.cancelPreview()
        #expect(!cancelled.isAwake)
    }

    @Test("arming again keeps the first request's time (the cap counts from it)")
    func rearm() {
        var gate = SidebarMotionGate()
        gate.armSlide(at: 10)
        gate.armSlide(at: 10.2)
        #expect(gate.slideArmedAt == 10)
        gate.cancelSlide()
        gate.armSlide(at: 11)
        #expect(gate.slideArmedAt == 11)
    }

    @Test("asleep once everything is done; the first frame after waking counts from the arm")
    func sleepAndWake() {
        var gate = SidebarMotionGate()
        #expect(!gate.isAwake)
        gate.armSlide(at: 10)
        #expect(gate.frame(at: 10.001, duration: frame, capsuleOffset: 0) == [.startSlide])
        #expect(!gate.isAwake)
        // Much later: the old frame is forgotten, the new arm is the reference.
        gate.armSlide(at: 50)
        #expect(gate.frame(at: 50.008, duration: frame, capsuleOffset: 0) == [.startSlide])
    }

    @Test("flush returns everything pending and leaves the gate asleep")
    func flush() {
        var gate = SidebarMotionGate()
        gate.pageCommitted()
        gate.armSlide(at: 10)
        gate.holdPreview(until: 11)
        #expect(gate.flush() == [.pageDrawn, .startSlide, .commitPreview])
        #expect(!gate.isAwake)
        #expect(gate.flush() == [])
        var released = SidebarMotionGate()
        released.releasePreview(at: 3)
        #expect(released.flush() == [.commitPreview])
    }
}
