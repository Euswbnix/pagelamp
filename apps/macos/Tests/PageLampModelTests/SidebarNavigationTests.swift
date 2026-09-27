// The sidebar without its views (spec §2.3): geometry that matches the system source list
// (measured on macOS 27.2; small rows are a uniform 24 pt on purpose, see SidebarMetrics); the
// keyboard, type-select, pointer and focus-ring rules measured on the system sidebar; and what
// keeps page switches from queueing up (a queued row while a page is being built, previews while
// a key is held or the pointer drags). Nothing here touches the real data folder, the keychain or
// preferences.

import CoreGraphics
import Foundation
import PageLampKit
import PageLampModel
import Testing

private let outline = SidebarOutline(
    courses: [(id: "101", title: "DEMO101"), (id: "205", title: "DEMO205"), (id: "310", title: "DEMO310")],
    thisWeek: "This Week", sources: "Sources & Sync", connect: "Connect AI App"
)
private let demo101 = Destination.course("101")
private let demo205 = Destination.course("205")
private let demo310 = Destination.course("310")

@Suite("Sidebar geometry")
struct SidebarLayoutTests {
    @Test("medium rows and headers sit where the system sidebar puts them (27.2: 52, 97, 116 … 276)")
    func mediumMatchesSystem() {
        let layout = SidebarLayout(outline: outline, metrics: SidebarMetrics(.medium))
        #expect(layout.frames.map(\.top) == [0, 45, 64, 96, 128, 173, 192, 224])
        // The system list starts below the 52 pt toolbar inset; ours starts at 0 inside it.
        #expect(layout.frames.map { $0.top + 52 } == [52, 97, 116, 148, 180, 225, 244, 276])
        #expect(layout.frames.map(\.height) == [32, 19, 32, 32, 32, 19, 32, 32])
        #expect(layout.gap(before: 1) == 13)
        #expect(layout.gap(before: 2) == 0)
    }

    @Test("rows are the source-list table's rowHeight 24 / 32 / 40 (uniform at small on purpose); headers stay 19 pt with a 13 pt gap", arguments: [
        (SidebarMetrics.Size.small, CGFloat(24), CGFloat(11)),
        (.medium, 32, 13),
        (.large, 40, 15),
    ])
    func sizes(size: SidebarMetrics.Size, row: CGFloat, title: CGFloat) {
        let layout = SidebarLayout(outline: outline, metrics: SidebarMetrics(size))
        #expect(layout.metrics.rowHeight == row)
        #expect(layout.metrics.titleSize == title)
        #expect(layout.metrics.iconSize == title)
        #expect(layout.frames[1] == SidebarLayout.Frame(top: row + 13, height: 19))
        #expect(layout.frames[2].top == row + 13 + 19)
    }

    @Test("the title starts at x 42 / 46 / 48")
    func titleX() {
        #expect(SidebarMetrics(.small).titleX == 42)
        #expect(SidebarMetrics(.medium).titleX == 46)
        #expect(SidebarMetrics(.large).titleX == 48)
    }

    @Test("without courses the Setup header follows This Week")
    func noCourses() {
        let empty = SidebarOutline(courses: [], thisWeek: "This Week", sources: "Sources & Sync", connect: "Connect AI App")
        #expect(empty.items.map(\.id) == [.row(.thisWeek), .header(.setup), .row(.sources), .row(.connect)])
        let layout = SidebarLayout(outline: empty, metrics: SidebarMetrics(.medium))
        #expect(layout.frames.map(\.top) == [0, 45, 64, 96])
        #expect(layout.contentHeight == 128)
    }

    @Test("the content height is every row and header plus the header gaps")
    func contentHeight() {
        #expect(SidebarLayout(outline: outline, metrics: SidebarMetrics(.medium)).contentHeight == 256)
        #expect(SidebarLayout(outline: outline, metrics: SidebarMetrics(.small)).contentHeight == CGFloat(208))   // 6 × 24 + 2 × 19 + 2 × 13
        #expect(SidebarLayout(outline: outline, metrics: SidebarMetrics(.large)).contentHeight == CGFloat(304))   // 6 × 40 + 2 × 19 + 2 × 13
    }

