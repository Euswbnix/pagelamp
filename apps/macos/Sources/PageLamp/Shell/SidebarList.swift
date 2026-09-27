// The sidebar (spec §2.3, M1): This Week, visible courses, Setup, and the footer status.
//
// A custom source list in the system sidebar column (whose glass the split view draws), because
// the selection is our one glass capsule that slides between rows (user decision 2026-09-26) and
// no public API restyles a List's selection (27 draws it per row as a solid accent fill). It stays
// indistinguishable from the system sidebar: SidebarLayout (PageLampModel) places rows and headers
// where the system source list does for the student's Sidebar icon size, SidebarNavigation
// (PageLampModel) decides what each key, pointer and drawing event does (the rules measured on the
// system list, plus coalescing and previews), and SidebarSelectionGlass (Chrome/) draws and moves
// the capsule. Like every macOS list it is one keyboard stop, with or without Full Keyboard Access.
//
// Motion order (no jank): a selection sets `AppModel.destination` at once, so the page switch goes
// first and the bold title moves with it; the capsule starts sliding only after the new page's
// first frame, on an idle main thread (SidebarMotionGate). Nothing in the sidebar uses a SwiftUI
// animation.

import AppKit
import QuartzCore
import SwiftUI
import PageLampKit
import PageLampModel

