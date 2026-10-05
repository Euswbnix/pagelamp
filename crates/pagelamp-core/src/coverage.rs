//! What a sync read of a Canvas course, and what it didn't.
//!
//! PageLamp reads the parts of a course its rules allow and Canvas shows the student. Where
//! it reads nothing, it says so: to the student (the sync row, the CLI) and to the AI app
//! (`course_overview`), which must not guess at what PageLamp didn't read.
//!
//! One record per course lives in the `settings` table:
//!
//! | key                           | value            |
//! |-------------------------------|------------------|
//! | `canvas.coverage.<course id>` | `CourseCoverage` |
//!
//! - A full Canvas sync writes it in the transaction that writes the course's materials. A
//!   sync limited to some courses rewrites only theirs. A sync nobody is at the app for (which
//!   asks Canvas for nothing with a course in its path) never touches it.
//! - It goes with the course: `remove_for_source` when its source is removed.
//! - It holds structure only: ids, reason codes, addresses, and the titles Canvas gives its
//!   own items (a tab, a module item, a page). Never text from a body, and never the words of
//!   a link found in one. So the views give it out even for a course whose text is withheld.
//! - It is never logged and never part of the diagnostic report (titles and addresses name
//!   the student's courses).
//!
//! `CoverageView` is what the views make of it (`views::course_overview`).

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Result;
use crate::model::{Course, DownloadBlock, Material, MaterialKind, TextStatus, Timestamp};
use crate::store::Store;

/// The version of the record's shape. A record a later version wrote reads as absent.
pub const VERSION: u32 = 1;
/// The records' keys in the `settings` table start with this; the course id follows.
pub const KEY_PREFIX: &str = "canvas.coverage.";
/// The most `NotRead` entries a record stores (`not_read_total` says how many there were).
pub const MAX_STORED: usize = 100;
/// The most entries a view gives (`CoverageView::not_readable_more` counts the rest).
pub const MAX_SHOWN: usize = 20;
/// The longest title an entry keeps, in characters.
pub const MAX_TITLE_CHARS: usize = 120;

/// The record's key for `course_id`.
pub fn key(course_id: &str) -> String {
    format!("{KEY_PREFIX}{course_id}")
}

/// Why PageLamp didn't read something. A code this version doesn't know reads as `Other`.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum CoverageReason {
    /// The course's navigation hides the list it would be in (Pages, Files). PageLamp never
    /// asks for a hidden list; it reads what modules and links lead to.
    IndexHidden,
    /// A file PageLamp knows of and hasn't got the text of: the student can download it in
    /// PageLamp.
    NeedsDownload,
    /// A file larger than the download limit.
    TooLarge,
    /// Canvas locks it for this student (not released yet, or closed).
    Locked,
    /// A page whose module asks the student to view it: reading it could mark it as viewed.
    /// It is read once the student has opened it in Canvas.
    WouldMarkViewed,
    /// Assignments and quizzes: PageLamp keeps the title, the due date and the link only.
    ByRule,
    /// It isn't in Canvas: an external tool, or another site.
    OutsideCanvas,
    /// A part of Canvas PageLamp doesn't read: grades, people, discussions, the activity
    /// stream.
    NotRead,
    /// It belongs to another course.
    OtherCourse,
    /// Left out by a limit of this sync (how many links are followed, and how far).
    Capped,
    /// The request for it failed in this sync; the next sync asks again.
    FailedThisSync,
    /// A file PageLamp stored earlier that Canvas no longer has.
    NoLongerInCanvas,
    #[serde(other)]
    Other,
}

impl CoverageReason {
    /// Every reason, in the order they are declared.
    pub const ALL: [CoverageReason; 13] = [
        CoverageReason::IndexHidden,
        CoverageReason::NeedsDownload,
        CoverageReason::TooLarge,
        CoverageReason::Locked,
        CoverageReason::WouldMarkViewed,
        CoverageReason::ByRule,
        CoverageReason::OutsideCanvas,
        CoverageReason::NotRead,
        CoverageReason::OtherCourse,
        CoverageReason::Capped,
        CoverageReason::FailedThisSync,
        CoverageReason::NoLongerInCanvas,
        CoverageReason::Other,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            CoverageReason::IndexHidden => "index_hidden",
            CoverageReason::NeedsDownload => "needs_download",
            CoverageReason::TooLarge => "too_large",
            CoverageReason::Locked => "locked",
            CoverageReason::WouldMarkViewed => "would_mark_viewed",
            CoverageReason::ByRule => "by_rule",
            CoverageReason::OutsideCanvas => "outside_canvas",
            CoverageReason::NotRead => "not_read",
            CoverageReason::OtherCourse => "other_course",
            CoverageReason::Capped => "capped",
            CoverageReason::FailedThisSync => "failed_this_sync",
            CoverageReason::NoLongerInCanvas => "no_longer_in_canvas",
            CoverageReason::Other => "other",
        }
    }
}

