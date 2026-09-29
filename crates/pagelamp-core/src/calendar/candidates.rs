//! Which of a course's materials a syllabus reading uses (docs/design/v0.3-course-calendar.md
//! §7.1). Computed at read time from kinds, titles, modules and posting dates; never stored.
//!
//! | Signal | Score |
//! |---|---|
//! | the LMS syllabus, with at least 200 characters of text | 100 |
//! | named as the outline in a folder's `course.toml` | 95 |
//! | a file linked from the syllabus | 90 |
//! | title: syllabus / course outline / outline / 教学大纲 | 80 |
//! | title: schedule / course calendar / important dates / 课程安排 / 日程 | 70 |
//! | title: course information / info sheet | 50 |
//! | the LMS front page | 40 |
//! | inside a "Start here" / "Course information" style module | +20 (20 alone) |
//! | an announcement from 21 days before teaching starts, or within 120 days of it that names a reading week, midterm, exam, schedule or week number (at most 2) | 30 |
//!
//! - External links and assessment-looking titles (D34, `ai_gate`'s title rule) are never
//!   candidates, unless the title names the calendar itself ("Exam schedule") or the student
//!   adds them.
//! - At most 4 candidates with text are read: the student's own adds first, then by score.
//!   Further ones are listed as `over_budget`. A scanned file (read, no text in it) and a file
//!   without text are listed with that reason and take no place; a file not downloaded yet is
//!   `downloadable` (D46).
//! - Inside a candidate, chunks are chosen by date density and calendar words until its share
//!   of the budget is full, then kept in their original order (`select_chunks`), so a schedule
//!   on the last pages of a long outline isn't cut off. Up to 8 chunks of other materials that
//!   hold both a date and a calendar word are added (`extra_chunks`).
//!
//! The syllabus links, the front page and the student's choices come from schema v4
//! (`CandidateSignals`); before that they are empty.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::LazyLock;

use chrono::NaiveDate;
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::scan;
use crate::ai_gate::looks_like_assessment;
use crate::dates::parse::find_dates;
use crate::dates::{Tz, course_date, days_between};
use crate::model::{Chunk, Course, Material, MaterialKind, TextStatus};
use crate::store::Store;
use crate::term::ResolvedTerm;
use crate::views::{AsOf, CourseData};

/// Candidates with text that a reading uses (the student's adds may go past it).
pub const MAX_CANDIDATES_WITH_TEXT: usize = 4;
/// Announcements that can be candidates.
pub const MAX_ANNOUNCEMENTS: usize = 2;
/// Chunks of non-candidate materials that `extra_chunks` adds.
pub const MAX_EXTRA_CHUNKS: usize = 8;
/// An LMS syllabus shorter than this is usually "see the outline PDF".
const MIN_SYLLABUS_CHARS: usize = 200;
/// Announcements posted this many days before teaching starts are candidates.
const ANNOUNCEMENT_LEAD_DAYS: i64 = 21;
/// Announcements this many days from the start are candidates when they name a calendar word.
const ANNOUNCEMENT_REACH_DAYS: i64 = 120;

const SCORE_SYLLABUS: u32 = 100;
const SCORE_NAMED: u32 = 95;
const SCORE_LINKED: u32 = 90;
const SCORE_OUTLINE: u32 = 80;
const SCORE_SCHEDULE: u32 = 70;
const SCORE_INFO: u32 = 50;
const SCORE_FRONT_PAGE: u32 = 40;
const SCORE_ANNOUNCEMENT: u32 = 30;
const SCORE_START_MODULE: u32 = 20;

/// Why a material is a candidate.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum CandidateReason {
    /// The LMS syllabus.
    Syllabus,
    /// A file the syllabus links to.
    LinkedFromSyllabus,
    /// The title names a syllabus or outline.
    TitleOutline,
    /// The title names a schedule or course calendar.
    TitleSchedule,
    /// The title names course information.
    TitleInfo,
    /// The LMS front page.
    FrontPage,
    /// Inside a "Start here" style module.
    StartModule,
    Announcement,
    /// A folder's `course.toml` names it as the outline.
    NamedInCourseToml,
    /// The student added it.
    StudentAdded,
}