    @Test("the capsule spans the row: x 10, width W − 20, full height; nothing for an unlisted course", arguments: [
        (CGFloat(200), CGFloat(180)), (232, 212), (300, 280),
    ])
    func capsule(width: CGFloat, capsuleWidth: CGFloat) {
        let layout = SidebarLayout(outline: outline, metrics: SidebarMetrics(.medium))
        #expect(layout.capsuleFrame(for: demo205, width: width) == CGRect(x: 10, y: 96, width: capsuleWidth, height: 32))
        #expect(layout.capsuleFrame(for: .course("hidden"), width: width) == nil)
    }

    @Test("the pointer hits rows only: headers and gaps select nothing")
    func hitTesting() {
        let layout = SidebarLayout(outline: outline, metrics: SidebarMetrics(.medium))
        #expect(layout.destination(atY: 0) == .thisWeek)
        #expect(layout.destination(atY: 31.9) == .thisWeek)
        #expect(layout.destination(atY: 32) == nil)       // the gap above "Courses"
        #expect(layout.destination(atY: 50) == nil)       // the header
        #expect(layout.destination(atY: 64) == demo101)
        #expect(layout.destination(atY: 255) == .connect)
        #expect(layout.destination(atY: 256) == nil)
        #expect(layout.destination(atY: -1) == nil)
    }
}

@Suite("Sidebar outline") @MainActor
struct SidebarOutlineTests {
    @Test("the outline lists visible courses by code, in the student's language")
    func outlineFromModel() async {
        let (model, _) = makeModel(scenario: .demo)
        await model.refresh()
        let english = model.sidebarOutline(l10n: model.l10n)
        let titles = english.rows.map(\.title)
        #expect(titles.first == "This Week")
        #expect(titles.suffix(2) == ["Sources & Sync", "Connect AI App"])
        #expect(english.rows.count == model.visibleCourses.count + 3)
        #expect(Array(titles.dropFirst().prefix(model.visibleCourses.count)) == model.visibleCourses.map { $0.course.code ?? $0.course.name })
        #expect(english.items.contains { $0.id == .header(.courses) })
        model.language = .simplifiedChinese
        let chinese = model.sidebarOutline(l10n: model.l10n)
        #expect(chinese.rows.first?.title == "本周")
        #expect(chinese.rows.last?.title == "连接 AI 应用")
    }
}

@Suite("Sidebar keyboard, type-select, pointer and focus")
struct SidebarNavigationTests {
    /// Long after any page switch: nothing is in flight.
    private let idle: TimeInterval = 100

    @Test("↑/↓ skip headers and stop at the ends; from an unlisted row ↓ takes the first row, ↑ the last")
    func arrows() {
        var nav = SidebarNavigation()
        #expect(nav.handle(.step(1, isRepeat: false), in: outline, selection: .thisWeek, now: idle) == .commit(demo101, announce: true))
        #expect(nav.handle(.step(1, isRepeat: false), in: outline, selection: demo310, now: idle) == .commit(.sources, announce: true))
        #expect(nav.handle(.step(-1, isRepeat: false), in: outline, selection: .sources, now: idle) == .commit(demo310, announce: true))
        #expect(nav.handle(.step(1, isRepeat: false), in: outline, selection: .connect, now: idle) == .handled)
        #expect(nav.handle(.step(-1, isRepeat: false), in: outline, selection: .thisWeek, now: idle) == .handled)
        #expect(nav.handle(.step(1, isRepeat: false), in: outline, selection: .course("hidden"), now: idle) == .commit(.thisWeek, announce: true))
        #expect(nav.handle(.step(-1, isRepeat: false), in: outline, selection: .course("hidden"), now: idle) == .commit(.connect, announce: true))
    }

    @Test("⌥↑/⌥↓ go to the first/last row")
    func firstLast() {
        var nav = SidebarNavigation()
        #expect(nav.handle(.last, in: outline, selection: .thisWeek, now: idle) == .commit(.connect, announce: true))
        #expect(nav.handle(.first, in: outline, selection: .connect, now: idle) == .commit(.thisWeek, announce: true))
        #expect(nav.handle(.first, in: outline, selection: .thisWeek, now: idle) == .handled)
    }

