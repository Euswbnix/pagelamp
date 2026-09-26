// The week's materials (spec §3.2 MaterialRow, "Mac list behaviour"). The page stays a
// ScrollView (for the lamp's spill), so the list is custom, and behaves like a table: the whole
// list is one keyboard stop (Tab enters and leaves it, with or without Full Keyboard Access, like
// every macOS list), a click selects a row, ↑/↓ move the selection, Return or a double-click
// opens, ⌘C copies, and each row has a context menu. Space → Quick Look is M3. The decisions
// (selection, what each key and click does, what opening does, what is copied) live in
// PageLampModel (MaterialListNavigation); the view forwards its events to `handle(_:)`.

import SwiftUI
import PageLampKit
import PageLampModel

struct CourseMaterialList: View {
    let materials: [MaterialView]
    let aiMaterials: AiMaterialsState

    @Environment(\.l10n) private var l10n
    @Environment(\.openURL) private var openURL
    @Environment(\.detailColumnWidth) private var detailWidth
    @State private var selection = MaterialListSelection()
    @FocusState private var listFocused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(materials.enumerated()), id: \.element.id) { index, material in
                CourseMaterialRow(
                    material: material,
                    status: CourseMaterialStatus(material: material, aiMaterials: aiMaterials),
                    isSelected: selection.selected == material.id,
                    showsFocusRing: listFocused && selection.selected == material.id,
                    compact: ReadingMeasure.isColumn(of: detailWidth, narrowerThan: CourseMaterialRow.compactBelow)
                )
                .onTapGesture(count: 2) { handle(.doubleClick(material.id)) }
                .simultaneousGesture(TapGesture().onEnded {
                    handle(.click(material.id))
                    listFocused = true
                })
                .contextMenu { menu(for: material) }
                .accessibilityAction { open(material) }
                if index < materials.count - 1 {
                    Divider().padding(.leading, PLLayout.rowPadding)
                }
            }
        }
        // One stop for the list. `.edit` (focusable by a click and by Tab whatever the Full
        // Keyboard Access setting) is how macOS treats lists; per-row stops would make Tab walk
        // every material. The selected row draws the focus ring instead of the whole list.
        .focusable(interactions: .edit)
        .focused($listFocused)
        .focusEffectDisabled()
        .onChange(of: listFocused) { _, focused in
            if focused { handle(.focusEntered) }
        }
        .onChange(of: materials.map(\.id)) { _, ids in selection.keep(in: ids) }
        .onMoveCommand { direction in
            switch direction {
            case .up: handle(.move(-1))
            case .down: handle(.move(1))
            default: break
            }
        }
        .onKeyPress(.return) {
            handle(.returnKey) ? .handled : .ignored
        }
        .copyable(selection.copyItems(materials))
        .accessibilityElement(children: .contain)
        .accessibilityRotor(
            Text(l10n("course.week.materials")),
            entries: materials,
            entryID: \.id,
            entryLabel: \.title
        )
    }

    /// Forwards an event to the selection's handler and carries out its effect; whether there
    /// was one (Return is handled only when it opens something).
    @discardableResult
    private func handle(_ event: MaterialListEvent) -> Bool {
        switch selection.handle(event, in: materials.map(\.id)) {
        case .open(let id)?:
            if let material = materials.first(where: { $0.id == id }) { open(material) }
            return true
        case nil:
            return false
        }
    }

    private func open(_ material: MaterialView) {
        Links.open(CourseLink(material.url), openURL: openURL)
    }

    @ViewBuilder private func menu(for material: MaterialView) -> some View {
        let link = CourseLink(material.url)
        Button(l10n("common.actions.open")) { open(material) }
            .disabled(link == nil)
        switch link {
        case .file(let url):
            Button(l10n("mac.actions.showInFinder")) { Links.showInFinder(url) }
            Button(l10n("mac.actions.copyPath")) { Pasteboard.copy(url.path(percentEncoded: false), announce: l10n("common.actions.copied")) }
        case .web(let url):
            Button(l10n("mac.actions.copyLink")) { Pasteboard.copy(url.absoluteString, announce: l10n("common.actions.copied")) }
        case nil:
            EmptyView()
        }
        Button(l10n("mac.actions.copyTitle")) { Pasteboard.copy(material.title, announce: l10n("common.actions.copied")) }
    }
}

