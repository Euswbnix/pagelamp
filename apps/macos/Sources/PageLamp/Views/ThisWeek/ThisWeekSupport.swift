// Small pieces the This Week sections share: columns as wide as their widest entry, and where
// the page's rows and buttons lead. (Row buttons, link-style actions, quiet empties and section
// errors are the shared Components.) Content layer: nothing here is glass.

import SwiftUI
import PageLampKit
import PageLampModel

// MARK: - Columns

/// A column cell as wide as the widest of `samples` (in the same font), so rows that live in
/// separate buttons still line up — without measuring in a second layout pass. The samples are
/// laid out hidden behind the content; the text never wraps or truncates.
struct ThisWeekColumnCell<Content: View>: View {
    var samples: [String]
    var alignment: Alignment = .leading
    @ViewBuilder var content: Content

    var body: some View {
        ZStack(alignment: alignment) {
            ForEach(Array(Set(samples)).sorted(), id: \.self) { sample in
                Text(verbatim: sample)
                    .lineLimit(1)
                    .fixedSize()
                    .hidden()
            }
            content
                .lineLimit(1)
                .fixedSize()
        }
        .accessibilityHidden(true)
    }
}

// MARK: - Navigation

/// Where This Week's rows and buttons lead.
@MainActor
enum ThisWeekNavigation {
    /// The course a deadline belongs to, if PageLamp knows it.
    static func course(of deadline: Deadline, in model: AppModel) -> CourseSummary? {
        guard let id = deadline.event.courseId else { return nil }
        return model.course(id: id)
    }

    /// Opens a course, optionally at a section (a deadline row opens its Deadlines).
    static func open(courseId: String, section: CourseSection? = nil, model: AppModel) {
        if let section { model.ui(for: courseId).section = section }
        model.destination = .course(courseId)
    }
}