    @Test("Home/End scroll and never select")
    func scrollKeys() {
        var nav = SidebarNavigation()
        #expect(nav.handle(.home, in: outline, selection: demo205, now: idle) == .scroll(.top))
        #expect(nav.handle(.end, in: outline, selection: demo205, now: idle) == .scroll(.bottom))
        #expect(nav.pending == nil)
        #expect(nav.showsFocusRing)
    }

    @Test("type-select: a new letter searches after the selection and wraps; more letters extend the prefix")
    func typeSelect() {
        var nav = SidebarNavigation()
        #expect(nav.handle(.typed("d"), in: outline, selection: .thisWeek, now: 10) == .commit(demo101, announce: true))
        // After a pause, "d" again goes to the next match: repeating a letter cycles.
        #expect(nav.handle(.typed("d"), in: outline, selection: demo101, now: 12) == .commit(demo205, announce: true))
        #expect(nav.handle(.typed("d"), in: outline, selection: demo310, now: 14) == .commit(demo101, announce: true))
        // "demo2" typed quickly: the selection stays while it still matches, then moves.
        var fast = SidebarNavigation()
        var selection: Destination = .thisWeek
        for (index, letter) in ["d", "e", "m", "o", "2"].enumerated() {
            if case .commit(let next, _) = fast.handle(.typed(letter), in: outline, selection: selection, now: 20 + Double(index) * 0.1) {
                selection = next
            }
        }
        #expect(selection == demo205)
        #expect(fast.typeSelectBuffer == "demo2")
        // "dd" quickly matches nothing: no change (a fast repeat does not cycle).
        var double = SidebarNavigation()
        #expect(double.handle(.typed("d"), in: outline, selection: .thisWeek, now: 30) == .commit(demo101, announce: true))
        #expect(double.handle(.typed("d"), in: outline, selection: demo101, now: 30.05) == .handled)
    }

    @Test("type-select skips headers, ignores case, diacritics and width; a space only extends a search")
    func typeSelectDetails() {
        var nav = SidebarNavigation()
        // "Courses" is a header: "c" is Connect AI App.
        #expect(nav.handle(.typed("C"), in: outline, selection: .thisWeek, now: 1) == .commit(.connect, announce: true))
        #expect(nav.handle(.typed("ö"), in: outline, selection: .connect, now: 1.1) == .handled)   // "co…" still Connect
        #expect(nav.handle(.typed(" "), in: outline, selection: .connect, now: 5) == .ignored)     // no search to extend
        var phrase = SidebarNavigation()
        var selection: Destination = .connect
        for (index, letter) in ["t", "h", "i", "s", " ", "w"].enumerated() {
            if case .commit(let next, _) = phrase.handle(.typed(letter), in: outline, selection: selection, now: 10 + Double(index) * 0.1) {
                selection = next
            }
        }
        #expect(selection == .thisWeek)
        #expect(phrase.handle(.typed("7"), in: outline, selection: .thisWeek, now: 20) == .handled)  // no match
        // Full-width letters match (width-insensitive).
        var wide = SidebarNavigation()
        #expect(wide.handle(.typed("Ｓ"), in: outline, selection: .thisWeek, now: 1) == .commit(.sources, announce: true))
    }

    @Test("the buffer resets after 2 × (key repeat delay + interval): 1.15 s continues, 1.2 s resets")
    func typeSelectTimeout() {
        #expect(abs(SidebarNavigation().typeSelectTimeout - 1.1667) < 0.001)
        #expect(abs(SidebarNavigation(keyRepeatDelay: 0.25, keyRepeatInterval: 0.05).typeSelectTimeout - 0.6) < 0.001)
        var within = SidebarNavigation()
        _ = within.handle(.typed("c"), in: outline, selection: .thisWeek, now: 0)
        #expect(within.handle(.typed("d"), in: outline, selection: .connect, now: 1.15) == .handled)   // "cd": nothing
        var after = SidebarNavigation()
        _ = after.handle(.typed("c"), in: outline, selection: .thisWeek, now: 0)
        #expect(after.handle(.typed("d"), in: outline, selection: .connect, now: 1.2) == .commit(demo101, announce: true))
    }

