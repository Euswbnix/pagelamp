// The sidebar's selection (spec §2.3, §4.1; user decision 2026-09-26): one Liquid Glass capsule
// behind the highlighted row, like the selected segment of a segmented control, that slides to a
// newly selected row.
//
// The glass and its spring are the shared glass thumb (GlassThumb.swift: `NSGlassEffectView`
// moved by an additive Core Animation spring that the render server draws). Here the slide starts
// only on an idle main thread, after the new page's first frame (`SidebarMotionGate`, driven by a
// display link that runs only while the gate has something to wait for). Chrome/ is the only
// folder with glass (spec §1.3; lint also greps NSGlassEffect).

import AppKit
import QuartzCore
import SwiftUI
import PageLampModel

/// Where the capsule goes: the highlighted row (by destination) and its frame in the list.
struct SidebarCapsuleTarget: Equatable {
    var destination: Destination
    var top: CGFloat
    var height: CGFloat
}

/// How the capsule looks besides where it is.
struct SidebarCapsuleStyle: Equatable {
    /// The keyboard focus ring: 3 pt `keyboardFocusIndicatorColor`, just outside the glass.
    var focusRing = false
    /// Show Borders (spec §7.2): a 1 pt `separatorColor` outline inside the edge.
    var border = false
}

/// Imperative calls from SidebarList that need no SwiftUI update (key-repeat previews). SwiftUI
/// doesn't observe it; the representable keeps `host` current.
@MainActor
final class SidebarCapsuleHandle {
    fileprivate weak var host: SidebarCapsuleHostView?

    /// A key-repeat preview: it counts as released if nothing follows within `seconds`.
    func holdPreview(for seconds: TimeInterval) { host?.holdPreview(for: seconds) }
    /// Key-up, mouse-up: `onPreviewSettled` once the capsule reaches the previewed row.
    func releasePreview() { host?.releasePreview() }
    func cancelPreview() { host?.cancelPreview() }
}

/// The capsule layer behind the sidebar's rows (their `.background`, so it spans them and
/// scrolls with them). Never takes the pointer or VoiceOver focus.
struct SidebarSelectionGlass: NSViewRepresentable {
    /// nil: no capsule (the destination has no row, e.g. a hidden course).
    var target: SidebarCapsuleTarget?
    /// `AppModel.destination`: a change means a page switch went out in this update.
    var committed: Destination
    /// False under Reduce Motion and until the window has restored its destination.
    var animates: Bool
    /// Debug (`DebugPreferences.sidebarCapsuleLeads`): slide in the frame of the choice instead of
    /// after the new page is drawn.
    var leadsPage = false
    var style: SidebarCapsuleStyle
    var handle: SidebarCapsuleHandle
    /// The first frame after a page switch was drawn (`SidebarEvent.pageDrawn`).
    var onPageDrawn: () -> Void
    /// A released preview's capsule reached its row (`SidebarEvent.previewSettled`).
    var onPreviewSettled: () -> Void

    func makeNSView(context: Context) -> SidebarCapsuleHostView {
        let view = SidebarCapsuleHostView()
        handle.host = view
        return view
    }

    func updateNSView(_ view: SidebarCapsuleHostView, context: Context) {
        handle.host = view
        view.onPageDrawn = onPageDrawn
        view.onPreviewSettled = onPreviewSettled
        view.update(
            target: target, committed: committed,
            animates: animates && !context.transaction.disablesAnimations, leadsPage: leadsPage, style: style
        )
    }

    /// Spans the rows; never measured.
    func sizeThatFits(_ proposal: ProposedViewSize, nsView: SidebarCapsuleHostView, context: Context) -> CGSize? {
        CGSize(width: proposal.width ?? 0, height: proposal.height ?? 0)
    }
}

/// Places the one capsule: y and height from the target, x and width from its own bounds (so a
/// column resize never goes through SwiftUI state). Runs the motion gate on a display link.
final class SidebarCapsuleHostView: NSView {
    var onPageDrawn: (() -> Void)?
    var onPreviewSettled: (() -> Void)?

    /// Off: the glass is untinted, like the segmented control's selected segment. If on-device
    /// review finds it too faint on the sidebar glass in light mode, a neutral tint (never the
    /// accent) is the fallback (spec §9): white at 25 %, 8 % in dark mode.
    private static let usesNeutralTint = false

    private let mover = GlassThumbMover(
        view: GlassThumbView(shape: .capsule, outline: true, ring: true, neutralTint: SidebarCapsuleHostView.usesNeutralTint)
    )
    private var capsule: GlassThumbView { mover.view }
    private var gate = SidebarMotionGate()
    private var link: CADisplayLink?
    /// The latest target (a slide always goes to it, however often it changed while waiting).
    private var target: SidebarCapsuleTarget?
    /// Where the capsule stands (or is sliding to).
    private var placed: SidebarCapsuleTarget?
    private var lastCommitted: Destination?
    private var animatedBefore = false
    /// The capsule went away while motion was allowed (the student chose a hidden course, or hid
    /// the selected one), so its row coming back fades in. At launch it goes away in the update
    /// that restores a course whose row doesn't exist yet (no motion there), and it appears
    /// without a fade once the courses have loaded.
    private var fadesInWhenShown = false

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = true
        capsule.isHidden = true
        addSubview(capsule)
        setAccessibilityElement(false)
        NotificationCenter.default.addObserver(
            self, selector: #selector(systemColorsChanged), name: NSColor.systemColorsDidChangeNotification, object: nil
        )
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { nil }

