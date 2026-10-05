//! What a full sync notes about a course while it reads it: the record of
//! `pagelamp_core::coverage`, the rules for reading a page again, and the limits on following
//! links.
//!
//! Links are followed one level deep: the Home page, the syllabus, the pages of modules and of
//! the Pages list, and the announcements are the texts a sync reads anyway; a page or a file
//! of this course that one of them links to is asked for. Of what a linked page links to, the
//! files are asked about and the pages are noted.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, TimeDelta, Utc};
use pagelamp_core::coverage::{
    CourseCoverage, CoverageArea, CoverageListState, CoverageReason, FileSeen, FoundLink, NotRead,
    PageRead,
};
use pagelamp_core::model::{Material, TextStatus};
use url::Url;

use crate::json::{self, CanvasId};
use crate::links::{self, Target};
use crate::transport::CanvasError;

/// The most linked pages one sync takes in one course: each request counts, whatever it
/// answers, and so does a page kept from a read less than `REREAD_AFTER` old.
pub(crate) const MAX_LINKED_PAGES: usize = 40;
/// The most linked files one sync takes in one course, counted the same way.
pub(crate) const MAX_LINKED_FILES: usize = 150;
/// The most requests one sync makes for linked pages and files, over all courses.
pub(crate) const MAX_FOLLOW_REQUESTS: u32 = 600;
/// A page no list gives a change date for (the Home page, a linked page, a module's page
/// while the Pages list is hidden) is read again by an automatic sync only after this long:
/// reading it may show in Canvas as the student viewing it. A sync the student starts reads
/// it.
pub(crate) const REREAD_AFTER: TimeDelta = TimeDelta::hours(24);
/// A linked file PageLamp already knows of is asked about again after this long (or when
/// files are to be downloaded).
pub(crate) const RECHECK_FILE_AFTER: TimeDelta = TimeDelta::days(7);
/// An automatic sync asks again for a link to a page Canvas said it doesn't have after this
/// long; in between the link takes no place under `MAX_LINKED_PAGES`. A sync the student
/// starts asks at once.
pub(crate) const RECHECK_DEAD_AFTER: TimeDelta = TimeDelta::days(7);

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

/// What identifies an entry (each thing is noted once).
type NoteKey = (CoverageArea, CoverageReason, Option<String>, Option<String>);

fn key_of(entry: &NotRead) -> NoteKey {
    (
        entry.area,
        entry.reason,
        entry.title.clone(),
        entry.url.clone(),
    )
}

/// The record being built for one course.
pub(crate) struct Cover {
    pub record: CourseCoverage,
    /// The record the last full sync wrote.
    pub previous: Option<CourseCoverage>,
    notes: Vec<(Rank, NotRead)>,
    noted: HashSet<NoteKey>,
    /// Slugs of module pages the student is asked to view and hasn't: never read.
    pub guarded: HashSet<String>,
    /// The same slugs in lower case (what Canvas answers with is compared without case).
    guarded_lower: HashSet<String>,
    /// The same pages by the slug form of their slug and of their title, and by their Canvas
    /// id, where the module gives one: Canvas may answer for a page under those too.
    guarded_forms: HashSet<String>,
    guarded_ids: HashSet<String>,
    /// The entry each guarded slug was noted with.
    guard_notes: HashMap<String, NoteKey>,
}

impl Cover {
    pub(crate) fn new(now: DateTime<Utc>, previous: Option<CourseCoverage>) -> Self {
        Cover {
            record: CourseCoverage::new(now),
            previous,
            notes: Vec::new(),
            noted: HashSet::new(),
            guarded: HashSet::new(),
            guarded_lower: HashSet::new(),
            guarded_forms: HashSet::new(),
            guarded_ids: HashSet::new(),
            guard_notes: HashMap::new(),
        }
    }

