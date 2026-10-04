//! What a full sync notes about a course while it reads it: the record of
//! `pagelamp_core::coverage`, the rules for reading a page again, and the limits on following
//! links.
//!
//! Links are followed one level deep: the Home page, the syllabus, the pages of modules and of
//! the Pages list, and the announcements are the texts a sync reads anyway; a page or a file
//! of this course that one of them links to is asked for. What a linked page links to is
//! noted, not asked for.

use std::collections::HashSet;

use chrono::{DateTime, TimeDelta, Utc};
use pagelamp_core::coverage::{
    CourseCoverage, CoverageArea, CoverageListState, CoverageReason, FoundLink, NotRead, PageRead,
};
use url::Url;

use crate::json::{self, CanvasId};
use crate::links::{self, Target};
use crate::transport::CanvasError;

/// The most linked pages one sync asks for in one course.
pub(crate) const MAX_LINKED_PAGES: usize = 40;
/// The most linked files one sync asks about in one course.
pub(crate) const MAX_LINKED_FILES: usize = 150;
/// The most requests one sync makes for linked pages and files, over all courses.
pub(crate) const MAX_FOLLOW_REQUESTS: u32 = 600;
/// A page no list gives a change date for (the Home page, a linked page, a module's page
/// while the Pages list is hidden) is read again by an automatic sync only after this long:
/// reading it shows in Canvas as the student viewing it. A sync the student starts reads it.
pub(crate) const REREAD_AFTER: TimeDelta = TimeDelta::hours(24);
/// A linked file PageLamp already knows of is asked about again after this long (or when
/// files are to be downloaded).
pub(crate) const RECHECK_FILE_AFTER: TimeDelta = TimeDelta::days(7);

/// The order of the record's entries: what the student or the AI app most needs to know
/// first, since only the first ones are stored and shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Rank {
    Home,
    HiddenList,
    MustView,
    Failed,
    Gone,
    Locked,
    Capped,
    Tab,
    ModuleItem,
    Link,
}

/// The record being built for one course.
pub(crate) struct Cover {
    pub record: CourseCoverage,
    /// The record the last full sync wrote.
    pub previous: Option<CourseCoverage>,
    notes: Vec<(Rank, NotRead)>,
    noted: HashSet<(CoverageArea, CoverageReason, Option<String>, Option<String>)>,
    /// Slugs of module pages the student is asked to view and hasn't: never read.
    pub guarded: HashSet<String>,
}

impl Cover {
    pub(crate) fn new(now: DateTime<Utc>, previous: Option<CourseCoverage>) -> Self {
        Cover {
            record: CourseCoverage::new(now),
            previous,
            notes: Vec::new(),
            noted: HashSet::new(),
            guarded: HashSet::new(),
        }
    }

    /// Note something that isn't read. Each thing is noted once: false when it already was.
    pub(crate) fn note(&mut self, rank: Rank, entry: NotRead) -> bool {
        let key = (
            entry.area,
            entry.reason,
            entry.title.clone(),
            entry.url.clone(),
        );
        let new = self.noted.insert(key);
        if new {
            self.notes.push((rank, entry));
        }
        new
    }

    /// A link left alone because of a limit (counted once).
    pub(crate) fn capped(&mut self, area: CoverageArea, address: &str) {
        let entry = NotRead::new(area, CoverageReason::Capped, None, Some(address));
        if self.note(Rank::Capped, entry) {
            self.record.counts.capped = self.record.counts.capped.saturating_add(1);
        }
    }