/// The part of a course an entry is about. One this version doesn't know reads as `Other`.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum CoverageArea {
    Home,
    Syllabus,
    Modules,
    Pages,
    Files,
    Announcements,
    Assignments,
    Quizzes,
    Discussions,
    Grades,
    People,
    ExternalTool,
    #[serde(other)]
    Other,
}

impl CoverageArea {
    pub fn as_str(self) -> &'static str {
        match self {
            CoverageArea::Home => "home",
            CoverageArea::Syllabus => "syllabus",
            CoverageArea::Modules => "modules",
            CoverageArea::Pages => "pages",
            CoverageArea::Files => "files",
            CoverageArea::Announcements => "announcements",
            CoverageArea::Assignments => "assignments",
            CoverageArea::Quizzes => "quizzes",
            CoverageArea::Discussions => "discussions",
            CoverageArea::Grades => "grades",
            CoverageArea::People => "people",
            CoverageArea::ExternalTool => "external_tool",
            CoverageArea::Other => "other",
        }
    }
}

/// What a course's Home shows in Canvas (its `default_view`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CourseHomeKind {
    /// A page the instructor wrote (Canvas: `wiki`).
    Page,
    /// The module list.
    Modules,
    /// The syllabus.
    Syllabus,
    /// The assignment list.
    Assignments,
    /// The activity stream (Canvas: `feed`).
    Activity,
    /// Canvas didn't say, or said something this version doesn't know.
    #[default]
    #[serde(other)]
    Unknown,
}

impl CourseHomeKind {
    /// From Canvas's `default_view`.
    pub fn from_canvas(default_view: Option<&str>) -> Self {
        match default_view {
            Some("wiki") => CourseHomeKind::Page,
            Some("modules") => CourseHomeKind::Modules,
            Some("syllabus") => CourseHomeKind::Syllabus,
            Some("assignments") => CourseHomeKind::Assignments,
            Some("feed") => CourseHomeKind::Activity,
            _ => CourseHomeKind::Unknown,
        }
    }
}

/// What a sync did with the Home page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CourseHomeState {
    /// The Home is a page and PageLamp has its text (`Home::material_id`).
    Read,
    /// The Home is a page Canvas locks for this student: PageLamp has its title only.
    Locked,
    /// The Home is another view (modules, the syllabus, assignments, the activity stream):
    /// there is no page to read.
    NotAPage,
    /// The course has no front page.
    Missing,
    /// The request for it failed in this sync.
    Failed,
    /// Not known (a record another version wrote, or nothing asked yet).
    #[default]
    #[serde(other)]
    Unknown,
}

/// The course's Home.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Home {
    pub kind: CourseHomeKind,
    pub state: CourseHomeState,
    /// The front page as a material (`Read` and `Locked`; kept from an earlier sync when
    /// this one `Failed`).
    pub material_id: Option<String>,
}

/// What a sync did with one list of the course.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CoverageListState {
    /// Read completely.
    Read,
    /// Hidden in the course's navigation: not asked for.
    Hidden,
    /// Asked for and not read (completely) in this sync.
    Failed,
    #[default]
    #[serde(other)]
    Unknown,
}

/// Something PageLamp didn't read. Built with `NotRead::new`, which keeps what may be stored.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NotRead {
    pub area: CoverageArea,
    pub reason: CoverageReason,
    /// The title Canvas gives the item in its own structure (a tab's label, a module item's
    /// or a page's title). `None` for one found through a link in a text.
    #[serde(default)]
    pub title: Option<String>,
    /// Where the student can open it in Canvas.
    #[serde(default)]
    pub url: Option<String>,
}

impl NotRead {
    /// `title`: only one from Canvas's structure, never a link's words; it is cut to
    /// `MAX_TITLE_CHARS`. `url` loses its access parameters (`scrub`).
    pub fn new(
        area: CoverageArea,
        reason: CoverageReason,
        title: Option<&str>,
        url: Option<&str>,
    ) -> Self {
        let title = title
            .map(|title| title.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|title| !title.is_empty())
            .map(|title| match title.char_indices().nth(MAX_TITLE_CHARS) {
                Some((end, _)) => format!("{}…", &title[..end]),
                None => title,
            });
        let url = url
            .map(|url| crate::scrub::scrub_text(url.trim()).into_owned())
            .filter(|url| !url.is_empty());
        NotRead {
            area,
            reason,
            title,
            url,
        }
    }
}