    @Test("a press commits at once; the highlighted row, a header or a gap do nothing")
    func pointerPress() {
        var nav = SidebarNavigation()
        #expect(nav.handle(.pointerDown(demo205), in: outline, selection: .thisWeek, now: idle) == .commit(demo205, announce: false))
        _ = nav.handle(.pointerUp, in: outline, selection: demo205, now: idle)
        #expect(nav.handle(.pointerDown(demo205), in: outline, selection: demo205, now: idle) == .handled)
        _ = nav.handle(.pointerUp, in: outline, selection: demo205, now: idle)
        #expect(nav.handle(.pointerDown(nil), in: outline, selection: demo205, now: idle) == .handled)
        _ = nav.handle(.pointerUp, in: outline, selection: demo205, now: idle)
        #expect(nav.handle(.pointerDown(.course("hidden")), in: outline, selection: demo205, now: idle) == .handled)
    }

    @Test("a drag previews the row under the pointer; back onto the selection clears it; release commits; cancel reverts")
    func pointerDrag() {
        var nav = SidebarNavigation()
        _ = nav.handle(.pointerDown(demo205), in: outline, selection: demo101, now: idle)
        #expect(nav.handle(.pointerMoved(demo310), in: outline, selection: demo205, now: idle) == .highlight(hold: nil))
        #expect(nav.pending == .preview(demo310, announce: false))
        #expect(nav.highlighted(demo205) == demo310)
        #expect(nav.handle(.pointerMoved(nil), in: outline, selection: demo205, now: idle) == .handled)   // over a header: keep
        #expect(nav.pending == .preview(demo310, announce: false))
        #expect(nav.handle(.pointerUp, in: outline, selection: demo205, now: idle) == .releasePreview)
        #expect(nav.handle(.previewSettled, in: outline, selection: demo205, now: idle) == .commit(demo310, announce: false))
        #expect(nav.pending == nil)
        // Back onto the selection: nothing to commit.
        _ = nav.handle(.pointerDown(demo101), in: outline, selection: demo310, now: idle)
        _ = nav.handle(.pointerMoved(.sources), in: outline, selection: demo101, now: idle)
        #expect(nav.handle(.pointerMoved(demo101), in: outline, selection: demo101, now: idle) == .highlight(hold: nil))
        #expect(nav.pending == nil)
        #expect(nav.handle(.pointerUp, in: outline, selection: demo101, now: idle) == .handled)
        // A cancelled drag drops its preview.
        _ = nav.handle(.pointerDown(demo101), in: outline, selection: demo101, now: idle)
        _ = nav.handle(.pointerMoved(.connect), in: outline, selection: demo101, now: idle)
        #expect(nav.handle(.pointerCancelled, in: outline, selection: demo101, now: idle) == .highlight(hold: nil))
        #expect(nav.pending == nil)
        // Moves without a press (hover) never select.
        #expect(nav.handle(.pointerMoved(.connect), in: outline, selection: demo101, now: idle) == .handled)
        #expect(nav.pending == nil)
    }

    @Test("the focus ring shows after Tab or a key, never after a click; focus loss hides it and clears type-select")
    func focusRing() {
        var nav = SidebarNavigation()
        _ = nav.handle(.focusEntered, in: outline, selection: .thisWeek, now: idle)          // Tab
        #expect(nav.showsFocusRing)
        _ = nav.handle(.pointerDown(demo101), in: outline, selection: .thisWeek, now: idle)  // a click hides it
        #expect(!nav.showsFocusRing)
        _ = nav.handle(.pointerUp, in: outline, selection: demo101, now: idle)
        _ = nav.handle(.focusLeft, in: outline, selection: demo101, now: idle)
        _ = nav.handle(.pointerDown(demo205), in: outline, selection: demo101, now: idle)    // click into the list
        _ = nav.handle(.focusEntered, in: outline, selection: demo205, now: idle)
        #expect(!nav.showsFocusRing)
        _ = nav.handle(.pointerUp, in: outline, selection: demo205, now: idle)
        _ = nav.handle(.typed("d"), in: outline, selection: demo205, now: idle)               // then a key
        #expect(nav.showsFocusRing)
        #expect(nav.typeSelectBuffer == "d")
        _ = nav.handle(.focusLeft, in: outline, selection: demo310, now: idle)
        #expect(!nav.showsFocusRing)
        #expect(nav.typeSelectBuffer == "")
    }