    /// The navigation: hidden lists, and the parts PageLamp doesn't read. `tabs` is `None`
    /// when Canvas didn't give them (then nothing is known to be hidden).
    pub(crate) fn tabs(&mut self, base: &Url, tabs: Option<&[json::Tab]>) {
        let Some(tabs) = tabs else {
            return;
        };
        let shown = |id: &str| tabs.iter().any(|t| t.id == id && t.hidden != Some(true));
        if !shown("pages") {
            self.record.pages_list = CoverageListState::Hidden;
            self.note(
                Rank::HiddenList,
                NotRead::new(CoverageArea::Pages, CoverageReason::IndexHidden, None, None),
            );
        }
        if !shown("files") {
            self.record.files_list = CoverageListState::Hidden;
            self.note(
                Rank::HiddenList,
                NotRead::new(CoverageArea::Files, CoverageReason::IndexHidden, None, None),
            );
        }
        for tab in tabs.iter().filter(|tab| tab.hidden != Some(true)) {
            let external =
                tab.kind.as_deref() == Some("external") || tab.id.starts_with("context_external_");
            let (area, reason) = match tab.id.as_str() {
                _ if external => (CoverageArea::ExternalTool, CoverageReason::OutsideCanvas),
                // What a sync reads (the lists above when they aren't hidden).
                "home" | "announcements" | "syllabus" | "modules" | "pages" | "files" => continue,
                "assignments" => (CoverageArea::Assignments, CoverageReason::ByRule),
                "quizzes" => (CoverageArea::Quizzes, CoverageReason::ByRule),
                "discussions" => (CoverageArea::Discussions, CoverageReason::NotRead),
                "grades" => (CoverageArea::Grades, CoverageReason::NotRead),
                "people" => (CoverageArea::People, CoverageReason::NotRead),
                _ => (CoverageArea::Other, CoverageReason::NotRead),
            };
            let address = tab
                .html_url
                .as_deref()
                .and_then(|written| base.join(written).ok())
                .map(String::from);
            self.note(
                Rank::Tab,
                NotRead::new(area, reason, tab.label.as_deref(), address.as_deref()),
            );
        }
    }

    /// A module item that isn't course material: noted with its title and address.
    pub(crate) fn module_item(&mut self, item: &json::ModuleItem) {
        let (area, reason) = match item.kind.as_deref() {
            Some("Assignment") => (CoverageArea::Assignments, CoverageReason::ByRule),
            Some("Quiz") => (CoverageArea::Quizzes, CoverageReason::ByRule),
            Some("Discussion") => (CoverageArea::Discussions, CoverageReason::NotRead),
            Some("ExternalTool") => (CoverageArea::ExternalTool, CoverageReason::OutsideCanvas),
            // A heading is structure, and the rest is read.
            Some("SubHeader" | "File" | "Page" | "ExternalUrl") | None => return,
            Some(_) => (CoverageArea::Other, CoverageReason::Other),
        };
        self.note(
            Rank::ModuleItem,
            NotRead::new(
                area,
                reason,
                item.title.as_deref(),
                item.html_url.as_deref(),
            ),
        );
    }

    /// A module's page the student is asked to view and hasn't: reading it would mark it as
    /// viewed, so no path of the sync reads it.
    pub(crate) fn guard(&mut self, slug: &str, item: &json::ModuleItem) {
        self.guarded.insert(slug.to_string());
        self.note(
            Rank::MustView,
            NotRead::new(
                CoverageArea::Pages,
                CoverageReason::WouldMarkViewed,
                item.title.as_deref(),
                item.html_url.as_deref(),
            ),
        );
    }

    /// What the last full sync recorded about the page `slug`: its material id and its read.
    pub(crate) fn earlier_page(&self, slug: &str) -> Option<(String, PageRead)> {
        self.previous
            .as_ref()?
            .followed
            .pages
            .iter()
            .find(|(_, read)| read.slug == slug)
            .map(|(id, read)| (id.clone(), read.clone()))
    }

    /// A link found in a text that isn't followed: noted by its address alone.
    pub(crate) fn link(&mut self, link: &FoundLink) {
        if let FoundLink::NotFollowed { area, reason, url } = link {
            self.note(Rank::Link, NotRead::new(*area, *reason, None, Some(url)));
        }
    }

    /// The record, with its entries in rank order (the order they were met within a rank).
    pub(crate) fn finish(mut self) -> CourseCoverage {
        self.notes.sort_by_key(|(rank, _)| *rank);
        for (_, entry) in self.notes {
            self.record.note(entry);
        }
        self.record
    }
}

/// Whether `at` is less than `window` before `now` (a time after `now` isn't trusted).
pub(crate) fn within(at: Option<DateTime<Utc>>, window: TimeDelta, now: DateTime<Utc>) -> bool {
    at.is_some_and(|at| at <= now && at > now - window)
}