    /// Note something that isn't read. Each thing is noted once: false when it already was.
    pub(crate) fn note(&mut self, rank: Rank, entry: NotRead) -> bool {
        let new = self.noted.insert(key_of(&entry));
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

    /// A body with more links, or longer, than a sync looks at: the rest of its links weren't
    /// taken.
    pub(crate) fn cut_body(&mut self, area: CoverageArea, title: Option<&str>, address: &str) {
        let entry = NotRead::new(area, CoverageReason::Capped, title, Some(address));
        if self.note(Rank::Capped, entry) {
            self.record.counts.capped = self.record.counts.capped.saturating_add(1);
        }
    }

    /// A page this sync didn't ask for again: what the last one noted about it still holds.
    /// `id` is its material id and `stored` the material as the last sync left it.
    pub(crate) fn carried_page(
        &mut self,
        id: &str,
        read: &PageRead,
        stored: Option<&Material>,
        address: &str,
    ) {
        let title = stored.map(|old| old.title.as_str());
        if read.locked {
            self.note(
                Rank::Locked,
                NotRead::new(
                    CoverageArea::Pages,
                    CoverageReason::Locked,
                    title,
                    Some(address),
                ),
            );
            if stored.is_some_and(|old| old.text_status != TextStatus::Ok) {
                self.record
                    .unread
                    .insert(id.to_string(), CoverageReason::Locked);
            }
        }
        if read.cut {
            self.cut_body(CoverageArea::Pages, title, address);
        }
    }

    /// A linked file as the last check found it: one Canvas no longer has is noted (the
    /// material stays).
    pub(crate) fn carried_file(&mut self, seen: &FileSeen, title: Option<&str>, address: &str) {
        if seen.gone {
            self.note(
                Rank::Gone,
                NotRead::new(
                    CoverageArea::Files,
                    CoverageReason::NoLongerInCanvas,
                    title,
                    Some(address),
                ),
            );
        }
    }

    /// The navigation: hidden lists, and the parts PageLamp doesn't read. `tabs` is `None`
    /// when Canvas didn't give them: then no list is asked for, and both count as failed.
    pub(crate) fn tabs(&mut self, base: &Url, tabs: Option<&[json::Tab]>) {
        let Some(tabs) = tabs else {
            // Which lists the course hides isn't known, so neither is asked for this time.
            self.list_failed(CoverageArea::Pages);
            self.list_failed(CoverageArea::Files);
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

    /// A list (modules, Pages, Files) that wasn't read, or not completely, in this sync: its
    /// state says so, and so does an entry, since only entries are shown.
    pub(crate) fn list_failed(&mut self, area: CoverageArea) {
        match area {
            CoverageArea::Modules => self.record.modules_list = CoverageListState::Failed,
            CoverageArea::Pages => self.record.pages_list = CoverageListState::Failed,
            CoverageArea::Files => self.record.files_list = CoverageListState::Failed,
            _ => {}
        }
        self.note(
            Rank::Failed,
            NotRead::new(area, CoverageReason::FailedThisSync, None, None),
        );
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

    /// A module's page the student is asked to view and hasn't: reading it could mark it as
    /// viewed, so no path of the sync reads it.
    pub(crate) fn guard(&mut self, slug: &str, item: &json::ModuleItem) {
        self.guarded.insert(slug.to_string());
        self.guarded_lower.insert(slug.to_lowercase());
        // (Never an empty form: a title of symbols alone has none, and would match every
        // link.)
        let forms = [
            slug.to_lowercase(),
            slug_form(slug),
            item.title.as_deref().map(slug_form).unwrap_or_default(),
        ];
        self.guarded_forms
            .extend(forms.into_iter().filter(|form| !form.is_empty()));
        if let Some(id) = &item.content_id {
            self.guarded_ids.insert(id.0.clone());
        }
        let entry = NotRead::new(
            CoverageArea::Pages,
            CoverageReason::WouldMarkViewed,
            item.title.as_deref(),
            item.html_url.as_deref(),
        );
        self.guard_notes.insert(slug.to_string(), key_of(&entry));
        self.note(Rank::MustView, entry);
    }

    /// Whether a link's `slug` could lead to a guarded page under another address. Canvas
    /// may answer `pages/:url_or_id` for a page's id, for the slug form of its title and for
    /// a slug it had before it was renamed, and a renamed page's slug often goes on from the
    /// old one with a hyphen (or the other way round): "week-1" and "week-1-intro", not
    /// "week-1" and "week-10". The link's slug is compared as written and in slug form
    /// ("Read me first", "read_me_first"). Only asked while the Pages list isn't there to
    /// say which slugs exist. Errs towards not reading.
    pub(crate) fn could_be_guarded(&self, slug: &str) -> bool {
        if self.guarded.is_empty() {
            return false;
        }
        let lower = slug.to_lowercase();
        if lower.starts_with("page_id:") || lower.bytes().all(|b| b.is_ascii_digit()) {
            return true;
        }
        // One slug goes on from the other at a word boundary.
        let goes_on = |longer: &str, shorter: &str| {
            longer
                .strip_prefix(shorter)
                .is_some_and(|rest| rest.starts_with('-'))
        };
        [lower.clone(), slug_form(&lower)]
            .iter()
            .filter(|written| !written.is_empty())
            .any(|written| {
                self.guarded_forms.iter().any(|guarded| {
                    guarded == written || goes_on(guarded, written) || goes_on(written, guarded)
                })
            })
    }

    /// Whether a page Canvas answered with is a guarded one: by its own slug (compared
    /// without case) or its id. Not by its title: the answer's `url` is the page's own slug,
    /// and two pages can share a title.
    pub(crate) fn is_guarded_page(&self, url: Option<&str>, page_id: Option<&CanvasId>) -> bool {
        url.is_some_and(|url| self.guarded_lower.contains(&url.to_lowercase()))
            || page_id.is_some_and(|id| self.guarded_ids.contains(&id.0))
    }

    /// The Home page was read although a module asks the student to view it (its slug can't
    /// be known before the first read): it is no longer "not read".
    pub(crate) fn unguard_note(&mut self, slug: &str) {
        if let Some(key) = self.guard_notes.remove(slug) {
            self.noted.remove(&key);
            self.notes.retain(|(_, entry)| key_of(entry) != key);
        }
    }

    /// What the last full sync recorded about the page `slug` leads to (its own slug, or
    /// another address of it): the page's material id and its read. When more than one page
    /// answers to it (a slug one page had and another page has now), the page whose own slug
    /// it is comes first, then the one read last.
    pub(crate) fn earlier_page(&self, slug: &str) -> Option<(String, PageRead)> {
        self.previous
            .as_ref()?
            .followed
            .pages
            .iter()
            .filter(|(_, read)| read.answers_to(slug))
            .max_by_key(|(_, read)| (read.slug == slug, read.read_at))
            .map(|(id, read)| (id.clone(), read.clone()))
    }

    /// How many entries so far say that something went wrong or that the student can do
    /// something about it (the sync summary's "not read"): not what PageLamp never reads by
    /// rule, and not the hidden lists, which the summary names by themselves.
    pub(crate) fn needs_attention(&self) -> u32 {
        let count = self
            .notes
            .iter()
            .filter(|(_, entry)| {
                matches!(
                    entry.reason,
                    CoverageReason::WouldMarkViewed
                        | CoverageReason::FailedThisSync
                        | CoverageReason::Capped
                        | CoverageReason::Locked
                        | CoverageReason::NoLongerInCanvas
                )
            })
            .count();
        u32::try_from(count).unwrap_or(u32::MAX)
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

/// The slug Canvas makes of a title: lowercase, every run of other characters one hyphen.
pub(crate) fn slug_form(title: &str) -> String {
    let mut slug = String::with_capacity(title.len());
    for c in title.to_lowercase().chars() {
        if c.is_alphanumeric() {
            slug.push(c);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_matches('-').to_string()
}

/// The id of the discussion topic an address of this Canvas opens
/// (`…/discussion_topics/<id>`): an announcement is one.
pub(crate) fn discussion_topic_id(address: &str) -> Option<&str> {
    let (_, rest) = address.split_once("/discussion_topics/")?;
    let id = rest.split(['/', '?', '#']).next()?;
    (!id.is_empty() && id.bytes().all(|b| b.is_ascii_digit() || b == b'~')).then_some(id)
}

/// What one body links to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Scanned {
    /// Each target once, in document order.
    pub links: Vec<FoundLink>,
    /// How many links lead outside this Canvas.
    pub off_site: u32,
    /// The body was longer, or held more links, than is looked at.
    pub cut: bool,
}

/// What a body links to.
pub(crate) fn scan(base: &Url, course: &CanvasId, html: &str) -> Scanned {
    let mut found: Vec<FoundLink> = Vec::new();
    let mut off_site: u32 = 0;
    let body = pagelamp_extract::links::links(html);
    let cut = body.cut;
    for link in body.links {
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
    Scanned {
        links: found,
        off_site,
        cut,
    }
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

/// Where the student opens the course's syllabus in Canvas.
pub(crate) fn syllabus_address(base: &Url, course: &CanvasId) -> String {
    let mut url = base.clone();
    url.set_query(None);
    url.set_fragment(None);
    if let Ok(mut path) = url.path_segments_mut() {
        path.clear()
            .extend(["courses", &course.0, "assignments", "syllabus"]);
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
        let Scanned {
            links: found,
            off_site,
            cut,
        } = scan(
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
        assert!(!cut);
        // A body with more links than are looked at says so.
        let many: String = (0..pagelamp_extract::links::MAX_LINKS + 1)
            .map(|n| format!(r#"<a href="/courses/101/pages/p{n}">p</a>"#))
            .collect();
        let scanned = scan(&base(), &CanvasId("101".into()), &many);
        assert!(scanned.cut);
        assert_eq!(scanned.links.len(), pagelamp_extract::links::MAX_LINKS);
    }

    fn must_view(slug: &str, title: &str, content_id: Option<u64>) -> json::ModuleItem {
        serde_json::from_value(json!({
            "id": 3, "type": "Page", "title": title, "page_url": slug, "content_id": content_id,
            "html_url": "https://lms.example.edu/courses/101/modules/items/3",
            "completion_requirement": {"type": "must_view", "completed": false}
        }))
        .unwrap()
    }

    #[test]
    fn a_link_that_could_reach_a_guarded_page_under_another_address_is_recognised() {
        let mut cover = Cover::new(now(), None);
        // Nothing is guarded: nothing could be.
        assert!(!cover.could_be_guarded("601"));
        cover.guard(
            "read-me-first-2",
            &must_view("read-me-first-2", "Read Me: First (updated)", Some(602)),
        );
        for slug in [
            // By id, in both forms Canvas takes.
            "602",
            "17",
            "page_id:602",
            // The slug itself in another letter case, and the slug form of its title.
            "Read-Me-First-2",
            "read-me-first-updated",
            // Written with spaces or underscores: the slug form is compared too.
            "Read me first (updated)",
            "read_me_first_2",
            // A slug it had before, or got after: one goes on from the other with a hyphen.
            "read-me-first",
            "read-me",
            "read-me-first-2-old",
        ] {
            assert!(cover.could_be_guarded(slug), "{slug}");
        }
        // Not at a word boundary: another page. (With "week-1" guarded, "week-10" is free.)
        for slug in [
            "week-1",
            "notes",
            "first",
            "readme",
            "read-me-first-20",
            "re",
        ] {
            assert!(!cover.could_be_guarded(slug), "{slug}");
        }
        // What Canvas answered with: by its own slug (in any letter case) or its id. Not by
        // its title: another page can have the same one.
        let id = |n: &str| CanvasId(n.into());
        assert!(cover.is_guarded_page(Some("read-me-first-2"), None));
        assert!(cover.is_guarded_page(Some("Read-Me-First-2"), Some(&id("9"))));
        assert!(cover.is_guarded_page(Some("renamed"), Some(&id("602"))));
        assert!(!cover.is_guarded_page(Some("read-me-first-updated"), Some(&id("9"))));
        assert!(!cover.is_guarded_page(Some("week-1"), Some(&id("601"))));

        // A title of symbols alone gives no form to compare: it must not match every link.
        let mut cover = Cover::new(now(), None);
        cover.guard("x1", &must_view("x1", "★★★", None));
        for slug in ["notes", "week-1", "a", "-notes"] {
            assert!(!cover.could_be_guarded(slug), "{slug}");
        }
        assert!(cover.could_be_guarded("x1-old"));
        // With "week-1" guarded, the other weeks are read.
        let mut cover = Cover::new(now(), None);
        cover.guard("week-1", &must_view("week-1", "Week 1", None));
        for slug in ["week-10", "week-11", "week-12", "week-2"] {
            assert!(!cover.could_be_guarded(slug), "{slug}");
        }
        for slug in ["week-1-intro", "week", "Week_1"] {
            assert!(cover.could_be_guarded(slug), "{slug}");
        }
        assert!(!cover.is_guarded_page(None, None));
        assert_eq!(
            slug_form("  Read Me: First (updated) "),
            "read-me-first-updated"
        );
        assert_eq!(slug_form("第1周 讲义"), "第1周-讲义");
    }

    #[test]
    fn the_page_a_slug_names_now_comes_before_one_it_named_earlier() {
        let read = |slug: &str, also: &[&str], hours_ago: i64| PageRead {
            slug: slug.into(),
            also: also.iter().map(|s| s.to_string()).collect(),
            read_at: Some(now() - TimeDelta::hours(hours_ago)),
            ..PageRead::default()
        };
        let mut previous = CourseCoverage::new(now());
        // "week-1" was page 1's slug once, is another address of page 2, and is page 3's own.
        previous.followed.pages = [
            ("s/page/1".to_string(), read("week-1", &[], 50)),
            ("s/page/2".to_string(), read("old", &["week-1"], 1)),
            ("s/page/3".to_string(), read("week-1", &[], 20)),
        ]
        .into();
        let cover = Cover::new(now(), Some(previous));
        // Its own slug first, and of those the page read last.
        assert_eq!(cover.earlier_page("week-1").unwrap().0, "s/page/3");
        assert_eq!(cover.earlier_page("old").unwrap().0, "s/page/2");
        assert_eq!(cover.earlier_page("nothing"), None);
    }

    #[test]
    fn the_home_page_that_was_read_is_no_longer_noted_as_not_read() {
        let mut cover = Cover::new(now(), None);
        cover.guard("welcome", &must_view("welcome", "Welcome", None));
        cover.guard("other", &must_view("other", "Other", None));
        assert_eq!(cover.needs_attention(), 2);
        cover.unguard_note("welcome");
        cover.unguard_note("never-guarded");
        assert_eq!(cover.needs_attention(), 1);
        // Still guarded for every other path of the sync.
        assert!(cover.guarded.contains("welcome"));
        let record = cover.finish();
        assert_eq!(
            record
                .not_read
                .iter()
                .map(|entry| entry.title.as_deref())
                .collect::<Vec<_>>(),
            [Some("Other")]
        );
    }

    #[test]
    fn the_summary_counts_what_went_wrong_or_the_student_can_act_on() {
        let mut cover = Cover::new(now(), None);
        // Not known which lists the course hides: neither was asked for.
        cover.tabs(&base(), None);
        assert_eq!(cover.record.pages_list, CoverageListState::Failed);
        assert_eq!(cover.record.files_list, CoverageListState::Failed);
        let note = |cover: &mut Cover, area, reason, title: &str| {
            cover.note(Rank::Link, NotRead::new(area, reason, Some(title), None));
        };
        use CoverageArea as A;
        use CoverageReason as R;
        for (area, reason) in [
            (A::Pages, R::Locked),
            (A::Files, R::NoLongerInCanvas),
            (A::Pages, R::Capped),
            // What PageLamp never reads by rule doesn't count.
            (A::Assignments, R::ByRule),
            (A::Grades, R::NotRead),
            (A::ExternalTool, R::OutsideCanvas),
            (A::Other, R::OtherCourse),
            (A::Pages, R::IndexHidden),
        ] {
            note(&mut cover, area, reason, "x");
        }
        // The two lists that weren't asked for, and the three above.
        assert_eq!(cover.needs_attention(), 5);
    }

    #[test]
    fn an_announcement_is_known_by_its_topic_id() {
        for (address, id) in [
            (
                "https://lms.example.edu/courses/101/discussion_topics/701",
                Some("701"),
            ),
            (
                "https://lms.example.edu/courses/101/discussion_topics/1~701/",
                Some("1~701"),
            ),
            (
                "https://lms.example.edu/courses/101/discussion_topics/701?module_item_id=3",
                Some("701"),
            ),
            (
                "https://lms.example.edu/courses/101/discussion_topics",
                None,
            ),
            (
                "https://lms.example.edu/courses/101/discussion_topics/new",
                None,
            ),
            ("https://lms.example.edu/courses/101/pages/701", None),
        ] {
            assert_eq!(discussion_topic_id(address), id, "{address}");
        }
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