/// How much a sync read, in numbers (the sync row's line and the CLI's summary).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct CoverageCounts {
    /// Page materials of the course after the sync (the Home page and linked pages included).
    pub pages: u32,
    /// Of them, pages no list or module gave: found through a link in a text.
    pub linked_pages: u32,
    /// File materials of the course after the sync.
    pub files: u32,
    /// Of them, files found through a link in a text only.
    pub linked_files: u32,
    /// Links PageLamp left alone because of a limit of the sync.
    pub capped: u32,
    /// Addresses outside Canvas in the texts that were read (counted, never listed or opened).
    pub off_site_links: u32,
}

/// A link found in a page's body. Only what the next sync needs: never the link's words.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "to", rename_all = "snake_case")]
pub enum FoundLink {
    /// A page of this course, by its slug.
    Page { slug: String },
    /// A file of this course, by its Canvas id.
    File { id: String },
    /// Something PageLamp doesn't follow (an assignment, a quiz, another course…).
    NotFollowed {
        area: CoverageArea,
        reason: CoverageReason,
        url: String,
    },
    /// A link a later version wrote.
    #[serde(other)]
    Unknown,
}

/// A page whose body a sync read: what the next sync needs to leave it alone.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PageRead {
    /// The page's slug in Canvas.
    pub slug: String,
    /// When its body was last asked for.
    pub read_at: Option<Timestamp>,
    /// No list or module gives this page: it was found through a link in a text (or is the
    /// Home page). Kept when the Pages list is hidden; nothing else says it still exists.
    pub linked: bool,
    /// What its body links to, in document order, without repeats: followed again without
    /// reading the page again.
    pub links: Vec<FoundLink>,
    /// Addresses outside Canvas in its body.
    pub off_site_links: u32,
    /// Other slugs that led to this page: Canvas may answer for a page's old slug, its title
    /// form and its id too. A link that uses one of them isn't asked for again.
    pub also: Vec<String>,
    /// Canvas locks the page for this student: PageLamp has its title only. Kept so that a
    /// sync that doesn't ask again still says so.
    pub locked: bool,
    /// Its body held more links, or was longer, than a sync looks at: the rest wasn't taken.
    pub cut: bool,
}

impl PageRead {
    /// Whether `slug` is this page's slug or another address that led to it.
    pub fn answers_to(&self, slug: &str) -> bool {
        self.slug == slug || self.also.iter().any(|other| other == slug)
    }
}

/// A file found through a link only, while the Files list is hidden.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FileSeen {
    /// When Canvas was last asked about it.
    pub checked_at: Option<Timestamp>,
    /// Canvas no longer has it (the material stays: nothing is deleted on that answer).
    pub gone: bool,
}

/// What the next sync of the course needs from this one. Not shown anywhere.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Followed {
    /// By page material id: every page whose body was read.
    pub pages: BTreeMap<String, PageRead>,
    /// By file material id: files only a link leads to.
    pub files: BTreeMap<String, FileSeen>,
    /// Page slugs a link names for which Canvas said there is no such page, with when it
    /// was asked. Such a link is asked about again a week later, not at every sync. It is
    /// the author's dead link, not something PageLamp failed to read: no entry says it.
    pub dead_pages: BTreeMap<String, Timestamp>,
}

impl Followed {
    /// The materials only this record knows of: a sync must not remove them just because no
    /// list or module named them (pages found through a link, files found through a link).
    pub fn keep(&self) -> impl Iterator<Item = &str> {
        self.pages
            .iter()
            .filter(|(_, page)| page.linked)
            .map(|(id, _)| id.as_str())
            .chain(self.files.keys().map(String::as_str))
    }
}

/// The record of one course (see the module docs).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CourseCoverage {
    /// `VERSION` when it was written.
    pub version: u32,
    /// When the sync that wrote it ended its reading of the course.
    pub written_at: Option<Timestamp>,
    pub home: Home,
    /// The Pages list.
    pub pages_list: CoverageListState,
    /// The Files list.
    pub files_list: CoverageListState,
    /// The module list.
    pub modules_list: CoverageListState,
    /// What wasn't read, in the order the sync met it; at most `MAX_STORED`.
    pub not_read: Vec<NotRead>,
    /// How many there were.
    pub not_read_total: u32,
    pub counts: CoverageCounts,
    pub followed: Followed,
    /// Materials PageLamp lists but has no text of, by a rule or a failure: material id →
    /// why (a page a module asks the student to view, a locked page, a request that failed).
    /// `read_material` and `list_materials` say so instead of "no text". Files that aren't
    /// downloaded are not here: their own state says it.
    pub unread: BTreeMap<String, CoverageReason>,
}

impl CourseCoverage {
    /// A record for a sync to fill.
    pub fn new(written_at: Timestamp) -> Self {
        CourseCoverage {
            version: VERSION,
            written_at: Some(written_at),
            ..CourseCoverage::default()
        }
    }