/// What a body links to, each target once and in document order, and how many of its links
/// lead outside this Canvas.
pub(crate) fn scan(base: &Url, course: &CanvasId, html: &str) -> (Vec<FoundLink>, u32) {
    let mut found: Vec<FoundLink> = Vec::new();
    let mut off_site: u32 = 0;
    for link in pagelamp_extract::links::links(html).links {
        let target = match links::classify(base, course, &link) {
            Target::Page { slug } => FoundLink::Page { slug },
            Target::File { id } => FoundLink::File { id },
            Target::NotFollowed { area, reason, url } => {
                FoundLink::NotFollowed { area, reason, url }
            }
            Target::OffSite => {
                off_site = off_site.saturating_add(1);
                continue;
            }
            Target::Nothing => continue,
        };
        if !found.contains(&target) {
            found.push(target);
        }
    }
    (found, off_site)
}

/// Where the student opens the page `slug` in Canvas.
pub(crate) fn page_address(base: &Url, course: &CanvasId, slug: &str) -> String {
    let mut url = base.clone();
    url.set_query(None);
    url.set_fragment(None);
    if let Ok(mut path) = url.path_segments_mut() {
        path.clear().extend(["courses", &course.0, "pages", slug]);
    }
    url.into()
}

/// Where the student opens the course in Canvas.
pub(crate) fn course_address(base: &Url, course: &CanvasId) -> String {
    let mut url = base.clone();
    url.set_query(None);
    url.set_fragment(None);
    if let Ok(mut path) = url.path_segments_mut() {
        path.clear().extend(["courses", &course.0]);
    }
    url.into()
}

/// Where the student opens the file `id` in Canvas.
pub(crate) fn file_address(base: &Url, course: &CanvasId, id: &str) -> String {
    let mut url = base.clone();
    url.set_query(None);
    url.set_fragment(None);
    if let Ok(mut path) = url.path_segments_mut() {
        path.clear().extend(["courses", &course.0, "files", id]);
    }
    url.into()
}

