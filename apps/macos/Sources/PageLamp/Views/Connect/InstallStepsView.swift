// The numbered setup steps of one AI app (spec §3.4, W7; `ConnectSetup.steps`), the client's
// "Good to know" notes, and the helpers the steps use (config path, Show in Finder).

import AppKit
import SwiftUI
import PageLampKit
import PageLampModel

struct InstallStepsView: View {
    let config: McpClientConfig
    /// S15: copying PageLamp's path first asks "Copy anyway?".
    let temporaryLocation: Bool

    @Environment(\.l10n) private var l10n

    var body: some View {
        let steps = ConnectSetup.steps(for: config, l10n: l10n)
        VStack(alignment: .leading, spacing: PLSpace.s5) {
            ForEach(Array(steps.enumerated()), id: \.element.id) { index, step in
                HStack(alignment: .firstTextBaseline, spacing: PLSpace.s3) {
                    StepNumber(number: index + 1)
                    VStack(alignment: .leading, spacing: PLSpace.s3) {
                        Text(step.text)
                            .font(PLType.body.font)
                            .paragraphLineSpacing()
                            .fixedSize(horizontal: false, vertical: true)
                        ForEach(Array(step.extras.enumerated()), id: \.offset) { _, extra in
                            ExtraView(extra: extra, temporaryLocation: temporaryLocation)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                .accessibilityElement(children: .contain)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel(l10n("connect.steps.label", ["app": config.title]))
    }
}

/// "1" in a 22 pt ring, aligned with the step's first line.
private struct StepNumber: View {
    let number: Int
    @Environment(\.l10n) private var l10n

    var body: some View {
        Text(l10n.number(number))
            .font(PLType.callout.font)
            .monospacedDigit()
            .foregroundStyle(.secondary)
            .frame(width: PLSize.stepNumber, height: PLSize.stepNumber)
            .background {
                Circle().strokeBorder(.separator, lineWidth: 1)
            }
            .accessibilityHidden(true)
    }
}

private struct ExtraView: View {
    let extra: InstallStep.Extra
    let temporaryLocation: Bool
    @Environment(\.l10n) private var l10n

    var body: some View {
        switch extra {
        case .path(let path):
            CodeBlock(copyable: path) {
                ShowConfigInFinderButton(pathHint: path.text)
            }
        case .code(let code):
            CodeBlock(copyable: code, temporaryLocation: temporaryLocation)
        case .variant(let title, let text, let code):
            if let title {
                // The paste variants for a file that isn't new (collapsed; the full snippet above
                // is for a new or empty file).
                VariantDisclosure(title: title, text: text, code: code, temporaryLocation: temporaryLocation)
            } else {
                VStack(alignment: .leading, spacing: PLSpace.s2) {
                    Text(text)
                        .paragraphLineSpacing()
                        .fixedSize(horizontal: false, vertical: true)
                    if let code {
                        CodeBlock(copyable: code, temporaryLocation: temporaryLocation)
                    }
                }
            }
        case .note(let text):
            Text(text)
                .foregroundStyle(.secondary)
                .paragraphLineSpacing()
                .fixedSize(horizontal: false, vertical: true)
        }
    }
}

private struct VariantDisclosure: View {
    let title: String
    let text: String
    let code: ConnectCopyable?
    let temporaryLocation: Bool
    @State private var expanded = false

    var body: some View {
        DisclosureGroup(isExpanded: $expanded) {
            VStack(alignment: .leading, spacing: PLSpace.s2) {
                Text(text)
                    .paragraphLineSpacing()
                    .fixedSize(horizontal: false, vertical: true)
                if let code {
                    CodeBlock(copyable: code, temporaryLocation: temporaryLocation)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.top, PLSpace.s2)
        } label: {
            Text(title)
                .fixedSize(horizontal: false, vertical: true)
        }
    }
}

/// Show in Finder for an AI app's config file: selects the file if it exists, else opens the
/// folder it belongs in, else beeps (checked on click). Only reveals; never creates or edits.
private struct ShowConfigInFinderButton: View {
    let pathHint: String
    @Environment(\.l10n) private var l10n

    var body: some View {
        Button(l10n("mac.actions.showInFinder")) {
            Links.revealInFinder(path: pathHint)
        }
        .buttonStyle(.bordered)
        .controlSize(.small)
    }
}

/// "Good to know": the core's notes for this client, localized from their codes (the core's
/// English, tagged, when this build has no translation).
struct ConnectNotesView: View {
    let config: McpClientConfig
    @Environment(\.l10n) private var l10n

    var body: some View {
        let notes = ConnectSetup.notes(for: config, l10n: l10n)
        if !notes.isEmpty {
            VStack(alignment: .leading, spacing: PLSpace.s2) {
                Label {
                    Text(l10n("connect.notes.title"))
                        .font(PLType.headline.font)
                } icon: {
                    Image(systemName: "info.circle")
                        .symbolRenderingMode(.hierarchical)
                        .foregroundStyle(.secondary)
                }
                .accessibilityAddTraits(.isHeader)
                ForEach(notes, id: \.self) { note in
                    HStack(alignment: .firstTextBaseline, spacing: PLSpace.s2) {
                        Text(verbatim: "•")
                            .foregroundStyle(.tertiary)
                            .accessibilityHidden(true)
                        Group {
                            if note.isEnglishFallback {
                                Text.english(note.text)
                            } else {
                                Text(note.text)
                            }
                        }
                        .paragraphLineSpacing()
                        .fixedSize(horizontal: false, vertical: true)
                    }
                }
            }
            .font(PLType.body.font)
        }
    }
}