    @Test("while a page is being built, keys queue one row (moving on from it); the drawn frame commits it")
    func coalescing() {
        var nav = SidebarNavigation()
        #expect(nav.handle(.step(1, isRepeat: false), in: outline, selection: .thisWeek, now: 10) == .commit(demo101, announce: true))
        _ = nav.handle(.destinationChanged(demo101), in: outline, selection: demo101, now: 10)
        // Before the page's first frame: queued, no commit.
        #expect(nav.handle(.step(1, isRepeat: false), in: outline, selection: demo101, now: 10.04) == .highlight(hold: nil))
        #expect(nav.pending == .queued(demo205, announce: true))
        // A second key moves on from the queued row; still one entry.
        #expect(nav.handle(.step(1, isRepeat: false), in: outline, selection: demo101, now: 10.08) == .highlight(hold: nil))
        #expect(nav.pending == .queued(demo310, announce: true))
        #expect(nav.highlighted(demo101) == demo310)
        #expect(nav.handle(.pageDrawn, in: outline, selection: demo101, now: 10.1) == .commit(demo310, announce: true))
        #expect(nav.pending == nil)
        _ = nav.handle(.destinationChanged(demo310), in: outline, selection: demo310, now: 10.1)
        // The next drawn frame with nothing queued does nothing.
        #expect(nav.handle(.pageDrawn, in: outline, selection: demo310, now: 10.2) == .handled)
        // A page whose frame never came counts as drawn after maxPageWait: a key commits directly.
        _ = nav.handle(.destinationChanged(demo310), in: outline, selection: demo310, now: 20)
        #expect(nav.handle(.step(1, isRepeat: false), in: outline, selection: demo310, now: 20 + SidebarNavigation.maxPageWait) == .commit(.sources, announce: true))
    }

    @Test("a queued row that is the selection again is dropped; a press while a page builds queues too")
    func coalescingEdges() {
        var nav = SidebarNavigation()
        _ = nav.handle(.destinationChanged(demo101), in: outline, selection: demo101, now: 5)
        _ = nav.handle(.step(1, isRepeat: false), in: outline, selection: demo101, now: 5.01)
        #expect(nav.pending == .queued(demo205, announce: true))
        #expect(nav.handle(.step(-1, isRepeat: false), in: outline, selection: demo101, now: 5.02) == .highlight(hold: nil))
        #expect(nav.pending == nil)
        #expect(nav.handle(.pointerDown(.sources), in: outline, selection: demo101, now: 5.03) == .highlight(hold: nil))
        #expect(nav.pending == .queued(.sources, announce: false))
        _ = nav.handle(.pointerUp, in: outline, selection: demo101, now: 5.04)
        #expect(nav.handle(.pageDrawn, in: outline, selection: demo101, now: 5.05) == .commit(.sources, announce: false))
    }

