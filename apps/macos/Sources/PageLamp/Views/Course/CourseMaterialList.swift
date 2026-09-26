// The week's materials (spec §3.2 MaterialRow, "Mac list behaviour"). The page stays a
// ScrollView (for the lamp's spill), so the rows are custom: click focuses, ↑/↓ move, Return or
// a double-click opens, ⌘C copies, and each row has a context menu. Space → Quick Look is M3.

import SwiftUI
import PageLampKit
import PageLampModel

struct CourseMaterialList: View {
    let materials: [MaterialView]
    let aiMaterials: AiMaterialsState

    @Environment(\.l10n) private var l10n
    @Environment(\.openURL) private var openURL
    @Environment(\.detailColumnWidth) private var detailWidth
    @FocusState private var focused: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(materials.enumerated()), id: \.element.id) { index, material in
                CourseMaterialRow(
                    material: material,
                    status: CourseMaterialStatus(material: material, aiMaterials: aiMaterials),
                    isFocused: focused == material.id,
                    compact: ReadingMeasure.isColumn(of: detailWidth, narrowerThan: CourseMaterialRow.compactBelow)
                )
                .focusable(interactions: .edit)
                .focused($focused, equals: material.id)
                .onTapGesture(count: 2) { open(material) }
                .simultaneousGesture(TapGesture().onEnded { focused = material.id })
                .contextMenu { menu(for: material) }
                .accessibilityAction { open(material) }
                if index < materials.count - 1 {
                    Divider().padding(.leading, PLLayout.rowPadding)
                }
            }
        }
        .onMoveCommand { direction in
            switch direction {
            case .up: moveFocus(by: -1)
            case .down: moveFocus(by: 1)
            default: break
            }
        }
        .onKeyPress(.return) {
            guard let material = focusedMaterial else { return .ignored }
            open(material)
            return .handled
        }
        .copyable(focusedMaterial.map { [$0.url ?? $0.title] } ?? [])
        .accessibilityRotor(
            Text(l10n("course.week.materials")),
            entries: materials,
            entryID: \.id,
            entryLabel: \.title
        )
    }

    private var focusedMaterial: MaterialView? {
        materials.first { $0.id == focused }
    }

    private func moveFocus(by delta: Int) {
        guard !materials.isEmpty else { return }
        guard let current = materials.firstIndex(where: { $0.id == focused }) else {
            focused = (delta > 0 ? materials.first : materials.last)?.id
            return
        }
        let next = min(max(current + delta, 0), materials.count - 1)
        focused = materials[next].id
    }

    private func open(_ material: MaterialView) {
        guard let link = CourseLink(material.url) else { return }
        Links.open(link, openURL: openURL)
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
/// Two lines, hairlines between rows, no cards. Focus: `.fill.secondary` (radius 8) plus the
/// system ring; hover: `.fill.quaternary`.
struct CourseMaterialRow: View {
    /// Below this list width the status moves under the title (narrow window, inspector open).
    static let compactBelow: CGFloat = 400

    let material: MaterialView
    let status: CourseMaterialStatus
    var isFocused: Bool
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
        .accessibilityAddTraits(.isButton)
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
        if isFocused { return AnyShapeStyle(.fill.secondary) }
        if hovered { return AnyShapeStyle(.fill.quaternary) }
        return AnyShapeStyle(.clear)
    }
}