/// A failed request for the Home page, a linked page or a linked file never stops the sync
/// by itself, not even a 401 Canvas doesn't explain: the item is noted as not read. What
/// would fail every request does (throttling, the network, the database, the student's Stop).
pub(crate) fn follow_failed(err: CanvasError) -> Result<CanvasError, CanvasError> {
    match err {
        CanvasError::RateLimited
        | CanvasError::Network(_)
        | CanvasError::Store(_)
        | CanvasError::Cancelled => Err(err),
        other => Ok(other),
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use serde_json::json;

    use super::*;

    fn base() -> Url {
        Url::parse("https://lms.example.edu").unwrap()
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 1, 9, 0, 0).unwrap()
    }

    #[test]
    fn a_body_gives_each_target_once_and_counts_other_sites() {
        let (found, off_site) = scan(
            &base(),
            &CanvasId("101".into()),
            r##"<p><a href="/courses/101/pages/week-1">one</a>
               <a href="/courses/101/pages/week-1#again">again</a>
               <a href="/courses/101/files/7/download?verifier=SECRET">slides</a>
               <a href="/courses/101/assignments/9">homework</a>
               <a href="https://example.org/a">a</a> <a href="https://example.org/b">b</a>
               <a href="#top">top</a></p>"##,
        );
        assert_eq!(
            found,
            [
                FoundLink::Page {
                    slug: "week-1".into()
                },
                FoundLink::File { id: "7".into() },
                FoundLink::NotFollowed {
                    area: CoverageArea::Assignments,
                    reason: CoverageReason::ByRule,
                    url: "https://lms.example.edu/courses/101/assignments/9".into()
                },
            ]
        );
        assert_eq!(off_site, 2);
    }

    #[test]
    fn the_record_puts_what_matters_first_and_notes_each_thing_once() {
        let tabs: Vec<json::Tab> = serde_json::from_value(json!([
            {"id": "home", "label": "Home", "type": "internal"},
            {"id": "modules", "label": "Modules", "type": "internal"},
            {"id": "grades", "label": "Grades", "type": "internal", "html_url": "/courses/101/grades"},
            {"id": "files", "label": "Files", "type": "internal", "hidden": true},
            {"id": "context_external_tool_7", "label": "Forum", "type": "external",
             "html_url": "/courses/101/external_tools/7"},
            {"id": "assignments", "label": "Assignments", "type": "internal"}
        ]))
        .unwrap();
        let mut cover = Cover::new(now(), None);
        cover.tabs(&base(), Some(&tabs));
        let quiz: json::ModuleItem = serde_json::from_value(json!({
            "id": 1, "type": "Quiz", "title": "Quiz 1",
            "html_url": "https://lms.example.edu/courses/101/modules/items/1"
        }))
        .unwrap();
        let heading: json::ModuleItem =
            serde_json::from_value(json!({"id": 2, "type": "SubHeader", "title": "Week 1"}))
                .unwrap();
        let must_view: json::ModuleItem = serde_json::from_value(json!({
            "id": 3, "type": "Page", "title": "Read me first", "page_url": "read-me-first",
            "html_url": "https://lms.example.edu/courses/101/modules/items/3",
            "completion_requirement": {"type": "must_view", "completed": false}
        }))
        .unwrap();
        assert!(must_view.would_mark_viewed());
        cover.module_item(&quiz);
        cover.module_item(&quiz);
        cover.module_item(&heading);
        cover.guard("read-me-first", &must_view);
        cover.capped(
            CoverageArea::Pages,
            "https://lms.example.edu/courses/101/pages/deep",
        );
        assert!(cover.guarded.contains("read-me-first"));

        let record = cover.finish();
        assert_eq!(record.pages_list, CoverageListState::Hidden, "not listed");
        assert_eq!(record.files_list, CoverageListState::Hidden);
        assert_eq!(record.counts.capped, 1);
        let entries: Vec<(CoverageArea, CoverageReason, Option<&str>)> = record
            .not_read
            .iter()
            .map(|entry| (entry.area, entry.reason, entry.title.as_deref()))
            .collect();
        use CoverageArea as A;
        use CoverageReason as R;
        assert_eq!(
            entries,
            [
                (A::Pages, R::IndexHidden, None),
                (A::Files, R::IndexHidden, None),
                (A::Pages, R::WouldMarkViewed, Some("Read me first")),
                (A::Pages, R::Capped, None),
                (A::Grades, R::NotRead, Some("Grades")),
                (A::ExternalTool, R::OutsideCanvas, Some("Forum")),
                (A::Assignments, R::ByRule, Some("Assignments")),
                (A::Quizzes, R::ByRule, Some("Quiz 1")),
            ]
        );
        assert_eq!(
            record.not_read[4].url.as_deref(),
            Some("https://lms.example.edu/courses/101/grades")
        );
        assert_eq!(record.not_read_total, 8);
    }

    #[test]
    fn a_requirement_that_is_done_or_of_another_kind_does_not_guard_a_page() {
        for (requirement, guarded) in [
            (json!({"type": "must_view", "completed": true}), false),
            (json!({"type": "must_view"}), true),
            (json!({"type": "must_mark_done", "completed": false}), false),
            (json!(null), false),
        ] {
            let item: json::ModuleItem = serde_json::from_value(json!({
                "id": 3, "type": "Page", "page_url": "p", "completion_requirement": requirement
            }))
            .unwrap();
            assert_eq!(item.would_mark_viewed(), guarded, "{requirement}");
        }
    }

    #[test]
    fn only_what_would_fail_every_request_stops_a_sync() {
        for err in [
            CanvasError::Unauthorized,
            CanvasError::Forbidden,
            CanvasError::NotFound,
            CanvasError::Http(500),
            CanvasError::BadResponse("x".into()),
        ] {
            assert_eq!(follow_failed(err.clone()), Ok(err));
        }
        for err in [
            CanvasError::RateLimited,
            CanvasError::Network("x".into()),
            CanvasError::Store("x".into()),
            CanvasError::Cancelled,
        ] {
            assert_eq!(follow_failed(err.clone()), Err(err));
        }
    }

    #[test]
    fn addresses_and_the_reread_window() {
        let course = CanvasId("101".into());
        assert_eq!(
            page_address(&base(), &course, "notes 页/x"),
            "https://lms.example.edu/courses/101/pages/notes%20%E9%A1%B5%2Fx"
        );
        assert_eq!(
            file_address(&base(), &course, "7"),
            "https://lms.example.edu/courses/101/files/7"
        );
        let day = REREAD_AFTER;
        assert!(within(Some(now() - TimeDelta::hours(23)), day, now()));
        assert!(!within(Some(now() - TimeDelta::hours(24)), day, now()));
        assert!(!within(Some(now() + TimeDelta::minutes(1)), day, now()));
        assert!(!within(None, day, now()));
    }
}