/// Why a candidate is not read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CandidateLeftOut {
    /// No text yet: not downloaded, not read yet, or reading it failed.
    NoText,
    /// Read, but there is no text in it (a scanned file).
    Scanned,
    /// Past the 4 candidates a reading uses.
    OverBudget,
    /// The student removed it.
    ExcludedByStudent,
}

/// A material a syllabus reading would use, and whether it does.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CalendarCandidate {
    pub material_id: String,
    pub title: String,
    pub kind: MaterialKind,
    pub reason: CandidateReason,
    /// Read by the scan and the AI (after the student's own choice, if any).
    pub included: bool,
    /// The student's add (true) or remove (false), if they chose.
    pub student_choice: Option<bool>,
    pub has_text: bool,
    /// A file not downloaded yet that can be ("Download (counts as viewing in Canvas)").
    pub downloadable: bool,
    pub left_out: Option<CandidateLeftOut>,
    pub url: Option<String>,
}

/// Candidate signals that schema v4 stores (empty before it).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CandidateSignals {
    /// Materials the syllabus links to (sync, S5).
    pub linked_from_syllabus: BTreeSet<String>,
    /// The LMS front page (sync, S4).
    pub front_page: BTreeSet<String>,
    /// The outline a folder's `course.toml` names.
    pub named_in_course_toml: BTreeSet<String>,
    /// The student's add (true) or remove (false), by material id.
    pub choices: BTreeMap<String, bool>,
}

/// One material as scoring sees it.
#[derive(Clone, Debug)]
pub struct CandidateMaterial<'a> {
    pub material: &'a Material,
    pub module_name: Option<&'a str>,
    pub chunk_count: u32,
    /// Characters of text (only needed for the LMS syllabus).
    pub text_chars: usize,
    /// The text names a calendar word (only needed for announcements).
    pub names_calendar: bool,
}

/// A candidate with its score (higher first).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScoredCandidate {
    pub candidate: CalendarCandidate,
    pub score: u32,
}

fn regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("valid candidate regex")
}

static TITLE_OUTLINE: LazyLock<Regex> = LazyLock::new(|| {
    regex(
        r"(?i)(?-u:\b)(?:syllabus|syllabi|(?:course\s+)?outline)(?-u:\b)|教学大纲|课程大纲|課程大綱",
    )
});

static TITLE_SCHEDULE: LazyLock<Regex> = LazyLock::new(|| {
    regex(
        r"(?i)(?-u:\b)(?:schedule|course\s+calendar|important\s+dates|key\s+dates|timetable)(?-u:\b)|课程安排|日程|时间表|時間表",
    )
});

static TITLE_INFO: LazyLock<Regex> = LazyLock::new(|| {
    regex(
        r"(?i)(?-u:\b)(?:course\s+info(?:rmation)?|info(?:rmation)?\s+sheet)(?-u:\b)|课程信息|課程資訊",
    )
});

static START_MODULE: LazyLock<Regex> = LazyLock::new(|| {
    regex(
        r"(?i)(?-u:\b)(?:start\s+here|getting\s+started|course\s+info(?:rmation)?|course\s+overview|welcome)(?-u:\b)|课程信息|课程简介|开始",
    )
});

/// Calendar words beyond the scan's keyword classes: what makes an announcement or a chunk
/// about the calendar.
static CALENDAR_WORDS: LazyLock<Regex> = LazyLock::new(|| {
    regex(
        r"(?i)(?-u:\b)(?:reading\s+week|mid-?terms?|exams?|schedule|week\s*\d{1,2})(?-u:\b)|第\s*\d{1,2}\s*[周週]|期中|期末",
    )
});

/// How many calendar words `text` names (the scan's keyword classes and `CALENDAR_WORDS`).
pub fn calendar_words(text: &str) -> usize {
    CALENDAR_WORDS.find_iter(text).count() + scan::keyword_count(text)
}

/// The calendar signal of a title, if it has one.
fn title_signal(title: &str) -> Option<(CandidateReason, u32)> {
    if TITLE_OUTLINE.is_match(title) {
        Some((CandidateReason::TitleOutline, SCORE_OUTLINE))
    } else if TITLE_SCHEDULE.is_match(title) {
        Some((CandidateReason::TitleSchedule, SCORE_SCHEDULE))
    } else if TITLE_INFO.is_match(title) {
        Some((CandidateReason::TitleInfo, SCORE_INFO))
    } else {
        None
    }
}

