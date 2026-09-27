// One Liquid Glass selection shape that slides (spec §1.3, §4.1): the sidebar's selection capsule
// (§2.3) and the course section picker's thumb (§3.2.1) are the same machinery.
//
// AppKit glass (`NSGlassEffectView`: the same SDF + glassBackground stack as the system segmented
// control's thumb) moved by an additive Core Animation spring on `position`: a committed CA
// animation is drawn by the render server, while a SwiftUI animation is advanced frame by frame on
// the main thread and stutters whenever a page is built. When a slide starts is the host's call
// (the sidebar waits for the new page's first frame; the picker starts in the input's own frame).
// Chrome/ is the only folder with glass (spec §1.3; lint also greps NSGlassEffect).

import AppKit
import QuartzCore

/// The glass's outline.
enum GlassThumbShape: Equatable {
    /// Radius half the height, circular curve (the sidebar's capsule).
    case capsule
    /// A fixed radius, continuous curve (the picker's thumb: 4 pt, concentric with its track).
    case rounded(CGFloat)

    func radius(height: CGFloat) -> CGFloat {
        switch self {
        case .capsule: height / 2
        case .rounded(let radius): radius
        }
    }

    var curve: CALayerCornerCurve {
        switch self {
        case .capsule: .circular
        case .rounded: .continuous
        }
    }
}

/// The glass and, optionally, a Show Borders outline and a keyboard focus ring. Outline and ring
/// are siblings of the glass, never inside it (`NSGlassEffectView` guarantees the z-order of its
/// contentView only). Never takes the pointer or VoiceOver focus.
final class GlassThumbView: NSView {
    private static let ringOutset: CGFloat = 3

    private let shape: GlassThumbShape
    private let neutralTint: Bool
    private let glass = NSGlassEffectView()
    private let outline: ThumbStroke?
    private let ring: ThumbStroke?

    /// The keyboard focus ring: 3 pt `keyboardFocusIndicatorColor`, just outside the glass.
    var showsRing = false {
        didSet { if showsRing != oldValue { ring?.isHidden = !showsRing } }
    }

    /// Show Borders (spec §7.2): a 1 pt `separatorColor` outline inside the edge.
    var showsOutline = false {
        didSet { if showsOutline != oldValue { outline?.isHidden = !showsOutline } }
    }

    /// `neutralTint`: the fallback if on-device review finds untinted glass too faint (spec §9):
    /// white at 25 %, 8 % in dark mode (never the accent).
    init(shape: GlassThumbShape, outline: Bool, ring: Bool, neutralTint: Bool = false) {
        self.shape = shape
        self.neutralTint = neutralTint
        self.outline = outline ? ThumbStroke(lineWidth: 1, color: .separatorColor, shape: shape, outset: 0) : nil
        self.ring = ring ? ThumbStroke(lineWidth: 3, color: .keyboardFocusIndicatorColor, shape: shape, outset: Self.ringOutset) : nil
        super.init(frame: .zero)
        wantsLayer = true
        glass.style = .regular  // never .clear (spec §1.3), never the accent (not a primary action)
        glass.tintColor = nil
        addSubview(glass)
        for stroke in [self.outline, self.ring].compactMap(\.self) {
            stroke.isHidden = true
            addSubview(stroke)
        }
        setAccessibilityElement(false)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { nil }

    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    /// A new size (row height, column width, segment width): the glass, its radius and the
    /// strokes follow at once.
    override func setFrameSize(_ newSize: NSSize) {
        let changed = newSize != frame.size
        super.setFrameSize(newSize)
        if changed { layoutParts() }
    }

    override func layout() {
        super.layout()
        layoutParts()
    }

    private func layoutParts() {
        glass.frame = bounds
        glass.cornerRadius = shape.radius(height: bounds.height)
        outline?.frame = bounds
        ring?.frame = bounds.insetBy(dx: -Self.ringOutset, dy: -Self.ringOutset)
        outline?.needsDisplay = true
        ring?.needsDisplay = true
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        guard neutralTint else { return }
        let dark = effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
        glass.tintColor = NSColor.white.withAlphaComponent(dark ? 0.08 : 0.25)
    }

    /// The accent (focus ring) or the separator changed in System Settings.
    func colorsChanged() {
        outline?.needsDisplay = true
        ring?.needsDisplay = true
    }
}

/// An outline drawn by its layer in the view's appearance (`updateLayer` runs again when the
/// appearance, Increase Contrast or a system colour changes), following the glass's shape
/// `outset` points outside it.
private final class ThumbStroke: NSView {
    private let lineWidth: CGFloat
    private let color: NSColor
    private let shape: GlassThumbShape
    private let outset: CGFloat