    @Test("holding ↓: the first key commits, repeats only preview, key-up releases, the settled capsule commits")
    func keyRepeat() {
        var nav = SidebarNavigation()
        let hold = 2.0 / 12
        #expect(nav.handle(.step(1, isRepeat: false), in: outline, selection: .thisWeek, now: 10) == .commit(demo101, announce: true))
        _ = nav.handle(.destinationChanged(demo101), in: outline, selection: demo101, now: 10)
        _ = nav.handle(.pageDrawn, in: outline, selection: demo101, now: 10.1)
        #expect(nav.previewHold == hold)
        #expect(nav.handle(.step(1, isRepeat: true), in: outline, selection: demo101, now: 10.5) == .highlight(hold: hold))
        #expect(nav.handle(.step(1, isRepeat: true), in: outline, selection: demo101, now: 10.58) == .highlight(hold: hold))
        #expect(nav.handle(.step(1, isRepeat: true), in: outline, selection: demo101, now: 10.67) == .highlight(hold: hold))
        #expect(nav.pending == .preview(.sources, announce: true))
        #expect(nav.handle(.keyUp, in: outline, selection: demo101, now: 10.7) == .releasePreview)
        #expect(nav.handle(.previewSettled, in: outline, selection: demo101, now: 10.9) == .commit(.sources, announce: true))
        #expect(nav.handle(.keyUp, in: outline, selection: .sources, now: 11) == .ignored)
        // A repeat at the end of the list changes nothing (the hold runs out: it commits).
        _ = nav.handle(.step(1, isRepeat: true), in: outline, selection: .sources, now: 12)
        #expect(nav.pending == .preview(.connect, announce: true))
        #expect(nav.handle(.step(1, isRepeat: true), in: outline, selection: .sources, now: 12.08) == .handled)
    }

    @Test("a preview commits on focus loss, and queues when a page is in flight at release")
    func previewEdges() {
        var nav = SidebarNavigation()
        _ = nav.handle(.step(1, isRepeat: true), in: outline, selection: .thisWeek, now: 10)
        #expect(nav.handle(.focusLeft, in: outline, selection: .thisWeek, now: 10.1) == .commit(demo101, announce: true))
        var inFlight = SidebarNavigation()
        _ = inFlight.handle(.step(1, isRepeat: true), in: outline, selection: .thisWeek, now: 10)
        _ = inFlight.handle(.step(1, isRepeat: true), in: outline, selection: .thisWeek, now: 10.08)
        _ = inFlight.handle(.destinationChanged(.thisWeek), in: outline, selection: .thisWeek, now: 10.09)
        // destinationChanged dropped the preview (the later change wins): nothing to settle.
        #expect(inFlight.pending == nil)
        #expect(inFlight.handle(.previewSettled, in: outline, selection: .thisWeek, now: 10.1) == .handled)
        _ = inFlight.handle(.step(1, isRepeat: true), in: outline, selection: .thisWeek, now: 10.11)
        #expect(inFlight.handle(.previewSettled, in: outline, selection: .thisWeek, now: 10.2) == .highlight(hold: nil))
        #expect(inFlight.pending == .queued(demo101, announce: true))
        #expect(inFlight.handle(.pageDrawn, in: outline, selection: .thisWeek, now: 10.25) == .commit(demo101, announce: true))
    }

    @Test("a change of the destination from elsewhere drops a preview or a queued row")
    func destinationChangedDrops() {
        var nav = SidebarNavigation()
        _ = nav.handle(.pointerDown(demo101), in: outline, selection: demo101, now: idle)
        _ = nav.handle(.pointerMoved(demo310), in: outline, selection: demo101, now: idle)
        #expect(nav.handle(.destinationChanged(.connect), in: outline, selection: .connect, now: idle) == .highlight(hold: nil))
        #expect(nav.pending == nil)
        _ = nav.handle(.step(-1, isRepeat: false), in: outline, selection: .connect, now: idle + 0.01)
        #expect(nav.pending == .queued(.sources, announce: true))
        #expect(nav.handle(.destinationChanged(.thisWeek), in: outline, selection: .thisWeek, now: idle + 0.02) == .highlight(hold: nil))
        #expect(nav.pending == nil)
        #expect(nav.handle(.destinationChanged(.thisWeek), in: outline, selection: .thisWeek, now: idle + 0.03) == .handled)
    }

    @Test("VoiceOver's press commits without an announcement")
    func activate() {
        var nav = SidebarNavigation()
        #expect(nav.handle(.activate(.sources), in: outline, selection: .thisWeek, now: idle) == .commit(.sources, announce: false))
        #expect(nav.handle(.activate(.sources), in: outline, selection: .sources, now: idle) == .handled)
    }
}
