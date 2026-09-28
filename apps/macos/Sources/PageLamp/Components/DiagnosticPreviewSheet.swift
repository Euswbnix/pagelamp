// The diagnostic report preview (spec §3.5 Help, S2, S6): the report is always shown before it
// is copied. A sheet: its contents use fills and standard buttons, never glass.

import SwiftUI
import PageLampModel

extension View {
    /// Presents the diagnostic report preview on this window when `model.diagnosticReportHost`
    /// is `host` (the main window for the Help menu and S2, Settings for Settings ▸ Help).
    func diagnosticReportSheet(host: DiagnosticReportHost) -> some View {
        modifier(DiagnosticReportSheet(host: host))
    }
}

private struct DiagnosticReportSheet: ViewModifier {
    let host: DiagnosticReportHost
    @Environment(AppModel.self) private var model

    func body(content: Content) -> some View {
        content.sheet(isPresented: presented) {
            DiagnosticPreviewSheet()
                .pageLampEnvironment(model)
        }
    }

    private var presented: Binding<Bool> {
        Binding(
            get: { model.diagnosticReport != nil && model.diagnosticReportHost == host },
            set: { if !$0 { model.dismissDiagnosticReport() } }
        )
    }
}

struct DiagnosticPreviewSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n
    @State private var copied = false

    private var report: String? {
        if case .loaded(let text) = model.diagnosticReport { return text }
        return nil
    }

    var body: some View {
        VStack(alignment: .leading, spacing: PLSpace.s4) {
            Text(l10n("mac.diagnostics.previewTitle"))
                .font(PLType.title2.font.weight(.semibold))
                .accessibilityAddTraits(.isHeader)
            Text(l10n("common.diagnostics.previewNote") + " " + l10n("common.diagnostics.previewContents"))
                .font(PLType.callout.font)
                .foregroundStyle(.secondary)
                .paragraphLineSpacing()
                .fixedSize(horizontal: false, vertical: true)
            content
                .frame(maxWidth: .infinity, minHeight: 260, maxHeight: 360)
                .background(.fill.quaternary, in: .rect(cornerRadius: PLRadius.row))
            HStack {
                Spacer()
                Button(l10n("mac.actions.done")) { model.dismissDiagnosticReport() }
                    .keyboardShortcut(.cancelAction)
                Button(copied ? l10n("common.actions.copied") : l10n("mac.actions.copyReport")) {
                    guard let report else { return }
                    Pasteboard.copy(report, announce: l10n("common.actions.copied"))
                    copied = true
                }
                .keyboardShortcut(.defaultAction)
                .disabled(report == nil)
            }
        }
        .padding(PLLayout.sheetInset)
        .frame(width: 560)
    }

    @ViewBuilder private var content: some View {
        switch model.diagnosticReport {
        case .loaded(let text):
            ScrollView {
                // The report is English (the core writes it for maintainers); tagged so.
                Text.english(text)
                    .font(PLType.mono.font)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(PLSpace.s3)
            }
        case .failed(let failure):
            Text(failure.localizedDescription(in: l10n))
                .foregroundStyle(.secondary)
                .padding(PLSpace.s3)
        case .loading, nil:
            ProgressView()
                .controlSize(.small)
                .accessibilityLabel(l10n("common.states.loading"))
        }
    }
}