    override var isFlipped: Bool { true }
    /// Rows take every click.
    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    /// For the performance probe: the one layer it reads per frame, and the display frame in
    /// which the last slide was added (the probe's read in that same frame sees the new position
    /// before its animation is committed).
    var capsuleLayer: CALayer? { mover.layer }
    private(set) var lastSlideFrame: CFTimeInterval = 0

    func update(target new: SidebarCapsuleTarget?, committed: Destination, animates: Bool, leadsPage: Bool, style: SidebarCapsuleStyle) {
        // Slides need animation in this update and the last: the update that restores the
        // destination at launch (the one that turns `animates` on) jumps.
        let canAnimate = animates && animatedBefore && window != nil
        animatedBefore = animates
        capsule.showsRing = style.focusRing
        capsule.showsOutline = style.border
        if let lastCommitted, lastCommitted != committed {
            gate.pageCommitted()
            wake()
        }
        lastCommitted = committed
        target = new

        guard let new else {
            // Only the update that hides it decides (later updates while hidden don't).
            if !capsule.isHidden { fadesInWhenShown = canAnimate }
            hide()
            return
        }
        if placed == nil || capsule.isHidden {
            show(at: new, fading: canAnimate && fadesInWhenShown)
        } else if placed?.destination == new.destination || !canAnimate {
            // The row moved with the list (courses loaded, the icon size changed), the selection
            // came back before its slide started, or Reduce Motion: follow at once.
            gate.cancelSlide()
            if placed != new { snap(to: new) }
        } else if leadsPage {
            gate.cancelSlide()
            slide(to: new)
        } else {
            gate.armSlide(at: CACurrentMediaTime())
            wake()
        }
    }

    func holdPreview(for seconds: TimeInterval) {
        gate.holdPreview(until: CACurrentMediaTime() + seconds)
        wake()
    }

    func releasePreview() {
        gate.releasePreview(at: CACurrentMediaTime())
        wake()
    }

    func cancelPreview() {
        gate.cancelPreview()
        wake()
    }

    // MARK: Display link

    override func viewWillMove(toWindow newWindow: NSWindow?) {
        super.viewWillMove(toWindow: newWindow)
        // The link retains its target: drop it with the old window.
        link?.invalidate()
        link = nil
        guard newWindow == nil else { return }
        // Nothing can wait for frames outside a window: pending callbacks happen now (after this
        // view update), a waiting slide becomes a jump.
        var drawn = false
        var settled = false
        for action in gate.flush() {
            switch action {
            case .pageDrawn: drawn = true
            case .startSlide: if let target { snap(to: target) }
            case .commitPreview: settled = true
            }
        }
        let (pageDrawn, previewSettled) = (onPageDrawn, onPreviewSettled)
        if drawn || settled {
            Task { @MainActor in
                if drawn { pageDrawn?() }
                if settled { previewSettled?() }
            }
        }
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        guard window != nil else { return }
        let link = displayLink(target: self, selector: #selector(frameTick(_:)))
        link.add(to: .main, forMode: .common)
        link.isPaused = !gate.isAwake
        self.link = link
        PerfProbe.register(capsuleHost: self)  // does nothing unless the probe runs
    }

    private func wake() {
        link?.isPaused = !gate.isAwake
    }

    @objc private func frameTick(_ link: CADisplayLink) {
        let waitsToSettle = gate.previewReleasedAt != nil || gate.previewDeadline != nil
        let actions = gate.frame(
            at: link.timestamp, duration: link.targetTimestamp - link.timestamp,
            capsuleOffset: waitsToSettle ? capsuleOffset() : 0
        )
        for action in actions {
            switch action {
            case .pageDrawn: onPageDrawn?()
            case .startSlide:
                if let target, !capsule.isHidden {
                    slide(to: target)
                    lastSlideFrame = link.timestamp
                }
            case .commitPreview: onPreviewSettled?()
            }
        }
        link.isPaused = !gate.isAwake
    }

    /// How far the capsule on screen still is from the target row. One layer read per frame,
    /// never a tree walk; a slide that hasn't started counts its whole way.
    private func capsuleOffset() -> Double {
        let waiting = (target?.top ?? 0) - (placed?.top ?? 0)
        return Double(mover.onScreenOffset() + abs(waiting))
    }

    // MARK: Placement

    override func layout() {
        super.layout()
        // A new column width: same origin, new width. Neither `position` nor a running slide
        // changes (the layer's anchor is its origin).
        guard let placed else { return }
        let frame = frame(for: placed)
        if capsule.frame.size.width != frame.width {
            mover.resize(width: frame.width)
        }
    }

    private func frame(for target: SidebarCapsuleTarget) -> NSRect {
        NSRect(
            x: SidebarMetrics.capsuleInset, y: target.top,
            width: max(bounds.width - 2 * SidebarMetrics.capsuleInset, 0), height: target.height
        )
    }

    private func show(at target: SidebarCapsuleTarget, fading: Bool) {
        gate.cancelSlide()
        mover.show(frame(for: target), fadeIn: fading ? PLMotion.quickSpring.duration : nil)
        placed = target
    }

    private func hide() {
        gate.cancelSlide()
        mover.hide()
        placed = nil
    }

    private func snap(to target: SidebarCapsuleTarget) {
        mover.place(frame(for: target))
        placed = target
    }

    /// Additive (GlassThumbMover): a slide still running keeps going and adds up.
    private func slide(to target: SidebarCapsuleTarget) {
        mover.slide(to: frame(for: target), spring: PLMotion.quickSpring)
        placed = target
    }

    /// The accent (focus ring) or the separator changed in System Settings.
    @objc private func systemColorsChanged() {
        capsule.colorsChanged()
    }
}