    /// Note something that wasn't read: stored while there is room, always counted.
    pub fn note(&mut self, entry: NotRead) {
        self.not_read_total = self.not_read_total.saturating_add(1);
        if self.not_read.len() < MAX_STORED {
            self.not_read.push(entry);
        }
    }

    /// Whether Canvas no longer has the file `material_id` (a linked file whose last check
    /// answered "not found"; the material stays).
    pub fn is_gone(&self, material_id: &str) -> bool {
        self.followed
            .files
            .get(material_id)
            .is_some_and(|seen| seen.gone)
    }

    /// Why PageLamp has no text of the material `material_id`, when this record says: a page
    /// it didn't read by a rule or after a failed request, or a file Canvas no longer has.
    /// Only for a material without text: one that has text has it from an earlier sync.
    pub fn why_no_text(&self, material_id: &str) -> Option<CoverageReason> {
        if self.is_gone(material_id) {
            Some(CoverageReason::NoLongerInCanvas)
        } else {
            self.unread.get(material_id).copied()
        }
    }
}

/// The record of `course_id`: `None` when no full sync has written one, when a later
/// version wrote it, or when it can't be read as one (a failed read of the database is
/// still an error).
pub fn read(store: &Store, course_id: &str) -> Result<Option<CourseCoverage>> {
    // Read as plain JSON first: `setting_or_absent` names the key when a value doesn't fit
    // its type, and this key holds a course's id.
    let value: Option<serde_json::Value> = store.setting_or_absent(&key(course_id))?;
    Ok(value
        .and_then(|value| serde_json::from_value::<CourseCoverage>(value).ok())
        .filter(|record| (1..=VERSION).contains(&record.version)))
}

/// Store the record of `course_id` (call it inside the sync's transaction).
pub fn write(store: &Store, course_id: &str, coverage: &CourseCoverage) -> Result<()> {
    store.set_setting(&key(course_id), coverage)
}

/// Forget the record of `course_id` (absent is fine).
pub fn remove(store: &Store, course_id: &str) -> Result<()> {
    store.remove_setting(&key(course_id))
}

/// Forget the records of every course of `source_id` (when the source is removed). Course
/// ids start with their source's id and a `/`.
pub fn remove_for_source(store: &Store, source_id: &str) -> Result<usize> {
    store.remove_settings_with_prefix(&format!("{KEY_PREFIX}{source_id}/"))
}

/// A material named by a view.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MaterialRef {
    pub id: String,
    pub title: String,
}

/// Something PageLamp didn't read, as the views give it. `count` items share the area and
/// the reason when it is more than 1 (files that aren't downloaded come as one entry).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NotReadable {
    pub area: CoverageArea,
    pub reason: CoverageReason,
    pub title: Option<String>,
    pub url: Option<String>,
    pub count: u32,
}

/// What PageLamp read of a Canvas course and what it didn't (`CourseOverview::coverage`).
/// Structure only, so it is given for a course whose text is withheld too.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CoverageView {
    /// What the course's Home shows in Canvas.
    pub home_kind: CourseHomeKind,
    pub home_state: CourseHomeState,
    /// The Home page, when it is a page PageLamp stored.
    pub home: Option<MaterialRef>,
    /// The syllabus, when the course has one.
    pub syllabus: Option<MaterialRef>,
    pub pages_list: CoverageListState,
    pub files_list: CoverageListState,
    /// What PageLamp didn't read: files that aren't downloaded first (one entry per reason),
    /// then what the last full sync noted, in its order. At most `MAX_SHOWN`.
    pub not_readable: Vec<NotReadable>,
    /// How many more entries there are than `not_readable` gives.
    pub not_readable_more: u32,
    /// Announcements PageLamp has of this course.
    pub announcements_synced: u32,
    pub counts: CoverageCounts,
    /// When the last full sync wrote this.
    pub written_at: Option<Timestamp>,
}

/// The coverage of `course` for the views, with the course's `materials` as they are now:
/// `None` when no full sync has written its record
/// (a course of a folder or a calendar feed, a course only a light sync has seen, or one
/// last synced by an earlier version).
pub fn view(
    store: &Store,
    course: &Course,
    materials: &[Material],
) -> Result<Option<CoverageView>> {
    Ok(read(store, &course.id)?.map(|record| view_of(&record, materials)))
}