/// The reason and score of one material, if it is a candidate by itself.
fn signal(
    entry: &CandidateMaterial<'_>,
    signals: &CandidateSignals,
    announcement_ok: bool,
) -> Option<(CandidateReason, u32)> {
    let material = entry.material;
    let id = material.id.as_str();
    match material.kind {
        MaterialKind::ExternalLink => return None,
        MaterialKind::Syllabus => {
            return (entry.text_chars >= MIN_SYLLABUS_CHARS)
                .then_some((CandidateReason::Syllabus, SCORE_SYLLABUS));
        }
        MaterialKind::Announcement => {
            return announcement_ok.then_some((CandidateReason::Announcement, SCORE_ANNOUNCEMENT));
        }
        MaterialKind::File | MaterialKind::Page => {}
    }
    if signals.named_in_course_toml.contains(id) {
        return Some((CandidateReason::NamedInCourseToml, SCORE_NAMED));
    }
    let title = title_signal(&material.title);
    // D34: an assessment-looking title is left out unless it names the calendar itself.
    if title.is_none() && looks_like_assessment(&material.title) {
        return None;
    }
    let base = [
        signals
            .linked_from_syllabus
            .contains(id)
            .then_some((CandidateReason::LinkedFromSyllabus, SCORE_LINKED)),
        title,
        signals
            .front_page
            .contains(id)
            .then_some((CandidateReason::FrontPage, SCORE_FRONT_PAGE)),
    ]
    .into_iter()
    .flatten()
    .max_by_key(|(_, score)| *score);
    let in_start_module = entry
        .module_name
        .is_some_and(|name| START_MODULE.is_match(name));
    match (base, in_start_module) {
        (Some((reason, score)), true) => Some((reason, score + SCORE_START_MODULE)),
        (Some(base), false) => Some(base),
        (None, true) => Some((CandidateReason::StartModule, SCORE_START_MODULE)),
        (None, false) => None,
    }
}

