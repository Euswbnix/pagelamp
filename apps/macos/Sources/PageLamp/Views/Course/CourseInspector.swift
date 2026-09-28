// The course inspector (spec §2.6, §3.2 "Inspector"; M1 read-only, M2 editable). The system
// draws the inspector column's glass; its contents are a grouped Form of LabeledContent values,
// never glass. "AI Policy…" and "Set Term Dates…" open it scrolled to their section.

import SwiftUI
import PageLampKit
import PageLampModel

struct CourseInspector: View {
    let summary: CourseSummary
    let detail: CourseDetailModel

    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        ScrollViewReader { proxy in
            CourseInspectorForm(summary: summary)
                .onChange(of: detail.inspectorRequest, initial: true) { _, request in
                    guard let request else { return }
                    withAnimation(reduceMotion ? nil : PLMotion.quick) {
                        proxy.scrollTo(request.section, anchor: .top)
                    }
                }
        }
    }
}

/// The inspector's sections: AI Policy, Course Materials, Term Dates, Course.
package struct CourseInspectorForm: View {
    package enum Layout {
        /// The inspector: a grouped Form.
        case form
        /// The same sections in a plain stack (snapshots: ImageRenderer can't draw a Form).
        case stack
    }

    let summary: CourseSummary
    var layout: Layout = .form

    @Environment(AppModel.self) private var model
    @Environment(\.l10n) private var l10n
    @Environment(\.openURL) private var openURL

    package init(summary: CourseSummary, layout: Layout = .form) {
        self.summary = summary
        self.layout = layout
    }

    private var course: Course { summary.course }

    package var body: some View {
        switch layout {
        case .form:
            Form { sections }
                .formStyle(.grouped)
        case .stack:
            VStack(alignment: .leading, spacing: PLSpace.s6) { sections }
                .labeledContentStyle(TrailingValueStyle())
                .padding(PLSpace.s5)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    /// Label leading, value trailing, like a grouped Form row (snapshot stack only).
    private struct TrailingValueStyle: LabeledContentStyle {
        func makeBody(configuration: Configuration) -> some View {
            HStack(alignment: .firstTextBaseline, spacing: PLSpace.s3) {
                configuration.label
                Spacer(minLength: PLSpace.s2)
                configuration.content
            }
        }
    }

    @ViewBuilder private var sections: some View {
        aiPolicy
        courseMaterials
        termDates
        courseSection
    }

    // MARK: AI Policy

    private var aiPolicy: some View {
        Section {
            // "How can you use AI in this course?" — answered with the policy (never coloured).
            LabeledContent(l10n("course.policy.title")) {
                Label(l10n.policy(course.aiPolicy), systemImage: course.aiPolicy.symbol)
                    .foregroundStyle(.primary)
            }
            Text(l10n(CourseCodes.descriptionKey(course.aiPolicy)))
                .foregroundStyle(.secondary)
                .paragraphLineSpacing()
                .fixedSize(horizontal: false, vertical: true)
            if let note = course.aiPolicyNote, !note.isEmpty {
                VStack(alignment: .leading, spacing: PLSpace.s1) {
                    Text(l10n("mac.course.policy.syllabusRule"))
                        .foregroundStyle(.secondary)
                    // The student's own words, in whatever language they pasted.
                    Text(note)
                        .textSelection(.enabled)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        } header: {
            Text(l10n("mac.inspector.aiPolicy"))
        } footer: {
            Text(l10n("course.policy.explanation", [
                "prohibited": l10n.policy(.prohibited),
                "unknown": l10n.policy(.unknown),
            ]))
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
        }
        .id(CourseInspectorSection.aiPolicy)
    }

    // MARK: Course Materials

    private var courseMaterials: some View {
        // No AI shows off whatever `ai_access` holds (rule 8 keeps the stored value).
        let readable = summary.aiMaterials == .readable
        return Section {
            LabeledContent(l10n("course.aiAccess.label")) {
                Text(readable ? l10n("mac.course.value.on") : l10n("mac.course.value.off"))
            }
            Text(l10n(CourseCodes.aiAccessNoteKey(summary.aiMaterials)))
                .foregroundStyle(.secondary)
                .paragraphLineSpacing()
                .fixedSize(horizontal: false, vertical: true)
        } header: {
            Text(l10n("mac.inspector.courseMaterials"))
        }
        .id(CourseInspectorSection.courseMaterials)
    }

    // MARK: Term Dates

    private var termDates: some View {
        let first = CourseDates.term(course.termStart, calendar: model.calendar, l10n: l10n)
        let last = CourseDates.term(course.termEnd, calendar: model.calendar, l10n: l10n)
        return Section {
            Text(l10n("course.term.description"))
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            LabeledContent(l10n("course.term.start")) {
                Text(first ?? l10n("mac.course.term.noDate")).monospacedDigit()
            }
            LabeledContent(l10n("mac.course.term.lastDay")) {
                Text(last ?? l10n("mac.course.term.noDate")).monospacedDigit()
            }
            Text(termSource)
                .foregroundStyle(.secondary)
        } header: {
            Text(l10n("mac.inspector.termDates"))
        } footer: {
            Text(l10n("course.term.breaksNote"))
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
        }
        .id(CourseInspectorSection.termDates)
    }

    /// "Set by you" / "From Canvas" / "No dates yet".
    private var termSource: String {
        switch course.termSource {
        case .user:
            return l10n("mac.course.term.source.user")
        case .synced:
            // A course folder's term start is the student's own folder setting; Canvas sends its own.
            switch model.sources.first(where: { $0.id == course.sourceId })?.kind {
            case .folder:
                return l10n("mac.course.term.sourceFolder")
            case .canvas:
                return l10n("mac.course.term.source.synced", ["source": l10n.sourceKind(.canvas)])
            case .ical, nil:
                return l10n("mac.course.term.source.synced", ["source": summary.sourceLabel])
            }
        case .none:
            return l10n("mac.course.term.source.none")
        }
    }

    // MARK: Course

    private var courseSection: some View {
        Section {
            Text(freshness)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            VStack(alignment: .leading, spacing: PLSpace.s1) {
                LabeledContent(l10n("course.settings.hideLabel")) {
                    Text(course.hidden ? l10n("mac.course.value.on") : l10n("mac.course.value.off"))
                }
                Text(l10n("course.settings.hideDescription"))
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            if !course.enrollmentActive {
                VStack(alignment: .leading, spacing: PLSpace.s1) {
                    Label(l10n("common.pastCourse.badge"), systemImage: "clock.arrow.circlepath")
                    Text(l10n("common.pastCourse.hint"))
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            if let url = Links.web(course.url) {
                Button(l10n("mac.actions.openCourseWebsite")) { openURL(url) }
            }
        } header: {
            Text(l10n("mac.inspector.course"))
        }
        .id(CourseInspectorSection.course)
    }

    /// "Data from Demo Canvas · synced 2 hr. ago".
    private var freshness: String {
        guard let synced = summary.lastSyncedAt else {
            return l10n("course.header.freshnessNever", ["source": summary.sourceLabel])
        }
        return l10n("course.header.freshness", [
            "source": summary.sourceLabel,
            "when": l10n.relative(synced, to: model.clock(), calendar: model.calendar),
        ])
    }
}