struct SidebarList: View {
    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n
    @Environment(\.sidebarRowSize) private var rowSize
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.accessibilityVoiceOverEnabled) private var voiceOver
    @Environment(\.accessibilityShowBorders) private var showBorders
    @Environment(\.appearsActive) private var appearsActive

    /// The navigation and the capsule handle, outside the view graph: a type-select keystroke
    /// or a drawn frame renders nothing unless the highlight or the focus ring changes.
    @State private var state = SidebarState()
    /// Mirrors of `state.navigation` that the body renders.
    @State private var pending: Destination?
    @State private var showsFocusRing = false
    @State private var pointerIsDown = false
    @GestureState private var pointerActive = false
    @FocusState private var focused: Bool

    var body: some View {
        let layout = SidebarLayout(outline: model.sidebarOutline(l10n: l10n), metrics: SidebarMetrics(.init(rowSize)))
        let highlighted = pending ?? model.destination
        ScrollViewReader { proxy in
            ScrollView {
                SidebarRows(layout: layout, highlighted: highlighted, selection: model.destination) { destination in
                    apply(.activate(destination), layout, proxy)
                }
                // VoiceOver: "Sidebar, list" (a lazy stack is a list), rows in order, headers as headings.
                .accessibilityElement(children: .contain)
                .accessibilityLabel(l10n("mac.a11y.sidebar"))
                .background(alignment: .topLeading) {
                    SidebarSelectionGlass(
                        target: layout.frame(of: highlighted).map {
                            SidebarCapsuleTarget(destination: highlighted, top: $0.top, height: $0.height)
                        },
                        committed: model.destination,
                        animates: !reduceMotion && model.restoredWindowState,
                        style: SidebarCapsuleStyle(focusRing: showsFocusRing && focused && appearsActive, border: showBorders),
                        handle: state.capsule,
                        onPageDrawn: { apply(.pageDrawn, layout, proxy) },
                        onPreviewSettled: { apply(.previewSettled, layout, proxy) }
                    )
                    .accessibilityHidden(true)
                }
                .contentShape(.rect)
                .gesture(pointer(layout, proxy))
                // One stop for the list; `.edit`: focusable by a click and by Tab whatever the Full
                // Keyboard Access setting, like every macOS list. The capsule draws the focus ring.
                .focusable(interactions: .edit)
                .focused($focused)
                .focusEffectDisabled()
                .onChange(of: focused) { _, focused in
                    apply(focused ? .focusEntered : .focusLeft, layout, proxy)
                    // VoiceOver lands on the list, which (unlike the system's outline) reports no
                    // selected row: when the keyboard brings focus here (Tab, ⌃F6; the ring shows),
                    // say the highlighted row. A click selects its row and says nothing.
                    if focused, voiceOver, state.navigation.showsFocusRing {
                        let row = state.navigation.highlighted(model.destination)
                        if layout.outline.contains(row) { announceRow(row) }
                    }
                }
                .onChange(of: pointerActive) { _, active in
                    // A press that ended without a release (the gesture was cancelled).
                    if !active, pointerIsDown {
                        pointerIsDown = false
                        apply(.pointerCancelled, layout, proxy)
                    }
                }
                .onKeyPress(keys: [.upArrow, .downArrow], phases: .all) { press in
                    let up = press.key == .upArrow
                    if press.phase == .up { return apply(.keyUp, layout, proxy) }
                    if press.modifiers.contains(.option) { return apply(up ? .first : .last, layout, proxy) }
                    return apply(.step(up ? -1 : 1, isRepeat: press.phase == .repeat), layout, proxy)
                }
                .onKeyPress(keys: [.home, .end]) { press in
                    apply(press.key == .home ? .home : .end, layout, proxy)
                }
                .onKeyPress(phases: [.down, .repeat]) { press in
                    guard let text = Self.typeSelectText(press) else { return .ignored }
                    return apply(.typed(text), layout, proxy)
                }
            }
            .onChange(of: highlighted) { _, destination in
                // Keys, menu commands, cross-links: the row comes into view, scrolling the least.
                proxy.scrollTo(SidebarOutline.Item.ID.row(destination))
            }
        }
        .onChange(of: model.destination) { _, destination in
            apply(.destinationChanged(destination), layout, nil)
        }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            SidebarFooterStatus()
        }
        .onReceive(NotificationCenter.default.publisher(for: PerfProbe.focusSidebar)) { _ in  // performance probe only
            focused = true
        }
    }

    /// A press selects its row at once (and focuses the list, like a click on any macOS list);
    /// dragging previews the row under the pointer; release commits it.
    private func pointer(_ layout: SidebarLayout, _ proxy: ScrollViewProxy) -> some Gesture {
        DragGesture(minimumDistance: 0, coordinateSpace: .local)
            .updating($pointerActive) { _, active, _ in active = true }
            .onChanged { value in
                let row = layout.destination(atY: value.location.y)
                if pointerIsDown {
                    apply(.pointerMoved(row), layout, proxy)
                } else {
                    pointerIsDown = true
                    apply(.pointerDown(row), layout, proxy)
                    focused = true
                }
            }
            .onEnded { _ in
                pointerIsDown = false
                apply(.pointerUp, layout, proxy)
            }
    }

    /// Forwards an event to the navigation and carries out its outcome.
    @discardableResult
    private func apply(_ event: SidebarEvent, _ layout: SidebarLayout, _ proxy: ScrollViewProxy?) -> KeyPress.Result {
        let outcome = state.navigation.handle(event, in: layout.outline, selection: model.destination, now: CACurrentMediaTime())
        defer {
            // Render only what changed.
            let navigation = state.navigation
            if pending != navigation.pending?.destination { pending = navigation.pending?.destination }
            if showsFocusRing != navigation.showsFocusRing { showsFocusRing = navigation.showsFocusRing }
        }
        switch outcome {
        case .ignored:
            return .ignored
        case .handled:
            return .handled
        case .commit(let destination, let announce):
            state.capsule.cancelPreview()
            model.destination = destination  // the page switch goes first
            // VoiceOver stays on the list while the keyboard moves the selection: say the new row.
            if announce, voiceOver { announceRow(destination) }
            return .handled
        case .highlight(let hold):
            if state.navigation.pending == nil { state.capsule.cancelPreview() }
            if let hold { state.capsule.holdPreview(for: hold) }
            return .handled
        case .releasePreview:
            state.capsule.releasePreview()
            return .handled
        case .scroll(let edge):
            let ids = layout.outline.items.map(\.id)
            if let id = edge == .top ? ids.first : ids.last {
                proxy?.scrollTo(id, anchor: edge == .top ? .top : .bottom)
            }
            return .handled
        }
    }

    private func announceRow(_ destination: Destination) {
        let row = SidebarRowContent(destination, model: model, l10n: l10n)
        let text = row.value.isEmpty ? row.label : l10n("mac.a11y.sidebarRowAnnouncement", ["label": row.label, "value": row.value])
        AccessibilityNotification.Announcement(text).post()
    }

    /// Type-select input: printable text typed without ⌘ or ⌃ (arrows and other function keys
    /// arrive as private-use characters; Return, Tab and Esc as control characters).
    private static func typeSelectText(_ press: KeyPress) -> String? {
        guard press.modifiers.isDisjoint(with: [.command, .control]), !press.characters.isEmpty else { return nil }
        let printable = press.characters.unicodeScalars.allSatisfy {
            !CharacterSet.controlCharacters.contains($0) && !(0xF700...0xF8FF).contains($0.value)
        }
        return printable ? press.characters : nil
    }
}