/// `view` of a record that was read already.
pub fn view_of(record: &CourseCoverage, materials: &[Material]) -> CoverageView {
    let named = |id: &str| {
        materials
            .iter()
            .find(|material| material.id == id)
            .map(|material| MaterialRef {
                id: material.id.clone(),
                title: material.title.clone(),
            })
    };
    let home = record.home.material_id.as_deref().and_then(named);
    let syllabus = materials
        .iter()
        .find(|material| material.kind == MaterialKind::Syllabus)
        .map(|material| MaterialRef {
            id: material.id.clone(),
            title: material.title.clone(),
        });
    let announcements = materials
        .iter()
        .filter(|material| material.kind == MaterialKind::Announcement)
        .count();

    // Files that aren't downloaded: counted from the materials as they are now (a download
    // since the sync changes them), one entry per reason.
    // (A file Canvas no longer has can't be downloaded: the sync's own entry says so.)
    let waiting = |blocked: Option<DownloadBlock>| {
        materials
            .iter()
            .filter(|material| {
                material.kind == MaterialKind::File
                    && material.text_status == TextStatus::NotDownloaded
                    && material.download_blocked == blocked
                    && !record.is_gone(&material.id)
            })
            .count()
    };
    let mut entries: Vec<NotReadable> = [
        (CoverageReason::NeedsDownload, waiting(None)),
        (
            CoverageReason::TooLarge,
            waiting(Some(DownloadBlock::TooLarge)),
        ),
        (CoverageReason::Locked, waiting(Some(DownloadBlock::Locked))),
    ]
    .into_iter()
    .filter(|(_, count)| *count > 0)
    .map(|(reason, count)| NotReadable {
        area: CoverageArea::Files,
        reason,
        title: None,
        url: None,
        count: to_u32(count),
    })
    .collect();
    let groups = entries.len();
    entries.extend(record.not_read.iter().map(|entry| NotReadable {
        area: entry.area,
        reason: entry.reason,
        title: entry.title.clone(),
        url: entry.url.clone(),
        count: 1,
    }));
    let total =
        to_u32(groups).saturating_add(record.not_read_total.max(to_u32(record.not_read.len())));
    entries.truncate(MAX_SHOWN);
    let shown = to_u32(entries.len());

    CoverageView {
        home_kind: record.home.kind,
        home_state: record.home.state,
        home,
        syllabus,
        pages_list: record.pages_list,
        files_list: record.files_list,
        not_readable: entries,
        not_readable_more: total.saturating_sub(shown),
        announcements_synced: to_u32(announcements),
        counts: record.counts,
        written_at: record.written_at,
    }
}