/// One material: kind glyph, title, "File · Week 4: Sampling · Sep 22", and a trailing status.
/// Two lines, hairlines between rows, no cards. Selected: `.fill.secondary` (radius 8), plus
/// the system focus ring while the list has keyboard focus; hover: `.fill.quaternary`.
struct CourseMaterialRow: View {
    /// Below this list width the status moves under the title (narrow window, inspector open).
    static let compactBelow: CGFloat = 400

    let material: MaterialView
    let status: CourseMaterialStatus
    var isSelected: Bool
    var showsFocusRing = false
    var compact = false

    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var hovered = false

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: PLSpace.s3) {
            Image(systemName: material.kind.symbol)
                .symbolRenderingMode(.hierarchical)
                .foregroundStyle(.secondary)
                .frame(width: PLSpace.s5)
            VStack(alignment: .leading, spacing: 2) {
                Text(material.title)
                    .font(PLType.body.font)
                    .fixedSize(horizontal: false, vertical: true)
                Text(meta)
                    .font(PLType.callout.font)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                if let detail {
                    detail
                        .font(PLType.callout.font)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                if compact {
                    statusLabel
                        .padding(.top, 2)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            if !compact {
                statusLabel
            }
        }
        .padding(PLLayout.rowPadding)
        .background(fill, in: .rect(cornerRadius: PLRadius.row))
        .overlay {
            if showsFocusRing {
                RoundedRectangle(cornerRadius: PLRadius.row)
                    .strokeBorder(Color(nsColor: .keyboardFocusIndicatorColor), lineWidth: 3)
            }
        }
        .rowBorder()
        .contentShape(.rect(cornerRadius: PLRadius.row))
        .onHover { hovering in
            withAnimation(reduceMotion ? nil : PLMotion.hover) { hovered = hovering }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(l10n("mac.course.a11y.materialRow", [
            "title": material.title,
            "meta": meta,
            "status": [l10n(status.spokenKey), status.blockKey.map { l10n($0) }].compactMap(\.self).joined(separator: ", "),
        ]))
        .accessibilityAddTraits(isSelected ? [.isButton, .isSelected] : .isButton)
    }

    /// A glyph and a short word; the tone is on the glyph only.
    private var statusLabel: some View {
        HStack(alignment: .firstTextBaseline, spacing: PLSpace.s1) {
            Image(systemName: status.symbol)
                .symbolRenderingMode(.hierarchical)
                .foregroundStyle(glyphStyle)
            Text(l10n(status.shortKey))
        }
        .font(PLType.callout.font)
        .fixedSize()
    }

    /// "File · Week 4: Sampling · Sep 22"
    private var meta: String {
        var parts = [l10n.materialKind(material.kind)]
        if let module = material.moduleName, !module.isEmpty { parts.append(module) }
        if let published = material.publishedAt {
            parts.append(CourseDates.published(published, now: model.clock(), calendar: model.calendar, l10n: l10n))
        }
        return parts.joined(separator: " · ")
    }

    /// Why a Canvas file can't be downloaded, or the core's extraction error (English, tagged).
    private var detail: Text? {
        if let key = status.blockKey { return Text(l10n(key)) }
        if status == .extractFailed, let error = material.textError, !error.isEmpty {
            return Text.english(error)
        }
        return nil
    }

    private var glyphStyle: AnyShapeStyle {
        switch status.tone {
        case .success: AnyShapeStyle(PLColor.success)
        case .warning: AnyShapeStyle(PLColor.warning)
        case .neutral: AnyShapeStyle(.secondary)
        }
    }

    private var fill: AnyShapeStyle {
        if isSelected { return AnyShapeStyle(.fill.secondary) }
        if hovered { return AnyShapeStyle(.fill.quaternary) }
        return AnyShapeStyle(.clear)
    }
}
