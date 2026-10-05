//! User-facing notices printed by the CLI (edit the wording here; the product and command
//! names come from `pagelamp_core::brand`).

use pagelamp_core::brand::{CLI_NAME, PRODUCT_NAME};

/// docs/ARCHITECTURE.md §3 rule 8: shown by `mcp-config` and on the first `… add`.
pub fn ai_disclosure() -> String {
    format!(
        "When you ask your AI app about a course, it reads that course's materials from \
         {PRODUCT_NAME} and sends them to your AI provider under your own account. \
         {PRODUCT_NAME} itself stores nothing remotely. Follow each course's AI policy; you can \
         turn sharing off per course (`{CLI_NAME} course ai-access <course> off`)."
    )
}

/// docs/ARCHITECTURE.md §3 rule 2: shown by `canvas add`.
pub fn canvas_personal_use() -> String {
    format!(
        "Canvas access tokens are for your own personal use only: never generate a token for \
         someone else's app, and don't share yours. Tokens expire — Canvas shows the maximum \
         when you create one (often 30–90 days). When sync reports that the token was \
         rejected, create a new one and run `{CLI_NAME} sources update-secret <source id>`. A \
         course folder plus the calendar feed works without any token."
    )
}

/// Where students create a token (shown right before the token prompt).
pub const CANVAS_TOKEN_HOWTO: &str = "Canvas: Account → Settings → Approved Integrations → + New \
    Access Token (fill in a purpose and an expiry; copy it right away).";

/// Shown before downloading Canvas files (leader research, 2026-09-25).
pub const CANVAS_DOWNLOAD_NOTICE: &str = "Downloading files through Canvas can count as viewing \
    them (e.g. module 'must view' requirements).";

/// Printed after the snippets (each client's notes say how to load it, e.g. quit Claude
/// Desktop first, so no generic "restart" line here).
pub fn try_prompt() -> String {
    format!("Try: \"Using {PRODUCT_NAME}, where is each of my courses this week?\"")
}

/// The automatic sync setting in words (`status`, `sync --auto`).
pub fn auto_sync(setting: pagelamp_core::auto_sync::AutoSync) -> &'static str {
    use pagelamp_core::auto_sync::AutoSync;
    match setting {
        AutoSync::Off => "off (PageLamp syncs only when you start a sync)",
        AutoSync::Daily => "once a day, while the PageLamp app is open",
        AutoSync::TwiceDaily => "twice a day, while the PageLamp app is open",
    }
}

/// The second line of a course in the sync summary: what a Canvas sync found through links,
/// what it noted as not read, and which lists the course hides. `None` when there is nothing
/// to say.
pub fn coverage_line(course: &pagelamp_core::source::CourseSyncSummary) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    let hidden = match (course.pages_hidden, course.files_hidden) {
        (true, true) => Some("Pages and Files lists hidden"),
        (true, false) => Some("Pages list hidden"),
        (false, true) => Some("Files list hidden"),
        (false, false) => None,
    };
    parts.extend(hidden.map(str::to_string));
    if course.linked_pages > 0 || course.linked_files > 0 {
        parts.push(format!(
            "found through links: {} page(s), {} file(s)",
            course.linked_pages, course.linked_files
        ));
    }
    if course.not_read > 0 {
        parts.push(format!("{} not read", course.not_read));
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

#[cfg(test)]
mod tests {
    use pagelamp_core::source::CourseSyncSummary;

    use super::coverage_line;

    #[test]
    fn the_coverage_line_says_only_what_there_is_to_say() {
        let quiet = CourseSyncSummary {
            course: "DEMO101".into(),
            pages: 3,
            files: 2,
            ..CourseSyncSummary::default()
        };
        assert_eq!(coverage_line(&quiet), None);
        let hidden = CourseSyncSummary {
            linked_pages: 2,
            linked_files: 3,
            not_read: 14,
            pages_hidden: true,
            files_hidden: true,
            ..quiet.clone()
        };
        assert_eq!(
            coverage_line(&hidden).as_deref(),
            Some(
                "Pages and Files lists hidden · found through links: 2 page(s), 3 file(s) · \
                 14 not read"
            )
        );
        let one = CourseSyncSummary {
            files_hidden: true,
            not_read: 1,
            ..quiet
        };
        assert_eq!(
            coverage_line(&one).as_deref(),
            Some("Files list hidden · 1 not read")
        );
    }
}