fn to_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use serde_json::json;

    use super::*;
    use crate::model::{CourseUpsert, MaterialUpsert, SourceKind, SourceRecord};

    const SOURCE: &str = "canvas:lms.example.edu";
    const COURSE: &str = "canvas:lms.example.edu/course/101";

    fn store() -> (tempfile::TempDir, Store) {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(&temp.path().join("pagelamp.db")).unwrap();
        store
            .upsert_source(&SourceRecord {
                id: SOURCE.into(),
                kind: SourceKind::Canvas,
                label: "Demo LMS".into(),
                config: json!({ "base_url": "https://lms.example.edu" }),
                last_synced_at: None,
                last_error: None,
                last_error_kind: None,
            })
            .unwrap();
        store
            .upsert_course(&CourseUpsert {
                id: COURSE.into(),
                source_id: SOURCE.into(),
                external_id: "101".into(),
                code: Some("DEMO101".into()),
                name: "Intro to Demo Studies".into(),
                term_start: None,
                term_end: None,
                url: None,
                syllabus_text: None,
                lms: Default::default(),
            })
            .unwrap();
        (temp, store)
    }

    fn material(store: &Store, id: &str, kind: MaterialKind, title: &str) {
        store
            .upsert_material(&MaterialUpsert {
                id: format!("{SOURCE}/{id}"),
                course_id: COURSE.into(),
                module_id: None,
                kind,
                title: title.into(),
                url: None,
                local_path: None,
                mime: None,
                published_at: None,
                week_hint: None,
            })
            .unwrap();
    }

    fn at() -> Timestamp {
        Utc.with_ymd_and_hms(2026, 10, 1, 9, 0, 0).unwrap()
    }

    #[test]
    fn codes_are_snake_case_and_an_unknown_one_reads_as_other() {
        for reason in [
            CoverageReason::IndexHidden,
            CoverageReason::NeedsDownload,
            CoverageReason::TooLarge,
            CoverageReason::Locked,
            CoverageReason::WouldMarkViewed,
            CoverageReason::ByRule,
            CoverageReason::OutsideCanvas,
            CoverageReason::NotRead,
            CoverageReason::OtherCourse,
            CoverageReason::Capped,
            CoverageReason::FailedThisSync,
            CoverageReason::NoLongerInCanvas,
            CoverageReason::Other,
        ] {
            assert_eq!(
                serde_json::to_value(reason).unwrap(),
                json!(reason.as_str())
            );
        }
        for area in [
            CoverageArea::Home,
            CoverageArea::Syllabus,
            CoverageArea::Modules,
            CoverageArea::Pages,
            CoverageArea::Files,
            CoverageArea::Announcements,
            CoverageArea::Assignments,
            CoverageArea::Quizzes,
            CoverageArea::Discussions,
            CoverageArea::Grades,
            CoverageArea::People,
            CoverageArea::ExternalTool,
            CoverageArea::Other,
        ] {
            assert_eq!(serde_json::to_value(area).unwrap(), json!(area.as_str()));
        }
        let entry: NotRead =
            serde_json::from_value(json!({ "area": "inbox", "reason": "a_later_reason" })).unwrap();
        assert_eq!(
            (entry.area, entry.reason),
            (CoverageArea::Other, CoverageReason::Other)
        );
        assert_eq!(
            serde_json::from_value::<CourseHomeKind>(json!("dashboard")).unwrap(),
            CourseHomeKind::Unknown
        );
        assert_eq!(
            CourseHomeKind::from_canvas(Some("wiki")),
            CourseHomeKind::Page
        );
        assert_eq!(
            CourseHomeKind::from_canvas(Some("feed")),
            CourseHomeKind::Activity
        );
        assert_eq!(CourseHomeKind::from_canvas(None), CourseHomeKind::Unknown);
        let link: FoundLink = serde_json::from_value(json!({ "to": "media", "id": "7" })).unwrap();
        assert_eq!(link, FoundLink::Unknown);
    }

    #[test]
    fn an_entry_keeps_a_short_title_and_an_address_without_access_parameters() {
        let long = "Week  1 \n notes ".repeat(40);
        let entry = NotRead::new(
            CoverageArea::Pages,
            CoverageReason::WouldMarkViewed,
            Some(&long),
            Some(" https://lms.example.edu/courses/101/files/7/download?verifier=SECRET&wrap=1 "),
        );
        let title = entry.title.unwrap();
        assert_eq!(title.chars().count(), MAX_TITLE_CHARS + 1, "{title}");
        assert!(title.starts_with("Week 1 notes Week 1 notes"), "{title}");
        assert!(title.ends_with('…'));
        assert_eq!(
            entry.url.as_deref(),
            Some("https://lms.example.edu/courses/101/files/7/download")
        );
        let empty = NotRead::new(
            CoverageArea::Grades,
            CoverageReason::NotRead,
            Some("  "),
            Some(""),
        );
        assert_eq!((empty.title, empty.url), (None, None));
    }

    #[test]
    fn a_record_stores_a_hundred_entries_and_counts_them_all() {
        let mut record = CourseCoverage::new(at());
        for n in 0..(MAX_STORED + 30) {
            record.note(NotRead::new(
                CoverageArea::Assignments,
                CoverageReason::ByRule,
                Some(&format!("Problem set {n}")),
                None,
            ));
        }
        assert_eq!(record.not_read.len(), MAX_STORED);
        assert_eq!(record.not_read_total, 130);
        assert_eq!(record.not_read[99].title.as_deref(), Some("Problem set 99"));
    }

    #[test]
    fn the_record_is_read_back_and_a_strange_one_is_absent() {
        let (_temp, store) = store();
        assert_eq!(read(&store, COURSE).unwrap(), None);

        let mut record = CourseCoverage::new(at());
        record.home = Home {
            kind: CourseHomeKind::Page,
            state: CourseHomeState::Read,
            material_id: Some(format!("{SOURCE}/page/1")),
        };
        record.followed.pages.insert(
            format!("{SOURCE}/page/2"),
            PageRead {
                slug: "week-1".into(),
                read_at: Some(at()),
                linked: true,
                links: vec![
                    FoundLink::Page {
                        slug: "week-2".into(),
                    },
                    FoundLink::File { id: "500".into() },
                    FoundLink::NotFollowed {
                        area: CoverageArea::Quizzes,
                        reason: CoverageReason::ByRule,
                        url: "https://lms.example.edu/courses/101/quizzes/9".into(),
                    },
                ],
                off_site_links: 2,
                ..PageRead::default()
            },
        );
        record.followed.pages.insert(
            format!("{SOURCE}/page/3"),
            PageRead {
                slug: "listed".into(),
                ..PageRead::default()
            },
        );
        record
            .followed
            .files
            .insert(format!("{SOURCE}/file/500"), FileSeen::default());
        write(&store, COURSE, &record).unwrap();
        assert_eq!(read(&store, COURSE).unwrap(), Some(record.clone()));
        // Only what nothing else names is kept by the record.
        assert_eq!(
            record.followed.keep().collect::<Vec<_>>(),
            [
                format!("{SOURCE}/page/2").as_str(),
                format!("{SOURCE}/file/500").as_str()
            ]
        );

        // A later version's record, one without a version, and one of another shape.
        for value in [
            json!({ "version": VERSION + 1, "home": { "kind": "page" } }),
            json!({ "home": { "kind": "page" } }),
            json!(["not", "a", "record"]),
        ] {
            store.set_setting(&key(COURSE), &value).unwrap();
            assert_eq!(read(&store, COURSE).unwrap(), None, "{value}");
        }
        // A record with fields this version doesn't know, and codes it doesn't know.
        store
            .set_setting(
                &key(COURSE),
                &json!({
                    "version": 1,
                    "later_field": true,
                    "home": { "kind": "page", "state": "archived" },
                    "pages_list": "partly",
                    "not_read": [{ "area": "inbox", "reason": "later" }],
                    "not_read_total": 1
                }),
            )
            .unwrap();
        let lenient = read(&store, COURSE).unwrap().unwrap();
        assert_eq!(lenient.home.state, CourseHomeState::Unknown);
        assert_eq!(lenient.pages_list, CoverageListState::Unknown);
        assert_eq!(lenient.not_read[0].reason, CoverageReason::Other);
    }

    #[test]
    fn records_go_with_their_course_and_their_source() {
        let (_temp, store) = store();
        let other_source = "canvas:lms.example.edu.evil.test/course/101";
        let other_course = "canvas:lms.example.edu/course/1010";
        for course in [COURSE, other_course, other_source] {
            write(&store, course, &CourseCoverage::new(at())).unwrap();
        }
        store.set_setting("sync.prefs", &json!({})).unwrap();

        remove(&store, COURSE).unwrap();
        remove(&store, COURSE).unwrap();
        assert_eq!(read(&store, COURSE).unwrap(), None);
        assert!(read(&store, other_course).unwrap().is_some());

        write(&store, COURSE, &CourseCoverage::new(at())).unwrap();
        assert_eq!(remove_for_source(&store, SOURCE).unwrap(), 2);
        assert_eq!(read(&store, COURSE).unwrap(), None);
        assert_eq!(read(&store, other_course).unwrap(), None);
        assert!(
            read(&store, other_source).unwrap().is_some(),
            "another source whose id starts the same"
        );
        assert!(
            store
                .setting::<serde_json::Value>("sync.prefs")
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn the_view_names_the_home_groups_the_files_and_counts_the_rest() {
        let (_temp, store) = store();
        let course = store.resolve_course("DEMO101").unwrap();
        assert_eq!(view(&store, &course, &[]).unwrap(), None, "no record yet");

        material(&store, "page/1", MaterialKind::Page, "Welcome");
        material(&store, "syllabus/101", MaterialKind::Syllabus, "Syllabus");
        material(
            &store,
            "announcement/1",
            MaterialKind::Announcement,
            "Hello",
        );
        material(&store, "announcement/2", MaterialKind::Announcement, "Room");
        for (id, blocked) in [
            ("file/1", None),
            ("file/2", None),
            ("file/3", Some(DownloadBlock::TooLarge)),
        ] {
            material(&store, id, MaterialKind::File, "A file");
            let id = format!("{SOURCE}/{id}");
            store
                .set_text_state(&id, TextStatus::NotDownloaded, None, None)
                .unwrap();
            store.set_download_blocked(&id, blocked).unwrap();
        }
        // A downloaded file is not waiting.
        material(&store, "file/4", MaterialKind::File, "Read");
        store
            .set_text_state(&format!("{SOURCE}/file/4"), TextStatus::Ok, None, Some("h"))
            .unwrap();

        let mut record = CourseCoverage::new(at());
        record.home = Home {
            kind: CourseHomeKind::Page,
            state: CourseHomeState::Read,
            material_id: Some(format!("{SOURCE}/page/1")),
        };
        record.pages_list = CoverageListState::Hidden;
        record.files_list = CoverageListState::Hidden;
        record.counts.linked_pages = 3;
        for n in 0..30 {
            record.note(NotRead::new(
                CoverageArea::Assignments,
                CoverageReason::ByRule,
                Some(&format!("Problem set {n}")),
                Some("https://lms.example.edu/courses/101/assignments/1"),
            ));
        }
        write(&store, COURSE, &record).unwrap();

        let materials = store.list_materials(COURSE).unwrap();
        let view = view(&store, &course, &materials).unwrap().unwrap();
        assert_eq!(view.home_kind, CourseHomeKind::Page);
        assert_eq!(
            view.home,
            Some(MaterialRef {
                id: format!("{SOURCE}/page/1"),
                title: "Welcome".into()
            })
        );
        assert_eq!(view.syllabus.unwrap().title, "Syllabus");
        assert_eq!(view.announcements_synced, 2);
        assert_eq!(view.pages_list, CoverageListState::Hidden);
        assert_eq!(view.counts.linked_pages, 3);
        assert_eq!(view.written_at, Some(at()));
        // The two groups of files first, then the sync's entries; 32 in all, 20 shown.
        assert_eq!(view.not_readable.len(), MAX_SHOWN);
        assert_eq!(
            view.not_readable[..2]
                .iter()
                .map(|entry| (entry.area, entry.reason, entry.count))
                .collect::<Vec<_>>(),
            [
                (CoverageArea::Files, CoverageReason::NeedsDownload, 2),
                (CoverageArea::Files, CoverageReason::TooLarge, 1)
            ]
        );
        assert_eq!(view.not_readable[2].title.as_deref(), Some("Problem set 0"));
        assert_eq!(view.not_readable[2].count, 1);
        assert_eq!(view.not_readable_more, 12);

        // A Home material that is gone is simply not named.
        record.home.material_id = Some(format!("{SOURCE}/page/404"));
        write(&store, COURSE, &record).unwrap();
        assert_eq!(
            super::view(&store, &course, &materials)
                .unwrap()
                .unwrap()
                .home,
            None
        );
    }
    #[test]
    fn a_file_canvas_no_longer_has_is_not_waiting_for_a_download() {
        let (_temp, store) = store();
        let course = store.resolve_course("DEMO101").unwrap();
        for id in ["file/1", "file/2"] {
            material(&store, id, MaterialKind::File, "A file");
            store
                .set_text_state(
                    &format!("{SOURCE}/{id}"),
                    TextStatus::NotDownloaded,
                    None,
                    None,
                )
                .unwrap();
        }
        let mut record = CourseCoverage::new(at());
        record.followed.files.insert(
            format!("{SOURCE}/file/2"),
            FileSeen {
                checked_at: Some(at()),
                gone: true,
            },
        );
        record.note(NotRead::new(
            CoverageArea::Files,
            CoverageReason::NoLongerInCanvas,
            Some("A file"),
            None,
        ));
        write(&store, COURSE, &record).unwrap();
        assert!(record.is_gone(&format!("{SOURCE}/file/2")));
        assert!(!record.is_gone(&format!("{SOURCE}/file/1")));
        // `ALL` names every reason once. (A new reason doesn't compile here until it is
        // added to this match: add it to `ALL` and to `as_str` too.)
        for reason in CoverageReason::ALL {
            match reason {
                CoverageReason::IndexHidden
                | CoverageReason::NeedsDownload
                | CoverageReason::TooLarge
                | CoverageReason::Locked
                | CoverageReason::WouldMarkViewed
                | CoverageReason::ByRule
                | CoverageReason::OutsideCanvas
                | CoverageReason::NotRead
                | CoverageReason::OtherCourse
                | CoverageReason::Capped
                | CoverageReason::FailedThisSync
                | CoverageReason::NoLongerInCanvas
                | CoverageReason::Other => {}
            }
        }
        let codes: std::collections::BTreeSet<&str> = CoverageReason::ALL
            .iter()
            .map(|reason| reason.as_str())
            .collect();
        assert_eq!(codes.len(), CoverageReason::ALL.len());
        assert_eq!(
            record.why_no_text(&format!("{SOURCE}/file/2")),
            Some(CoverageReason::NoLongerInCanvas)
        );
        assert_eq!(record.why_no_text(&format!("{SOURCE}/file/1")), None);

        let materials = store.list_materials(COURSE).unwrap();
        let view = view(&store, &course, &materials).unwrap().unwrap();
        assert_eq!(
            view.not_readable
                .iter()
                .map(|entry| (entry.reason, entry.count))
                .collect::<Vec<_>>(),
            [
                (CoverageReason::NeedsDownload, 1),
                (CoverageReason::NoLongerInCanvas, 1)
            ]
        );
    }

    #[test]
    fn a_page_answers_to_its_slug_and_to_the_other_addresses_that_led_to_it() {
        let read = PageRead {
            slug: "week-1".into(),
            also: vec!["week-one".into(), "601".into()],
            ..PageRead::default()
        };
        for slug in ["week-1", "week-one", "601"] {
            assert!(read.answers_to(slug), "{slug}");
        }
        assert!(!read.answers_to("week-2"));
        // A record written before these fields existed reads with none of them.
        let old: PageRead =
            serde_json::from_value(json!({ "slug": "week-1", "linked": true })).unwrap();
        assert!(old.also.is_empty() && !old.locked && !old.cut);
        let record: CourseCoverage =
            serde_json::from_value(json!({ "version": 1, "home": { "kind": "page" } })).unwrap();
        assert!(record.unread.is_empty());
    }
}
