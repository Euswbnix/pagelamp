// Plan your study (design §5.1, §7; the Tauri app's PlanPage), a sheet on the main window from
// This Week ▸ Plan with PageLamp…: what to plan and "≈ $x"; the run with its stages and Stop;
// then the draft to use, write again or discard. Nothing is saved until Use This Plan. VoiceOver
// hears the stages and how the run ended, never the plan as it's written; a failure is read out
// and takes the focus. Closing the sheet stops a run in flight.

import SwiftUI
import PageLampKit
import PageLampModel

package struct PlanSheet: View {
    let plan: PlanModel
    /// Closed; the plan saved, if Use This Plan was chosen.
    let done: (StoredStudyPlan?) -> Void

    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n
    @Environment(\.drawsControlStandIns) private var standIns
    @AccessibilityFocusState private var focus: Focus?

    private enum Focus: Hashable {
        case title
        case stop
        case draft
        case outcome
    }

    package init(plan: PlanModel, done: @escaping (StoredStudyPlan?) -> Void) {
        self.plan = plan
        self.done = done
    }

    package var body: some View {
        VStack(alignment: .leading, spacing: PLSpace.s4) {
            VStack(alignment: .leading, spacing: PLSpace.s1) {
                Text(l10n("plan.title"))
                    .font(PLType.title2.font.weight(.semibold))
                    .accessibilityAddTraits(.isHeader)
                    .accessibilityFocused($focus, equals: .title)
                Text(l10n("plan.description"))
                    .font(PLType.callout.font)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            // Offscreen (snapshots) a scroll view draws nothing: the content at full height.
            if standIns {
                content
            } else {
                ScrollView {
                    content.padding(.trailing, PLSpace.s3)
                }
                .frame(maxHeight: Self.contentMaxHeight)
                .scrollBounceBehavior(.basedOnSize)
            }
            footer
        }
        .padding(PLLayout.sheetInset)
        .frame(width: Self.width)
        .onAppear { focus = .title }
        .onExitCommand(perform: close)
        .onChange(of: announcement) { _, text in
            if let text { AccessibilityNotification.Announcement(text).post() }
        }
        .onChange(of: phaseName) { _, phase in
            switch phase {
            case "running": focus = .stop
            case "finished": focus = .draft
            case "stopped", "failed", "discarded": focus = .outcome
            default: break
            }
        }
    }

    // MARK: - Content

    @ViewBuilder
    private var content: some View {
        VStack(alignment: .leading, spacing: PLSpace.s4) {
            switch plan.run.phase {
            case .running(let progress):
                PlanProgressView(progress: progress, text: planText)
            case .finished(let draft):
                PlanDraftView(draft: draft, plan: plan, text: planText)
                    .accessibilityFocused($focus, equals: .draft)
            case .idle, .stopped, .failed:
                outcome
                if plan.nothingToPlan {
                    Text(l10n("plan.form.nothingToPlan"))
                        .fixedSize(horizontal: false, vertical: true)
                } else {
                    PlanFormView(plan: plan)
                }
            }
            if let failure = plan.acceptFailure {
                VStack(alignment: .leading, spacing: 2) {
                    Text(l10n("plan.draft.acceptFailed"))
                        .font(PLType.body.font.weight(.semibold))
                    Text(failure.localizedDescription(in: l10n))
                        .foregroundStyle(.secondary)
                }
                .foregroundStyle(PLColor.danger)
                .accessibilityElement(children: .combine)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// How the last run ended (stopped, failed) or that the draft was discarded.
    @ViewBuilder
    private var outcome: some View {
        switch plan.run.phase {
        case .stopped:
            Text(l10n("plan.draft.stopped"))
                .accessibilityFocused($focus, equals: .outcome)
        case .failed(let failure):
            VStack(alignment: .leading, spacing: 2) {
                Text(l10n("plan.draft.failed"))
                    .font(PLType.body.font.weight(.semibold))
                Text(l10n.aiError(failure))
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .foregroundStyle(PLColor.danger)
            .accessibilityElement(children: .combine)
            .accessibilityFocused($focus, equals: .outcome)
        default:
            if plan.discarded {
                Text(l10n("plan.draft.discarded"))
                    .accessibilityFocused($focus, equals: .outcome)
            }
        }
    }

    // MARK: - Buttons

    @ViewBuilder
    private var footer: some View {
        switch plan.run.phase {
        case .running(let progress):
            HStack(alignment: .firstTextBaseline, spacing: PLSpace.s3) {
                Text(l10n("mac.plan.closeHint"))
                    .font(PLType.callout.font)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                Spacer()
                Button(l10n(progress.stopping ? "plan.running.stopping" : "plan.running.stop")) {
                    Task { await plan.stop() }
                }
                .disabled(progress.stopping)
                .accessibilityHint(l10n("mac.plan.closeHint"))
                .accessibilityFocused($focus, equals: .stop)
            }
        case .finished:
            HStack(spacing: PLSpace.s2) {
                Button(l10n("plan.draft.discard"), action: plan.discard)
                    .disabled(plan.accepting)
                Spacer()
                Button(l10n("mac.plan.writeAgain")) {
                    Task { await plan.writeAgain() }
                }
                .disabled(!plan.againEstimate.canGenerate || plan.accepting)
                .accessibilityHint(plan.againEstimate.spokenHint(l10n))
                Button(l10n("mac.plan.useThisPlan")) {
                    Task {
                        if let stored = await plan.accept() {
                            AccessibilityNotification.Announcement(l10n("plan.draft.accepted")).post()
                            done(stored)
                        }
                    }
                }
                .keyboardShortcut(.defaultAction)
                .disabled(plan.accepting)
            }
        case .idle, .stopped, .failed:
            HStack(spacing: PLSpace.s2) {
                Spacer()
                Button(l10n("common.actions.cancel"), action: close)
                    .keyboardShortcut(.cancelAction)
                if !plan.nothingToPlan {
                    Button(l10n("mac.plan.writeMyPlan")) {
                        Task { await plan.generate() }
                    }
                    .keyboardShortcut(.defaultAction)
                    .disabled(!plan.estimate.canGenerate)
                    .accessibilityHint(plan.estimate.spokenHint(l10n))
                }
            }
        }
    }

    private func close() {
        plan.close()
        done(nil)
    }

    // MARK: - What VoiceOver hears

    private var planText: PlanText {
        PlanText(l10n: l10n, calendar: model.calendar)
    }

    private var phaseName: String {
        switch plan.run.phase {
        case .idle: plan.discarded ? "discarded" : "idle"
        case .running: "running"
        case .finished: "finished"
        case .stopped: "stopped"
        case .failed: "failed"
        }
    }

    /// The stages and how the run ended, never the plan's text (design §7).
    private var announcement: String? {
        switch plan.run.phase {
        case .running(let progress): planText.stage(progress.stage)
        case .finished: l10n("plan.draft.ready")
        case .stopped: l10n("plan.draft.stopped")
        case .failed(let failure): l10n.sentences(l10n("plan.draft.failed"), l10n.aiError(failure))
        case .idle: plan.discarded ? l10n("plan.draft.discarded") : nil
        }
    }

    package static let width: CGFloat = 600
    private static let contentMaxHeight: CGFloat = 520
}

/// "Writing with OpenAI · gpt-6-luna", and the stage (VoiceOver hears it once, as an
/// announcement).
private struct PlanProgressView: View {
    let progress: GenerationRun<GeneratedStudyPlan>.Progress
    let text: PlanText

    @Environment(\.drawsControlStandIns) private var standIns

    var body: some View {
        VStack(alignment: .leading, spacing: PLSpace.s2) {
            HStack(spacing: PLSpace.s2) {
                Group {
                    // Offscreen a spinner draws nothing.
                    if standIns {
                        Image(systemName: "ellipsis.circle")
                            .foregroundStyle(.secondary)
                    } else {
                        ProgressView()
                            .controlSize(.small)
                    }
                }
                .accessibilityHidden(true)
                Text(text.headline(progress))
                    .font(PLType.body.font.weight(.semibold))
            }
            if progress.stage != nil {
                Text(text.stage(progress.stage))
                    .foregroundStyle(.secondary)
                    .accessibilityHidden(true)
            }
        }
        .frame(maxWidth: .infinity, minHeight: 80, alignment: .topLeading)
    }
}
