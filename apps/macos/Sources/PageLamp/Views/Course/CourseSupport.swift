// Small pieces the course detail views share (spec §3.2): names of backend codes, the Course menu's focused value, and loading placeholders (S1). Content layer only:
// never glass. (Quiet states, section errors, notes and row styles are shared Components.)

import SwiftUI
import PageLampKit
import PageLampModel

/// Keys of backend codes (spec §7.3: words by code, never English from the core).
enum CourseCodes {
    static func descriptionKey(_ policy: AiPolicy) -> String {
        switch policy {
        case .unknown: "common.policy.description.unknown"
        case .prohibited: "common.policy.description.prohibited"
        case .learningAid: "common.policy.description.learning_aid"
        case .allowedWithCitation: "common.policy.description.allowed_with_citation"
        case .unrestricted: "common.policy.description.unrestricted"
        }
    }

    static func confidenceKey(_ confidence: Confidence) -> String {
        switch confidence {
        case .high: "common.confidence.high"
        case .medium: "common.confidence.medium"
        case .low: "common.confidence.low"
        }
    }

    static func confidenceSentenceKey(_ confidence: Confidence) -> String {
        switch confidence {
        case .high: "course.timeline.confidence.high"
        case .medium: "course.timeline.confidence.medium"
        case .low: "course.timeline.confidence.low"
        }
    }

    static func aiAccessNoteKey(_ state: AiMaterialsState) -> String {
        switch state {
        case .readable: "course.aiAccess.note.readable"
        case .turnedOff: "course.aiAccess.note.turned_off"
        case .withheldByPolicy: "course.aiAccess.note.withheld_by_policy"
        }
    }
}

/// What the Course menu can do for the course on screen (spec §2.7). CourseDetailView publishes
/// it with `focusedSceneValue`; `CourseCommandsMenu` reads it.
struct CourseCommands {
    /// The course website (a web address only).
    var website: URL?
    var showAIPolicy: () -> Void
    var showTermDates: () -> Void
}

extension FocusedValues {
    @Entry var courseCommands: CourseCommands?
}

/// Loading (S1): nothing for 250 ms, then redacted placeholder rows.
struct CoursePlaceholderRows: View {
    var rows = 3
    @Environment(\.l10n) private var l10n
    @State private var visible = false

    var body: some View {
        VStack(alignment: .leading, spacing: PLSpace.s4) {
            ForEach(0 ..< rows, id: \.self) { row in
                VStack(alignment: .leading, spacing: PLSpace.s1) {
                    Text(verbatim: String(repeating: "x", count: 28 + row * 7))
                        .font(PLType.body.font)
                    Text(verbatim: String(repeating: "x", count: 22))
                        .font(PLType.callout.font)
                }
            }
        }
        .redacted(reason: .placeholder)
        .opacity(visible ? 1 : 0)
        .frame(maxWidth: .infinity, alignment: .leading)
        .task {
            try? await Task.sleep(for: .milliseconds(250))
            visible = true
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(l10n("common.states.loading"))
    }
}