/// The sidebar's transient state, which SwiftUI doesn't observe.
@MainActor
private final class SidebarState {
    var navigation = SidebarNavigation(keyRepeatDelay: NSEvent.keyRepeatDelay, keyRepeatInterval: NSEvent.keyRepeatInterval)
    let capsule = SidebarCapsuleHandle()
}

extension SidebarMetrics.Size {
    /// The student's Sidebar icon size (System Settings ▸ Appearance).
    init(_ size: SidebarRowSize) {
        switch size {
        case .small: self = .small
        case .large: self = .large
        default: self = .medium
        }
    }
}

/// The rows and headers, where SidebarLayout puts them (the snapshots render this without the
/// scroll view, the capsule and the keyboard handling). Lazy on purpose: VoiceOver reads a lazy
/// stack as a list ("Sidebar, list", like the system sidebar); it holds a dozen fixed-height rows.
package struct SidebarRows: View {
    let layout: SidebarLayout
    /// The row drawn as selected (bold title): a pending row, else the destination.
    let highlighted: Destination
    /// `AppModel.destination`: the row VoiceOver calls selected.
    let selection: Destination
    /// VoiceOver's press on a row.
    let activate: (Destination) -> Void

    package init(layout: SidebarLayout, highlighted: Destination, selection: Destination, activate: @escaping (Destination) -> Void = { _ in }) {
        self.layout = layout
        self.highlighted = highlighted
        self.selection = selection
        self.activate = activate
    }

    package var body: some View {
        LazyVStack(alignment: .leading, spacing: 0) {
            ForEach(Array(layout.outline.items.enumerated()), id: \.element.id) { index, item in
                switch item {
                case .header(let section):
                    SidebarHeader(section: section)
                        .padding(.top, layout.gap(before: index))
                        .id(item.id)
                case .row(let destination, _):
                    SidebarRow(
                        destination: destination,
                        isHighlighted: destination == highlighted,
                        isSelected: destination == selection,
                        metrics: layout.metrics,
                        activate: activate
                    )
                    .id(item.id)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// "Courses", "Setup": 11 pt Semibold, secondary, 19 pt tall (the system's section header).
struct SidebarHeader: View {
    let section: SidebarOutline.Section
    @Environment(\.l10n) private var l10n

    var body: some View {
        Text(section == .courses ? l10n("common.nav.courses") : l10n("mac.sidebar.setup"))
            .font(.system(size: SidebarMetrics.headerFontSize, weight: .semibold))
            .foregroundStyle(.secondary)
            .lineLimit(1)
            .padding(.leading, SidebarMetrics.headerInset)
            .frame(maxWidth: .infinity, alignment: .leading)
            .frame(height: SidebarMetrics.headerHeight)
            .accessibilityAddTraits(.isHeader)
    }
}

/// One row: icon (accent while the window is active), title (Semibold when highlighted),
/// trailing status. It draws no fill: the capsule behind the rows is the selection.
struct SidebarRow: View {
    let destination: Destination
    let isHighlighted: Bool
    let isSelected: Bool
    let metrics: SidebarMetrics
    let activate: (Destination) -> Void

    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n
    @Environment(\.appearsActive) private var appearsActive

    var body: some View {
        let content = SidebarRowContent(destination, model: model, l10n: l10n)
        HStack(spacing: 0) {
            Image(systemName: content.symbol)
                .font(.system(size: metrics.iconSize, weight: .medium))
                .imageScale(.large)
                // The student's accent, as 27 colours sidebar icons; grey while the window is in
                // the background.
                .foregroundStyle(appearsActive ? AnyShapeStyle(.tint) : AnyShapeStyle(.secondary))
                .frame(width: metrics.iconColumn)
                .padding(.trailing, SidebarMetrics.iconTitleGap)
            Text(content.title)
                .font(.system(size: metrics.titleSize, weight: isHighlighted ? .semibold : .regular))
                .lineLimit(1)
            Spacer(minLength: PLSpace.s1)
            // The week and the glyphs never wrap; the title truncates first (its tooltip has the name).
            trailing(content.trailing)
                .fixedSize()
        }
        .padding(.horizontal, SidebarMetrics.cellInset)
        .frame(height: metrics.rowHeight)
        // A tooltip only: the combined element below drops it, and its label already has the
        // full name (an accessibility help would make VoiceOver say the name twice).
        .modifier(CourseNameHelp(name: content.help))
        // VoiceOver's outline spans the whole row the capsule marks, not only icon and text
        // (hit testing is unchanged).
        .contentShape(.accessibility, .rect)
        // One element per row, read like the system's: its title (a course's full sentence) and
        // value, "selected", pressable; not a button (the order of these modifiers matters).
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(content.label)
        .accessibilityValue(content.value)
        .accessibilityAddTraits(isSelected ? .isSelected : [])
        .accessibilityAction { activate(destination) }
        .accessibilityRemoveTraits(.isButton)
        .accessibilityAddTraits(.isStaticText)
    }

    @ViewBuilder private func trailing(_ trailing: SidebarRowContent.Trailing) -> some View {
        switch trailing {
        case .none:
            EmptyView()
        case .course(let failing, let week):
            // Not a badge, which means a count.
            HStack(spacing: PLSpace.s1) {
                if failing {
                    Image(systemName: "exclamationmark.triangle")
                        .foregroundStyle(PLColor.warning)
                }
                Text(week)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
            }
            .font(PLType.callout.font)
        case .count(let count):
            Text(count)
                .font(PLType.callout.font)
                .monospacedDigit()
                .foregroundStyle(.secondary)
        case .warning:
            Image(systemName: "exclamationmark.triangle")
                .font(PLType.callout.font)
                .foregroundStyle(PLColor.warning)
        }
    }
}

/// The course's full name as the row's tooltip (courses only).
private struct CourseNameHelp: ViewModifier {
    let name: String?

    func body(content: Content) -> some View {
        if let name {
            content.help(name)
        } else {
            content
        }
    }
}

/// What a row shows and says.
struct SidebarRowContent {
    enum Trailing: Equatable {
        case none
        /// A warning glyph if the course's source fails, then "Wk 4" (or "—").
        case course(failing: Bool, week: String)
        /// Sources & Sync: how many sources fail (only when some do).
        case count(String)
        /// Connect: the app runs from a temporary location (S15).
        case warning
    }

    let symbol: String
    let title: String
    let label: String
    let value: String
    /// A course's full name (its tooltip); nil for the other rows.
    let help: String?
    let trailing: Trailing

    init(_ destination: Destination, model: AppModel, l10n: L10n) {
        switch destination {
        case .thisWeek:
            let title = l10n("mac.nav.thisWeek")
            (symbol, self.title, label, value, help, trailing) = ("lamp.desk", title, title, "", nil, .none)
        case .course(let id):
            guard let summary = model.course(id: id) else {
                (symbol, title, label, value, help, trailing) = ("book.closed", id, id, "", nil, .none)
                return
            }
            let course = summary.course
            let week = summary.timeline.outsideTerm ? nil : summary.timeline.currentWeek
            let failing = model.isSourceFailing(course.sourceId)
            let noAI = course.aiPolicy == .prohibited
            let arguments = ["code": course.code ?? course.name, "name": course.name, "week": l10n.week(week)]
            // No AI is shown neutrally, never as an error (spec §1.2).
            symbol = noAI ? "hand.raised" : "book.closed"
            title = course.code ?? course.name
            label = switch (noAI, failing) {
            case (false, false): l10n("mac.a11y.sidebarCourse", arguments)
            case (true, false): l10n("mac.a11y.sidebarCourseNoAI", arguments)
            case (false, true): l10n("mac.a11y.sidebarCourseAttention", arguments)
            case (true, true): l10n("mac.a11y.sidebarCourseNoAIAttention", arguments)
            }
            value = ""
            help = course.name
            trailing = .course(failing: failing, week: l10n.compactWeek(week))
        case .sources:
            // The count of failing sources only (zero shows nothing); VoiceOver hears it in words.
            let failing = model.failingSources.count
            let title = l10n("mac.nav.sources")
            let value = failing > 0 ? l10n.plural("mac.a11y.sidebarSourcesFailing", count: failing) : ""
            (symbol, self.title, label, self.value, help) = ("folder.badge.gearshape", title, title, value, nil)
            trailing = failing > 0 ? .count(l10n.number(failing)) : .none
        case .connect:
            let title = l10n("mac.nav.connect")
            // The glyph is hidden from VoiceOver; the value says it in words (S15).
            let warning = model.temporaryLocation.map { ConnectSetup.temporaryLocationText($0, l10n: l10n).title }
            (symbol, self.title, label, value, help) = ("cable.connector", title, title, warning ?? "", nil)
            trailing = warning == nil ? .none : .warning
        }
    }
}

/// The persistent, low-emphasis status (spec §2.3): plain text, Subheadline, never glass, never
/// a count. A click opens Sources & Sync.
struct SidebarFooterStatus: View {
    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n
    @Environment(\.accessibilityShowBorders) private var showBorders

    var body: some View {
        Button {
            model.destination = .sources
        } label: {
            TimelineView(.everyMinute) { _ in
                let status = model.footerStatus
                Label {
                    Text(text(for: status))
                } icon: {
                    glyph(for: status)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .font(PLType.subheadline.font)
        .foregroundStyle(.secondary)
        .padding(.horizontal, PLSpace.s2)
        .padding(.vertical, PLSpace.s1)
        // Show Borders (spec §7.2): the plain footer gets a 1 pt outline.
        .overlay {
            if showBorders {
                RoundedRectangle(cornerRadius: PLRadius.row).strokeBorder(.separator, lineWidth: 1)
            }
        }
        .padding(.horizontal, PLSpace.s2)
        .padding(.vertical, PLSpace.s2)
        .accessibilityHint(l10n("mac.actions.openSourcesAndSync"))
    }

    private func text(for status: FooterStatus) -> String {
        switch status {
        case .syncing: l10n("common.sync.syncing")
        case .needsAttention: l10n("common.sync.needsAttention")
        case .synced(let date): l10n("common.sync.syncedAgo", ["when": l10n.relative(date, to: model.clock(), calendar: model.calendar)])
        case .neverSynced: l10n("common.sync.never")
        }
    }

    @ViewBuilder private func glyph(for status: FooterStatus) -> some View {
        switch status {
        case .syncing:
            Image(systemName: "arrow.triangle.2.circlepath")
        case .needsAttention:
            Image(systemName: "exclamationmark.triangle")
                .foregroundStyle(PLColor.warning)
        case .synced:
            Image(systemName: "checkmark.circle")
        case .neverSynced:
            Image(systemName: "circle.dashed")
        }
    }
}

/// The sidebar as the snapshots draw it: the rows below the toolbar's inset, a flat stand-in for
/// the glass capsule (glass and the NSView behind it don't render offscreen) and the footer.
package struct SidebarSnapshot: View {
    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n
    @Environment(\.sidebarRowSize) private var rowSize

    package init() {}

    package var body: some View {
        let layout = SidebarLayout(outline: model.sidebarOutline(l10n: l10n), metrics: SidebarMetrics(.init(rowSize)))
        VStack(spacing: 0) {
            SidebarRows(layout: layout, highlighted: model.destination, selection: model.destination)
                .background(alignment: .topLeading) {
                    GeometryReader { geometry in
                        if let frame = layout.capsuleFrame(for: model.destination, width: geometry.size.width) {
                            Capsule()
                                .fill(.fill.quaternary)
                                .overlay { Capsule().strokeBorder(.separator, lineWidth: 1) }
                                .frame(width: frame.width, height: frame.height)
                                .offset(x: frame.minX, y: frame.minY)
                        }
                    }
                }
                .padding(.top, WindowMetrics.toolbarInset)
            Spacer(minLength: 0)
            SidebarFooterStatus()
        }
    }
}