/// The announcements that may be candidates: posted from 21 days before teaching starts, or
/// within 120 days of the start and naming a calendar word; the 2 closest to the start.
fn announcement_ids(
    materials: &[CandidateMaterial<'_>],
    teaching_start: Option<NaiveDate>,
    tz: Option<Tz>,
) -> BTreeSet<String> {
    let Some(start) = teaching_start else {
        return BTreeSet::new();
    };
    let mut fitting: Vec<(i64, &str)> = materials
        .iter()
        .filter(|entry| entry.material.kind == MaterialKind::Announcement)
        .filter_map(|entry| {
            let posted = course_date(entry.material.published_at?, tz);
            let offset = days_between(start, posted);
            let lead = (-ANNOUNCEMENT_LEAD_DAYS..=0).contains(&offset);
            let named = offset.abs() <= ANNOUNCEMENT_REACH_DAYS && entry.names_calendar;
            (lead || named).then_some((offset.abs(), entry.material.id.as_str()))
        })
        .collect();
    fitting.sort();
    fitting
        .into_iter()
        .take(MAX_ANNOUNCEMENTS)
        .map(|(_, id)| id.to_string())
        .collect()
}

/// Score the course's materials (§7.1): candidates first by whether they are read, then by
/// score, then by title.
pub fn score_candidates(
    materials: &[CandidateMaterial<'_>],
    signals: &CandidateSignals,
    teaching_start: Option<NaiveDate>,
    tz: Option<Tz>,
) -> Vec<ScoredCandidate> {
    let announcements = announcement_ids(materials, teaching_start, tz);
    let mut scored: Vec<ScoredCandidate> = Vec::new();
    for entry in materials {
        let material = entry.material;
        let choice = signals.choices.get(&material.id).copied();
        let found = signal(entry, signals, announcements.contains(material.id.as_str()));
        let (reason, score) = match (found, choice) {
            (Some(found), _) => found,
            (None, Some(true)) if material.kind != MaterialKind::ExternalLink => {
                (CandidateReason::StudentAdded, 0)
            }
            _ => continue,
        };
        let has_text = material.text_status == TextStatus::Ok && entry.chunk_count > 0;
        let scanned = material.text_status == TextStatus::Ok && entry.chunk_count == 0;
        let left_out = if choice == Some(false) {
            Some(CandidateLeftOut::ExcludedByStudent)
        } else if scanned {
            Some(CandidateLeftOut::Scanned)
        } else if !has_text {
            Some(CandidateLeftOut::NoText)
        } else {
            None
        };
        scored.push(ScoredCandidate {
            candidate: CalendarCandidate {
                material_id: material.id.clone(),
                title: material.title.clone(),
                kind: material.kind,
                reason,
                included: false,
                student_choice: choice,
                has_text,
                downloadable: material.kind == MaterialKind::File
                    && material.text_status == TextStatus::NotDownloaded
                    && material.download_blocked.is_none(),
                left_out,
                url: material.url.clone(),
            },
            score,
        });
    }
    // The student's adds first, then by score; the first 4 readable ones are read.
    scored.sort_by(|a, b| {
        let added = |c: &ScoredCandidate| c.candidate.student_choice == Some(true);
        added(b)
            .cmp(&added(a))
            .then(b.score.cmp(&a.score))
            .then_with(|| a.candidate.title.cmp(&b.candidate.title))
            .then_with(|| a.candidate.material_id.cmp(&b.candidate.material_id))
    });
    let mut places = 0usize;
    for entry in &mut scored {
        let candidate = &mut entry.candidate;
        if candidate.left_out.is_some() {
            continue;
        }
        if candidate.student_choice == Some(true) || places < MAX_CANDIDATES_WITH_TEXT {
            candidate.included = true;
            places += 1;
        } else {
            candidate.left_out = Some(CandidateLeftOut::OverBudget);
        }
    }
    scored.sort_by_key(|entry| !entry.candidate.included);
    scored
}

/// The course's candidates, read from the store.
pub fn course_candidates(
    store: &Store,
    course: &Course,
    at: AsOf,
    signals: &CandidateSignals,
) -> crate::Result<Vec<ScoredCandidate>> {
    let data = CourseData::load(store, course)?;
    let (resolved, _) = data.timeline(course, at);
    candidates_in(store, &data, &resolved, signals)
}

/// `course_candidates` over rows already loaded.
pub(crate) fn candidates_in(
    store: &Store,
    data: &CourseData,
    resolved: &ResolvedTerm,
    signals: &CandidateSignals,
) -> crate::Result<Vec<ScoredCandidate>> {
    let resolution = &resolved.resolution;
    let teaching_start = resolution
        .teaching
        .first()
        .map(|segment| segment.first_class)
        .or(resolution.week_one_monday)
        .or(resolution.outer_frame.map(|frame| frame.start));
    let module_names: HashMap<&str, &str> = data
        .modules
        .iter()
        .map(|m| (m.id.as_str(), m.name.as_str()))
        .collect();
    let mut entries = Vec::with_capacity(data.materials.len());
    for material in &data.materials {
        let chunk_count = data.chunks_of(&material.id);
        let needs_text = chunk_count > 0
            && matches!(
                material.kind,
                MaterialKind::Syllabus | MaterialKind::Announcement
            );
        let (text_chars, names_calendar) = if needs_text {
            let chunks = store.get_chunks(&material.id, 0, None)?;
            (
                chunks.iter().map(|c| c.text.chars().count()).sum(),
                chunks.iter().any(|c| CALENDAR_WORDS.is_match(&c.text)),
            )
        } else {
            (0, false)
        };
        entries.push(CandidateMaterial {
            material,
            module_name: material
                .module_id
                .as_deref()
                .and_then(|id| module_names.get(id).copied()),
            chunk_count,
            text_chars,
            names_calendar,
        });
    }
    Ok(score_candidates(
        &entries,
        signals,
        teaching_start,
        resolved.tz,
    ))
}

/// How much a chunk says about the calendar: its dates and calendar words.
fn chunk_score(chunk: &Chunk) -> usize {
    find_dates(&chunk.text, None).len() + 2 * calendar_words(&chunk.text)
}

/// Which chunks of one candidate fit its share of `share_chars` characters: the best by
/// `chunk_score` first (earlier ones on ties), then returned in their original order, with
/// the ords skipped.
pub fn select_chunks(chunks: &[Chunk], share_chars: usize) -> ChunkSelection {
    let mut by_score: Vec<(usize, &Chunk)> = chunks.iter().map(|c| (chunk_score(c), c)).collect();
    by_score.sort_by(|(a, x), (b, y)| b.cmp(a).then(x.ord.cmp(&y.ord)));
    let mut left = share_chars;
    let mut chosen = Vec::new();
    let mut skipped = Vec::new();
    for (_, chunk) in by_score {
        let size = chunk.text.chars().count();
        if size <= left {
            left -= size;
            chosen.push(chunk.ord);
        } else {
            skipped.push(chunk.ord);
        }
    }
    chosen.sort_unstable();
    skipped.sort_unstable();
    ChunkSelection { chosen, skipped }
}

/// The chunks of one candidate that a reading uses (ords, in order).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChunkSelection {
    pub chosen: Vec<u32>,
    pub skipped: Vec<u32>,
}

/// Up to 8 chunks of other materials that hold both a date and a calendar word (a schedule on
/// the first lecture's slides), best first: `(material id, ord)`.
pub fn extra_chunks<'a>(chunks: impl IntoIterator<Item = &'a Chunk>) -> Vec<(String, u32)> {
    let mut found: Vec<(usize, &Chunk)> = chunks
        .into_iter()
        .filter_map(|chunk| {
            let dates = find_dates(&chunk.text, None).len();
            let words = calendar_words(&chunk.text);
            (dates > 0 && words > 0).then_some((dates + 2 * words, chunk))
        })
        .collect();
    found.sort_by(|(a, x), (b, y)| {
        b.cmp(a)
            .then_with(|| x.material_id.cmp(&y.material_id))
            .then(x.ord.cmp(&y.ord))
    });
    found
        .into_iter()
        .take(MAX_EXTRA_CHUNKS)
        .map(|(_, chunk)| (chunk.material_id.clone(), chunk.ord))
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};

    use super::*;
    use crate::model::DownloadBlock;

    fn date(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    fn at(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(&format!("{text}T12:00:00Z"))
            .unwrap()
            .with_timezone(&Utc)
    }

    fn material(id: &str, kind: MaterialKind, title: &str) -> Material {
        Material {
            id: id.into(),
            course_id: "c".into(),
            module_id: None,
            kind,
            title: title.into(),
            url: Some(format!("https://lms.example.edu/{id}")),
            local_path: None,
            mime: None,
            published_at: Some(at("2026-09-01")),
            week_hint: None,
            content_hash: None,
            text_status: TextStatus::Ok,
            text_error: None,
            text_error_kind: None,
            text_error_fingerprint: None,
            download_blocked: None,
            updated_at: at("2026-09-01"),
        }
    }

    fn entry(material: &Material) -> CandidateMaterial<'_> {
        CandidateMaterial {
            material,
            module_name: None,
            chunk_count: 3,
            text_chars: 1_000,
            names_calendar: false,
        }
    }

    fn score(
        entries: &[CandidateMaterial<'_>],
        signals: &CandidateSignals,
    ) -> Vec<(String, CandidateReason, u32, bool, Option<CandidateLeftOut>)> {
        score_candidates(entries, signals, Some(date("2026-09-08")), None)
            .into_iter()
            .map(|s| {
                (
                    s.candidate.material_id,
                    s.candidate.reason,
                    s.score,
                    s.candidate.included,
                    s.candidate.left_out,
                )
            })
            .collect()
    }

    #[test]
    fn titles_kinds_and_modules_score_as_the_table_says() {
        let syllabus = material("syl", MaterialKind::Syllabus, "Syllabus");
        let short_syllabus = material("syl2", MaterialKind::Syllabus, "Syllabus");
        let outline = material("out", MaterialKind::File, "DEMO101 Course Outline.pdf");
        let schedule = material("sch", MaterialKind::Page, "Lecture schedule");
        let info = material("inf", MaterialKind::File, "Course information sheet");
        let welcome = material("wel", MaterialKind::Page, "Read me first");
        let slides = material("sl", MaterialKind::File, "Week 3 slides");
        let link = material(
            "lnk",
            MaterialKind::ExternalLink,
            "Course outline (website)",
        );
        let mut entries: Vec<CandidateMaterial<'_>> = [
            &syllabus, &outline, &schedule, &info, &welcome, &slides, &link,
        ]
        .into_iter()
        .map(entry)
        .collect();
        entries.push(CandidateMaterial {
            text_chars: 120,
            ..entry(&short_syllabus)
        });
        entries[4].module_name = Some("Start Here");
        entries[2].module_name = Some("Course information");
        let scored = score(&entries, &CandidateSignals::default());
        let reasons: Vec<(&str, CandidateReason, u32)> = scored
            .iter()
            .map(|(id, reason, score, ..)| (id.as_str(), *reason, *score))
            .collect();
        assert_eq!(
            reasons,
            [
                ("syl", CandidateReason::Syllabus, 100),
                ("sch", CandidateReason::TitleSchedule, 90),
                ("out", CandidateReason::TitleOutline, 80),
                ("inf", CandidateReason::TitleInfo, 50),
                ("wel", CandidateReason::StartModule, 20),
            ],
            "a short syllabus, plain slides and external links aren't candidates"
        );
        // Four are read; the fifth is over the limit.
        let read: Vec<bool> = scored.iter().map(|s| s.3).collect();
        assert_eq!(read, [true, true, true, true, false]);
        assert_eq!(scored[4].4, Some(CandidateLeftOut::OverBudget));
    }

    #[test]
    fn assessment_titles_are_left_out_unless_they_name_the_calendar() {
        let quiz = material("q", MaterialKind::File, "Quiz 2");
        let exam_dates = material("e", MaterialKind::Page, "Exam schedule");
        let linked_assignment = material("a", MaterialKind::File, "Assignment 1");
        let entries = [entry(&quiz), entry(&exam_dates), entry(&linked_assignment)];
        let signals = CandidateSignals {
            linked_from_syllabus: ["a".to_string()].into(),
            ..CandidateSignals::default()
        };
        let ids: Vec<String> = score(&entries, &signals).into_iter().map(|s| s.0).collect();
        assert_eq!(ids, ["e"]);
        // The student can still add one.
        let signals = CandidateSignals {
            choices: [("q".to_string(), true)].into(),
            ..signals
        };
        let scored = score(&entries, &signals);
        assert_eq!(scored[0].0, "q");
        assert_eq!(scored[0].1, CandidateReason::StudentAdded);
    }

    #[test]
    fn text_state_and_the_students_choices_decide_what_is_read() {
        let mut scanned = material("scan", MaterialKind::File, "Course outline (scan)");
        scanned.text_status = TextStatus::Ok;
        let mut remote = material("remote", MaterialKind::File, "Course schedule");
        remote.text_status = TextStatus::NotDownloaded;
        let mut locked = material("locked", MaterialKind::File, "Important dates");
        locked.text_status = TextStatus::NotDownloaded;
        locked.download_blocked = Some(DownloadBlock::Locked);
        let removed = material("removed", MaterialKind::File, "Syllabus v1");
        let extra: Vec<Material> = (0..5)
            .map(|i| {
                material(
                    &format!("x{i}"),
                    MaterialKind::File,
                    &format!("Outline part {i}"),
                )
            })
            .collect();
        let mut entries = vec![
            CandidateMaterial {
                chunk_count: 0,
                ..entry(&scanned)
            },
            CandidateMaterial {
                chunk_count: 0,
                ..entry(&remote)
            },
            CandidateMaterial {
                chunk_count: 0,
                ..entry(&locked)
            },
            entry(&removed),
        ];
        entries.extend(extra.iter().map(entry));
        let signals = CandidateSignals {
            choices: [("removed".to_string(), false), ("x4".to_string(), true)].into(),
            ..CandidateSignals::default()
        };
        let scored = score_candidates(&entries, &signals, None, None);
        let by_id = |id: &str| {
            scored
                .iter()
                .find(|s| s.candidate.material_id == id)
                .unwrap()
                .candidate
                .clone()
        };
        assert_eq!(by_id("scan").left_out, Some(CandidateLeftOut::Scanned));
        assert!(!by_id("scan").has_text);
        let remote = by_id("remote");
        assert_eq!(remote.left_out, Some(CandidateLeftOut::NoText));
        assert!(remote.downloadable);
        assert!(!by_id("locked").downloadable, "locked in the LMS");
        let removed = by_id("removed");
        assert_eq!(
            (removed.included, removed.left_out, removed.student_choice),
            (
                false,
                Some(CandidateLeftOut::ExcludedByStudent),
                Some(false)
            )
        );
        // The student's add takes a place first; 3 of the other 4 fit.
        let included: Vec<&str> = scored
            .iter()
            .filter(|s| s.candidate.included)
            .map(|s| s.candidate.material_id.as_str())
            .collect();
        assert_eq!(included, ["x4", "x0", "x1", "x2"]);
        assert_eq!(by_id("x3").left_out, Some(CandidateLeftOut::OverBudget));
    }

    #[test]
    fn announcements_near_the_start_or_about_the_calendar_count_at_most_twice() {
        let mut posts = Vec::new();
        for (id, posted) in [
            ("early", "2026-08-10"),
            ("lead", "2026-08-25"),
            ("welcome", "2026-09-06"),
            ("dates", "2026-10-20"),
            ("late", "2027-02-01"),
        ] {
            let mut post = material(id, MaterialKind::Announcement, "Update");
            post.published_at = Some(at(posted));
            posts.push(post);
        }
        let entries: Vec<CandidateMaterial<'_>> = posts
            .iter()
            .map(|post| CandidateMaterial {
                names_calendar: matches!(post.id.as_str(), "dates" | "late" | "early"),
                ..entry(post)
            })
            .collect();
        let ids: BTreeSet<String> = score(&entries, &CandidateSignals::default())
            .into_iter()
            .map(|s| s.0)
            .collect();
        // "early" (29 days before, a calendar word) and "dates" (42 days after) fit too, but
        // the two closest to the start win.
        assert_eq!(ids, ["lead".to_string(), "welcome".to_string()].into());
        // Without a known start no announcement is a candidate.
        assert!(score_candidates(&entries, &CandidateSignals::default(), None, None).is_empty());
    }

    fn chunk(ord: u32, text: &str) -> Chunk {
        Chunk {
            material_id: "m".into(),
            ord,
            locator: Some(format!("p. {}", ord + 1)),
            text: text.into(),
        }
    }

    #[test]
    fn the_schedule_at_the_end_of_a_long_outline_is_kept_in_order() {
        let filler = "Academic integrity matters in this course. ".repeat(10);
        let chunks = vec![
            chunk(0, &filler),
            chunk(1, &filler),
            chunk(2, "Classes begin September 8. Reading week: October 26-30."),
            chunk(3, &filler),
            chunk(
                4,
                "Week 1 (Sept 8) Basics. Week 2 (Sept 15) Methods. Final exam period Dec 9-20.",
            ),
        ];
        let share = chunks[2].text.len() + chunks[4].text.len() + 10;
        let selection = select_chunks(&chunks, share);
        assert_eq!(selection.chosen, [2, 4]);
        assert_eq!(selection.skipped, [0, 1, 3]);
        // With room for everything, everything is kept.
        assert_eq!(select_chunks(&chunks, 100_000).chosen, [0, 1, 2, 3, 4]);
    }

    #[test]
    fn extra_chunks_need_a_date_and_a_calendar_word() {
        let mut chunks = vec![
            chunk(0, "Today we cover stomata (see page 3)."),
            chunk(1, "Midterm on October 14 in class."),
            chunk(2, "Office hours move to Tuesday."),
        ];
        for ord in 3..15 {
            chunks.push(chunk(ord, "Week 5 starts October 5."));
        }
        let extra = extra_chunks(&chunks);
        assert_eq!(extra.len(), MAX_EXTRA_CHUNKS);
        assert!(extra.iter().all(|(_, ord)| *ord != 0 && *ord != 2));
    }
}