    init(lineWidth: CGFloat, color: NSColor, shape: GlassThumbShape, outset: CGFloat) {
        self.lineWidth = lineWidth
        self.color = color
        self.shape = shape
        self.outset = outset
        super.init(frame: .zero)
        wantsLayer = true
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { nil }

    override var wantsUpdateLayer: Bool { true }
    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    override func updateLayer() {
        guard let layer else { return }
        layer.borderWidth = lineWidth
        layer.borderColor = color.cgColor
        // A capsule's stroke is a capsule of its own height; a fixed radius grows by the outset.
        layer.cornerRadius = switch shape {
        case .capsule: bounds.height / 2
        case .rounded(let radius): radius + outset
        }
        layer.cornerCurve = shape.curve
    }
}

/// Places and moves one `GlassThumbView` (the host adds `view` as a subview and decides when).
/// Only the position animates: a slide is one additive `CASpringAnimation` from the old position
/// to the new one, so a slide still running keeps going and adds up (a new target bends the path
/// and the velocity carries over; no restart from the on-screen position). Sizes snap.
@MainActor
final class GlassThumbMover {
    let view: GlassThumbView
    private var slideCount = 0

    init(view: GlassThumbView) {
        self.view = view
    }

    /// The thumb's layer (the performance probes read it once per frame).
    var layer: CALayer? { view.layer }

    /// Straight to `frame`: running slides stop.
    func place(_ frame: NSRect) {
        removeSlides()
        setFrame(frame)
    }

    /// To `frame` with a spring; false when the position didn't change (nothing to animate).
    ///
    /// The new frame and its spring go to the render server in one transaction. Committed apart,
    /// the frame could be drawn without the spring (the thumb flashing at its destination): outside
    /// an event (VoiceOver's press, a display-link callback) no implicit transaction is open, so
    /// the frame's own transaction would be committed at once and the spring only with the next
    /// commit, possibly after a page build.
    @discardableResult
    func slide(to frame: NSRect, spring: PLMotion.Spring) -> Bool {
        guard let layer else {
            setFrame(frame)
            return false
        }
        CATransaction.begin()
        defer { CATransaction.commit() }
        let before = layer.position
        setFrame(frame)
        // Read back: right whatever AppKit's flipping and anchor are.
        let after = layer.position
        guard before != after else { return false }
        let animation = CASpringAnimation(perceptualDuration: spring.duration, bounce: spring.bounce)
        animation.keyPath = "position"
        animation.isAdditive = true
        animation.fromValue = NSValue(point: NSPoint(x: before.x - after.x, y: before.y - after.y))
        animation.toValue = NSValue(point: .zero)
        animation.duration = animation.settlingDuration
        slideCount &+= 1
        layer.add(animation, forKey: "slide.\(slideCount)")
        return true
    }

    /// Visible at `frame`, optionally fading in (opacity 0 → 1, ease-out) over `fadeIn` seconds.
    func show(_ frame: NSRect, fadeIn: TimeInterval?) {
        place(frame)
        view.isHidden = false
        guard let fadeIn, let layer else { return }
        let fade = CABasicAnimation(keyPath: "opacity")
        fade.fromValue = 0
        fade.toValue = 1
        fade.duration = fadeIn
        fade.timingFunction = CAMediaTimingFunction(name: .easeOut)
        layer.add(fade, forKey: "fade")
    }

    func hide() {
        removeSlides()
        view.isHidden = true
    }

    /// A new width at the same origin (a column resize). Neither `position` nor a running slide
    /// changes: the layer's anchor is its origin.
    func resize(width: CGFloat) {
        setFrame(NSRect(origin: view.frame.origin, size: NSSize(width: width, height: view.frame.height)))
    }

    /// How far the thumb on screen still is from where it is going.
    func onScreenOffset() -> CGFloat {
        guard let layer else { return 0 }
        let shown = layer.presentation()?.position ?? layer.position
        return hypot(shown.x - layer.position.x, shown.y - layer.position.y)
    }

    func removeSlides() {
        guard let layer else { return }
        for key in layer.animationKeys() ?? [] where key.hasPrefix("slide.") {
            layer.removeAnimation(forKey: key)
        }
    }

    private func setFrame(_ frame: NSRect) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        view.frame = frame
        CATransaction.commit()
    }
}
