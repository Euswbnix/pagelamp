// Your study plan (spec §3.1): the latest plan the student's AI app saved over MCP
// (`save_study_plan`), read-only. Today and the next two days up front, the rest behind Show Full
// Plan; static done / not-done glyphs; the AI app's notes in a callout; a stale warning; and
// "Read-only here. To change the plan, ask your AI app." S12: no plan yet → the prompt to ask
// for one. S14: the plan failed to load.

import SwiftUI
import PageLampKit
import PageLampModel

struct StudyPlanSection: View {
    let text: ThisWeekText

    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n
    @State private var expanded: Bool

    init(text: ThisWeekText, expanded: Bool = false) {
        self.text = text
        _expanded = State(initialValue: expanded)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: PLLayout.titleToRule) {
            SectionHeader(title: l10n("courses.plan.title"))
            if let failure = model.sectionErrors[.studyPlan] {
                SectionError(title: l10n("courses.plan.errorTitle"), message: failure.localizedDescription(in: l10n)) {
                    Task { await model.refresh() }
                }
            } else if let stored = model.studyPlan {
                plan(stored)
            } else {
                StudyPlanEmpty()
            }
        }
    }

    @ViewBuilder private func plan(_ stored: StoredStudyPlan) -> some View {
        let digest = StudyPlanDigest(stored, now: text.now, calendar: model.calendar)
        let courses = model.courses
        let columns = PlanColumns(
            courses: stored.plan.items.map { ThisWeekContents.planCourseLabel($0.courseId, courses: courses) ?? "" },
            minutes: stored.plan.items.map { $0.minutes.map(text.planMinutes) ?? "" }
        )
        VStack(alignment: .leading, spacing: PLSpace.s4) {
            VStack(alignment: .leading, spacing: PLSpace.s1) {
                Text(text.planMeta(stored))
                    .font(PLType.callout.font)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                if digest.isStale {
                    Label {
                        Text(l10n("courses.plan.stale"))
                            .fixedSize(horizontal: false, vertical: true)
                    } icon: {
                        Image(systemName: "clock")
                            .foregroundStyle(PLColor.warning)
                    }
                    .font(PLType.callout.font)
                }
            }

            if let notes = stored.plan.notes, !notes.isEmpty {
                Callout(tone: .info, symbol: "list.bullet.clipboard", title: l10n("courses.plan.notesLabel"), message: notes)
            }

            if digest.days.isEmpty {
                Text(l10n("courses.plan.noItems"))
                    .foregroundStyle(.secondary)
            } else if digest.focus.isEmpty {
                Text(l10n("courses.plan.nothingSoon"))
                    .foregroundStyle(.secondary)
            } else {
                PlanDays(days: digest.focus, text: text, columns: columns)
            }

            if !digest.others.isEmpty {
                DisclosureGroup(isExpanded: $expanded) {
                    PlanDays(days: digest.others, text: text, columns: columns)
                        .padding(.top, PLSpace.s2)
                } label: {
                    Text(l10n(expanded ? "mac.actions.showLess" : "mac.actions.showFullPlan"))
                }
            }

            Text(l10n("courses.plan.readOnly"))
                .font(PLType.callout.font)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
        }
        .font(PLType.body.font)
    }

    /// Every task's course and duration text, so those columns line up across days.
    struct PlanColumns {
        var courses: [String]
        var minutes: [String]
    }
}

/// Plan days, each with its heading ("Today · Fri, Sep 25") and tasks.
private struct PlanDays: View {
    let days: [StudyPlanDigest.Day]
    let text: ThisWeekText
    let columns: StudyPlanSection.PlanColumns

    @Environment(AppModel.self) private var model

    var body: some View {
        VStack(alignment: .leading, spacing: PLSpace.s4) {
            ForEach(days) { day in
                VStack(alignment: .leading, spacing: PLSpace.s2) {
                    HStack(alignment: .firstTextBaseline, spacing: PLSpace.s2) {
                        if let date = day.day {
                            Text(text.dayTitle(date))
                                .font(PLType.body.font.weight(.semibold))
                            Text(text.dayDate(date))
                                .font(PLType.callout.font)
                                .monospacedDigit()
                                .foregroundStyle(.secondary)
                        } else {
                            Text(verbatim: day.date)
                                .font(PLType.body.font.weight(.semibold))
                        }
                    }
                    .accessibilityElement(children: .combine)
                    .accessibilityAddTraits(.isHeader)
                    ForEach(day.entries) { entry in
                        PlanItemRow(
                            item: entry.item,
                            course: ThisWeekContents.planCourseLabel(entry.item.courseId, courses: model.courses),
                            text: text,
                            columns: columns
                        )
                    }
                }
            }
        }
    }
}

/// One task: a static glyph (read-only), the title (and description), course, duration.
private struct PlanItemRow: View {
    let item: StudyPlanItem
    let course: String?
    let text: ThisWeekText
    let columns: StudyPlanSection.PlanColumns

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: PLSpace.s3) {
            Image(systemName: item.done ? "checkmark.circle.fill" : "circle")
                .symbolRenderingMode(.hierarchical)
                .foregroundStyle(item.done ? AnyShapeStyle(PLColor.success) : AnyShapeStyle(.secondary))
            VStack(alignment: .leading, spacing: 2) {
                Text(item.title)
                    .foregroundStyle(item.done ? .secondary : .primary)
                    .fixedSize(horizontal: false, vertical: true)
                if let description = item.description, !description.isEmpty {
                    Text(description)
                        .font(PLType.callout.font)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            ThisWeekColumnCell(samples: columns.courses) {
                Text(course ?? "")
            }
            .font(PLType.subheadline.font.weight(.semibold))
            .foregroundStyle(.secondary)
            ThisWeekColumnCell(samples: columns.minutes, alignment: .trailing) {
                Text(item.minutes.map(text.planMinutes) ?? "")
            }
            .font(PLType.callout.font)
            .monospacedDigit()
            .foregroundStyle(.secondary)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(text.spokenPlanItem(item, course: course))
    }
}

/// S12: no plan yet. What to ask the AI app, with Copy the Prompt and a way to connect one.
private struct StudyPlanEmpty: View {
    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n

    var body: some View {
        let prompt = l10n("courses.plan.prompt")
        QuietState(
            symbol: "list.bullet.clipboard",
            title: l10n("courses.plan.emptyTitle"),
            message: l10n("courses.plan.emptyDescription", ["prompt": prompt])
        ) {
            CopyButton(title: l10n("mac.actions.copyThePrompt"), text: prompt)
            Button(l10n("mac.nav.connect")) {
                model.destination = .connect
            }
            .linkButtonStyle()
        }
    }
}
