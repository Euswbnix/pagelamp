//! Checking what a reader extracted against the course's own text (docs/design/
//! v0.3-course-calendar.md §7.5). Shared by the model path, the student's AI app over MCP and
//! the deterministic scan; pure, each step tested.
//!
//! This part covers:
//! - V2 the source resolves to one of the course's materials (`unknown_source`);
//! - V3 the quote (8–300 characters) is in that material's rebuilt text, in matching form
//!   (`text`); a header quote the same (`unsupported_quote`; never retried);
//! - V4 the date parses from the quote (`dates::parse`; a range supports both ends; a week row
//!   needs a week token, or a bare leading number under a Week header found earlier in the
//!   same material; a row's date may instead follow from the other rows, marked `derived`)
//!   (`date_not_in_quote`);
//! - V5 years (§7.5): a year in the quote must match; otherwise the first class's year comes
//!   from a dated claim, the stated term, the session window, the resolved week 1, the LMS
//!   term or today, and every other date takes the smallest year from the first class; a
//!   weekday that fits no year makes the claim a `syllabus_from_another_year` finding (never
//!   recommended), two fitting years drop it (`ambiguous_year`), as does a stated term of
//!   another year;
//! - V6 every date within the outer frame −30 … +45 days (`outside_frame`);
//! - V10 (the part that needs no assembly): no first class at all, or more than half the claims
//!   dropped → the whole run is bad output; a week table losing more than half its rows is
//!   dropped as a whole (`table_dropped`);
//! - V11 labels (80) and topics (120) as plain text.
//!
//! V7 (consistency and conflicts), V8 (cross-checks), V9 (alternatives) and assembly follow.

use std::collections::{BTreeMap, HashMap};
use std::sync::LazyLock;

use chrono::{Datelike, NaiveDate};
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::extraction::{CalendarExtraction, ClaimKind, ExtractedClaim, ExtractedWeek, WeekKind};
use super::text::{TextPart, find_quote, quote_position};
use crate::dates::add_days;
use crate::dates::parse::{
    DateMention, PartialDate, Placement, find_dates, leading_week_number, numeric_order,
    place_after, place_end, place_first, week_number,
};
use crate::model::Timestamp;
use crate::term::DateSpan;
use crate::term::evidence::plain_text;

/// Quotes shorter than this can't tie a date to its text; longer ones are not a quote.
pub const MIN_QUOTE_CHARS: usize = 8;
pub const MAX_QUOTE_CHARS: usize = 300;
/// V11 limits.
pub const MAX_LABEL_CHARS: usize = 80;
pub const MAX_TOPIC_CHARS: usize = 120;
/// V6: dates this far outside the outer frame are dropped.
const FRAME_BEFORE_DAYS: i64 = 30;
const FRAME_AFTER_DAYS: i64 = 45;
/// Without an outer frame: today ± this many days.
const FALLBACK_FRAME_DAYS: i64 = 240;
/// A stated term further than this from the course's own session is another year's.
const STATED_TERM_TOLERANCE_DAYS: i64 = 120;
/// Highest week a schedule row may name.
const MAX_ROW_WEEK: i64 = 40;

/// One of the course's materials, as the validator reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceMaterial {
    pub material_id: String,
    pub title: String,
    pub url: Option<String>,
    pub published_at: Option<Timestamp>,
    /// The text rebuilt from its chunks (`text::rebuild_parts`).
    pub parts: Vec<TextPart>,
}

/// What the validator knows about the course besides its materials.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ValidationContext {
    pub today: Option<NaiveDate>,
    pub outer_frame: Option<DateSpan>,
    /// V5 year hints for the first class, after the extraction's own (in this order).
    pub session_start: Option<NaiveDate>,
    pub week_one_monday: Option<NaiveDate>,
    pub lms_term_start: Option<NaiveDate>,
}

/// Where a date's words are (design §4 `DateEvidence`). The quote is material text: shown to
/// the student only, never in structure outputs (§7.11).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DateEvidence {
    pub material_id: String,
    pub title: String,
    pub locator: Option<String>,
    pub quote: Option<String>,
    pub url: Option<String>,
    /// The date follows from other rows of a validated table, not from this row's words.
    pub derived: bool,
}

/// Why a claim or row was dropped.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DropReason {
    UnknownSource,
    UnsupportedQuote,
    DateNotInQuote,
    AmbiguousYear,
    OutsideFrame,
    Inconsistent,
    TableDropped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DropCount {
    pub reason: DropReason,
    pub count: u32,
}

/// A validated claim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidDate {
    pub kind: ClaimKind,
    pub date: NaiveDate,
    pub end: Option<NaiveDate>,
    pub label: String,
    pub evidence: DateEvidence,
    /// When the material was published (V9: the later one is the default).
    pub published_at: Option<Timestamp>,
}

/// A validated schedule row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidWeek {
    pub week: u32,
    pub starts_on: Option<NaiveDate>,
    pub kind: WeekKind,
    pub topic: Option<String>,
    pub evidence: DateEvidence,
}

/// A claim whose weekday fits no year near the course (V5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnotherYear {
    pub kind: Option<ClaimKind>,
    pub evidence: DateEvidence,
}

/// The result of checking one extraction.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Validated {
    pub dates: Vec<ValidDate>,
    pub weeks: Vec<ValidWeek>,
    pub dropped: Vec<DropCount>,
    /// V5 findings: dates or the stated term from another year's syllabus.
    pub another_year: Vec<AnotherYear>,
    pub stated_term_another_year: bool,
    /// V10: nothing gives the first class, or more than half the claims were dropped.
    pub bad_output: bool,
    /// V10: the week table lost more than half its rows and was dropped whole.
    pub table_dropped: bool,
}

/// Check `extraction` against `sources` (keyed by the handle or material id the reader used).
pub fn validate(
    extraction: &CalendarExtraction,
    sources: &HashMap<String, SourceMaterial>,
    ctx: &ValidationContext,
) -> Validated {
    let mut out = Validated::default();
    let mut dropped: BTreeMap<DropReason, u32> = BTreeMap::new();
    let mut drop = |reason: DropReason| *dropped.entry(reason).or_default() += 1;
    let today = ctx.today.unwrap_or(NaiveDate::MIN);

    // V2–V4: claims.
    let mut claims: Vec<(&ExtractedClaim, &SourceMaterial, Found)> = Vec::new();
    for claim in &extraction.claims {
        match check_claim(claim, sources) {
            Ok((source, found)) => claims.push((claim, source, found)),
            Err(reason) => drop(reason),
        }
    }
    // V2–V4: rows (a row's own date, if any; derived dates come after placement).
    let mut rows: Vec<(&ExtractedWeek, &SourceMaterial, u32, Option<Found>)> = Vec::new();
    let mut rows_dropped = 0usize;
    for row in &extraction.weeks {
        match check_row(row, sources) {
            Ok((source, week, found)) => rows.push((row, source, week, found)),
            Err(reason) => {
                drop(reason);
                rows_dropped += 1;
            }
        }
    }

    // V5: the anchor for the first class's year.
    let explicit = claims
        .iter()
        .map(|(_, _, found)| found.start)
        .chain(
            rows.iter()
                .filter_map(|(_, _, _, found)| found.as_ref().map(|f| f.start)),
        )
        .find_map(|partial| {
            partial
                .year
                .and_then(|y| NaiveDate::from_ymd_opt(y, partial.month, partial.day))
        });
    let stated = extraction
        .stated_term
        .text
        .as_deref()
        .and_then(stated_term_start);
    let course_reference = ctx
        .session_start
        .or(ctx.week_one_monday)
        .or(ctx.lms_term_start);
    if let (Some(stated), Some(reference)) = (stated, course_reference)
        && (stated - reference).num_days().abs() > STATED_TERM_TOLERANCE_DAYS
    {
        out.stated_term_another_year = true;
    }
    let anchor = explicit
        .or(stated.filter(|_| !out.stated_term_another_year))
        .or(course_reference)
        .or(ctx.today)
        .unwrap_or(NaiveDate::MIN);

    // The first class: the earliest first-class claim, else a dated week 0/1 row.
    let first_candidates = claims
        .iter()
        .filter(|(claim, _, _)| claim.kind == ClaimKind::FirstClass)
        .map(|(_, _, found)| found.start)
        .chain(
            rows.iter()
                .filter(|(_, _, week, found)| *week <= 1 && found.is_some())
                .filter_map(|(_, _, _, found)| found.as_ref().map(|f| f.start)),
        );
    let first_class = first_candidates
        .filter_map(|partial| match place_first(partial, anchor) {
            Placement::Date(date) => Some(date),
            _ => None,
        })
        .min();
    let reference = first_class.unwrap_or(anchor);

    let frame = ctx.outer_frame.unwrap_or(DateSpan {
        start: add_days(today, -FALLBACK_FRAME_DAYS),
        end: add_days(today, FALLBACK_FRAME_DAYS),
    });
    let in_frame = |date: NaiveDate| {
        date >= add_days(frame.start, -FRAME_BEFORE_DAYS)
            && date <= add_days(frame.end, FRAME_AFTER_DAYS)
    };

    // V5, V6 and V11 for claims.
    for (claim, source, found) in claims {
        let placed = if claim.kind == ClaimKind::FirstClass {
            place_first(found.start, anchor)
        } else {
            place_after(found.start, reference)
        };
        let evidence = evidence(source, &found, &claim.quote, false);
        let date = match placed {
            Placement::Date(date) => date,
            Placement::WeekdayMismatch => {
                out.another_year.push(AnotherYear {
                    kind: Some(claim.kind),
                    evidence,
                });
                continue;
            }
            Placement::AmbiguousYear => {
                drop(DropReason::AmbiguousYear);
                continue;
            }
            Placement::Invalid => {
                drop(DropReason::DateNotInQuote);
                continue;
            }
        };
        let end = match found.end.map(|end| place_end(end, date)) {
            None => None,
            Some(Placement::Date(end)) => Some(end),
            Some(Placement::WeekdayMismatch) => {
                out.another_year.push(AnotherYear {
                    kind: Some(claim.kind),
                    evidence,
                });
                continue;
            }
            Some(_) => {
                drop(DropReason::DateNotInQuote);
                continue;
            }
        };
        if !in_frame(date) || end.is_some_and(|end| !in_frame(end)) {
            drop(DropReason::OutsideFrame);
            continue;
        }
        out.dates.push(ValidDate {
            kind: claim.kind,
            date,
            end,
            label: clean(&claim.label, MAX_LABEL_CHARS),
            evidence,
            published_at: source.published_at,
        });
    }

    // V5, V6 and V11 for rows; dates that aren't in a row's words follow from the others.
    let mut placed_rows: Vec<ValidWeek> = Vec::new();
    let mut undated: Vec<(&ExtractedWeek, &SourceMaterial, u32)> = Vec::new();
    for (row, source, week, found) in rows {
        let Some(found) = found else {
            undated.push((row, source, week));
            continue;
        };
        let evidence = evidence(source, &found, &row.quote, false);
        match place_after(found.start, reference) {
            Placement::Date(date) if in_frame(date) => placed_rows.push(ValidWeek {
                week,
                starts_on: Some(date),
                kind: row.kind,
                topic: row.topic.as_deref().map(|t| clean(t, MAX_TOPIC_CHARS)),
                evidence,
            }),
            Placement::Date(_) => {
                drop(DropReason::OutsideFrame);
                rows_dropped += 1;
            }
            Placement::WeekdayMismatch => {
                out.another_year.push(AnotherYear {
                    kind: None,
                    evidence,
                });
                rows_dropped += 1;
            }
            Placement::AmbiguousYear => {
                drop(DropReason::AmbiguousYear);
                rows_dropped += 1;
            }
            Placement::Invalid => {
                drop(DropReason::DateNotInQuote);
                rows_dropped += 1;
            }
        }
    }
    for (row, source, week) in undated {
        let derived = derive_start(&placed_rows, week);
        let claimed = row.starts_on.as_deref().and_then(parse_claimed);
        let starts_on = match (derived, claimed) {
            // The reader gave a date the table's other rows don't support.
            (Some(derived), Some((_, month, day)))
                if (derived.month(), derived.day()) != (month, day) =>
            {
                drop(DropReason::DateNotInQuote);
                rows_dropped += 1;
                continue;
            }
            (derived, _) => derived,
        };
        placed_rows.push(ValidWeek {
            week,
            starts_on,
            kind: row.kind,
            topic: row.topic.as_deref().map(|t| clean(t, MAX_TOPIC_CHARS)),
            evidence: DateEvidence {
                material_id: source.material_id.clone(),
                title: source.title.clone(),
                locator: find_quote(&source.parts, &row.quote)
                    .and_then(|m| m.part)
                    .and_then(|i| source.parts[i].locator.clone()),
                quote: Some(row.quote.clone()),
                url: source.url.clone(),
                derived: starts_on.is_some(),
            },
        });
    }
    placed_rows.sort_by_key(|row| row.week);

    // V10: a table that lost more than half its rows goes as a whole.
    let total_rows = extraction.weeks.len();
    if total_rows > 0 && rows_dropped * 2 > total_rows {
        *dropped.entry(DropReason::TableDropped).or_default() +=
            u32::try_from(placed_rows.len()).unwrap_or(u32::MAX);
        placed_rows.clear();
        out.table_dropped = true;
    }
    out.weeks = placed_rows;

    // V10: the run as a whole.
    let has_first_class = out.dates.iter().any(|d| d.kind == ClaimKind::FirstClass)
        || out
            .weeks
            .iter()
            .any(|w| w.week <= 1 && w.starts_on.is_some());
    let claims_total = extraction.claims.len();
    let claims_kept =
        out.dates.len() + out.another_year.iter().filter(|a| a.kind.is_some()).count();
    let claims_lost = claims_total.saturating_sub(claims_kept);
    out.bad_output = !has_first_class || (claims_total > 0 && claims_lost * 2 > claims_total);

    out.dropped = dropped
        .into_iter()
        .map(|(reason, count)| DropCount { reason, count })
        .collect();
    out
}

/// A quote and the date(s) it states.
#[derive(Clone, Debug)]
struct Found {
    start: PartialDate,
    end: Option<PartialDate>,
    part: Option<usize>,
}

/// V2–V4 for one claim.
fn check_claim<'a>(
    claim: &ExtractedClaim,
    sources: &'a HashMap<String, SourceMaterial>,
) -> Result<(&'a SourceMaterial, Found), DropReason> {
    let source = sources
        .get(&claim.source)
        .ok_or(DropReason::UnknownSource)?;
    let part = supported_quote(source, &claim.quote)?;
    let claimed = parse_claimed(&claim.date).ok_or(DropReason::DateNotInQuote)?;
    let claimed_end = match &claim.end_date {
        Some(end) => Some(parse_claimed(end).ok_or(DropReason::DateNotInQuote)?),
        None => None,
    };
    let order = numeric_order(&material_text(source));
    let mentions = find_dates(&claim.quote, order);
    let (start, end) =
        match_mentions(&mentions, claimed, claimed_end).ok_or(DropReason::DateNotInQuote)?;
    Ok((source, Found { start, end, part }))
}

/// V2–V4 for one row: the week number, and the row's own date when its words state it.
fn check_row<'a>(
    row: &ExtractedWeek,
    sources: &'a HashMap<String, SourceMaterial>,
) -> Result<(&'a SourceMaterial, u32, Option<Found>), DropReason> {
    let source = sources.get(&row.source).ok_or(DropReason::UnknownSource)?;
    let part = supported_quote(source, &row.quote)?;
    if !(0..=MAX_ROW_WEEK).contains(&row.week) {
        return Err(DropReason::DateNotInQuote);
    }
    let week = u32::try_from(row.week).map_err(|_| DropReason::DateNotInQuote)?;
    // The week: a token in the row, or a bare leading number under a Week header found in
    // the same material before the row (review finding 4).
    let header = match &row.header_quote {
        Some(header) => {
            supported_quote(source, header)?;
            let before = match (
                quote_position(&source.parts, header),
                quote_position(&source.parts, &row.quote),
            ) {
                (Some(h), Some(r)) => h < r,
                _ => false,
            };
            before.then_some(header.as_str())
        }
        None => None,
    };
    if leading_week_number(&row.quote, header) != Some(week)
        && week_number(&row.quote) != Some(week)
    {
        return Err(DropReason::DateNotInQuote);
    }
    let found = match row.starts_on.as_deref().map(parse_claimed) {
        Some(Some(claimed)) => {
            let order = numeric_order(&material_text(source));
            let mentions = find_dates(&row.quote, order);
            match_mentions(&mentions, claimed, None).map(|(start, _)| Found {
                start,
                end: None,
                part,
            })
        }
        Some(None) => return Err(DropReason::DateNotInQuote),
        None => None,
    };
    Ok((source, week, found))
}

/// V3: the quote's length, and where it is in the material.
fn supported_quote(source: &SourceMaterial, quote: &str) -> Result<Option<usize>, DropReason> {
    let length = quote.trim().chars().count();
    if !(MIN_QUOTE_CHARS..=MAX_QUOTE_CHARS).contains(&length) {
        return Err(DropReason::UnsupportedQuote);
    }
    find_quote(&source.parts, quote)
        .map(|found| found.part)
        .ok_or(DropReason::UnsupportedQuote)
}

fn material_text(source: &SourceMaterial) -> String {
    source
        .parts
        .iter()
        .map(|part| part.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// "2026-09-08", "09-08" or "--09-08" → (year, month, day).
fn parse_claimed(text: &str) -> Option<(Option<i32>, u32, u32)> {
    static CLAIMED: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^\s*(?:(?P<y>\d{4})-|--)?(?P<m>\d{1,2})-(?P<d>\d{1,2})\s*$")
            .expect("valid regex")
    });
    let captures = CLAIMED.captures(text)?;
    let year = captures.name("y").and_then(|m| m.as_str().parse().ok());
    let month: u32 = captures["m"].parse().ok()?;
    let day: u32 = captures["d"].parse().ok()?;
    ((1..=12).contains(&month) && (1..=31).contains(&day)).then_some((year, month, day))
}

/// The mention(s) in the quote that state the claimed date (and end): a range whose ends
/// match, or two separate dates. A year written in the quote must equal a claimed one.
fn match_mentions(
    mentions: &[DateMention],
    claimed: (Option<i32>, u32, u32),
    claimed_end: Option<(Option<i32>, u32, u32)>,
) -> Option<(PartialDate, Option<PartialDate>)> {
    let same = |partial: &PartialDate, (year, month, day): (Option<i32>, u32, u32)| {
        (partial.month, partial.day) == (month, day)
            && match (partial.year, year) {
                (Some(a), Some(b)) => a == b,
                _ => true,
            }
    };
    let start = mentions.iter().find(|m| same(&m.start, claimed))?;
    match claimed_end {
        None => Some((start.start, None)),
        Some(end) => {
            if let Some(range_end) = start.end.filter(|e| same(e, end)) {
                return Some((start.start, Some(range_end)));
            }
            let separate = mentions.iter().find(|m| same(&m.start, end))?;
            Some((start.start, Some(separate.start)))
        }
    }
}

/// A row's start from the other rows: week 1 on a row's date shifts by 7 days a week.
fn derive_start(rows: &[ValidWeek], week: u32) -> Option<NaiveDate> {
    let nearest = rows
        .iter()
        .filter(|row| row.starts_on.is_some() && !row.evidence.derived)
        .min_by_key(|row| (i64::from(row.week) - i64::from(week)).abs())?;
    let offset = (i64::from(week) - i64::from(nearest.week)) * 7;
    Some(add_days(nearest.starts_on?, offset))
}

fn evidence(source: &SourceMaterial, found: &Found, quote: &str, derived: bool) -> DateEvidence {
    DateEvidence {
        material_id: source.material_id.clone(),
        title: source.title.clone(),
        locator: found
            .part
            .and_then(|i| source.parts.get(i))
            .and_then(|part| part.locator.clone()),
        quote: Some(quote.trim().to_string()),
        url: source.url.clone(),
        derived,
    }
}

/// V11: plain text, at most `max` characters.
fn clean(text: &str, max: usize) -> String {
    let plain = plain_text(text);
    if plain.chars().count() <= max {
        return plain;
    }
    let mut cut: String = plain.chars().take(max).collect();
    cut.push('…');
    cut
}

/// The first day of a stated term such as "Fall 2026", "Winter 2027", "Summer 2026",
/// "2026 Fall", "2026秋季".
fn stated_term_start(text: &str) -> Option<NaiveDate> {
    static TERM: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)(?P<s1>fall|autumn|winter|spring|summer|秋|冬|春|夏)\D{0,3}(?P<y1>(?:19|20)\d{2})|(?P<y2>(?:19|20)\d{2})\D{0,3}(?P<s2>fall|autumn|winter|spring|summer|秋|冬|春|夏)")
            .expect("valid regex")
    });
    let captures = TERM.captures(text)?;
    let season = captures
        .name("s1")
        .or_else(|| captures.name("s2"))?
        .as_str()
        .to_lowercase();
    let year: i32 = captures
        .name("y1")
        .or_else(|| captures.name("y2"))?
        .as_str()
        .parse()
        .ok()?;
    let month = match season.as_str() {
        "fall" | "autumn" | "秋" => 9,
        "winter" | "冬" => 1,
        "spring" | "春" => 1,
        _ => 5,
    };
    NaiveDate::from_ymd_opt(year, month, 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::extraction::StatedTerm;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    /// The synthetic outline every test reads.
    const OUTLINE: &str = "DEM332 Demo Methods, Fall 2026. Classes begin Tuesday, September 8. \
        Reading week: Oct 26–30 (no classes). Last day of classes: Dec 8. \
        Final exam period: December 10-22.";

    const SCHEDULE: &str = "Schedule\nWeek | Topic | Date\n1 Demo basics Sept 8\n2 Sampling \
        Sept 15\n3 Methods\nWeek 4: Review (Sept 29)";

    fn sources() -> HashMap<String, SourceMaterial> {
        let material = |id: &str, title: &str, locator: &str, text: &str| {
            (
                id.to_string(),
                SourceMaterial {
                    material_id: format!("demo/file/{id}"),
                    title: title.into(),
                    url: None,
                    published_at: None,
                    parts: vec![TextPart {
                        locator: Some(locator.into()),
                        text: text.into(),
                    }],
                },
            )
        };
        HashMap::from([
            material("m1", "Course outline", "p. 1", OUTLINE),
            material("m2", "Schedule", "§ Schedule", SCHEDULE),
        ])
    }

    fn context() -> ValidationContext {
        ValidationContext {
            today: Some(date(2026, 9, 28)),
            outer_frame: Some(DateSpan {
                start: date(2026, 5, 4),
                end: date(2027, 1, 31),
            }),
            session_start: Some(date(2026, 9, 1)),
            week_one_monday: None,
            lms_term_start: Some(date(2026, 5, 4)),
        }
    }

    fn claim(kind: ClaimKind, date: &str, end: Option<&str>, quote: &str) -> ExtractedClaim {
        ExtractedClaim {
            kind,
            date: date.into(),
            end_date: end.map(Into::into),
            label: "Label".into(),
            quote: quote.into(),
            source: "m1".into(),
        }
    }

    fn extraction(claims: Vec<ExtractedClaim>, weeks: Vec<ExtractedWeek>) -> CalendarExtraction {
        CalendarExtraction {
            stated_term: StatedTerm {
                text: Some("Fall 2026".into()),
                quote: Some("Fall 2026".into()),
                source: Some("m1".into()),
            },
            claims,
            weeks,
            not_found: Vec::new(),
        }
    }

    fn dropped(result: &Validated, reason: DropReason) -> u32 {
        result
            .dropped
            .iter()
            .find(|d| d.reason == reason)
            .map_or(0, |d| d.count)
    }

    fn good_claims() -> Vec<ExtractedClaim> {
        vec![
            claim(
                ClaimKind::FirstClass,
                "2026-09-08",
                None,
                "Classes begin Tuesday, September 8",
            ),
            claim(
                ClaimKind::Break,
                "10-26",
                Some("10-30"),
                "Reading week: Oct 26–30 (no classes)",
            ),
            claim(
                ClaimKind::LastClass,
                "12-08",
                None,
                "Last day of classes: Dec 8",
            ),
            claim(
                ClaimKind::ExamPeriod,
                "12-10",
                Some("12-22"),
                "Final exam period: December 10-22",
            ),
        ]
    }

    #[test]
    fn claims_with_their_quotes_are_placed_in_the_course_year() {
        let result = validate(
            &extraction(good_claims(), Vec::new()),
            &sources(),
            &context(),
        );
        assert!(!result.bad_output, "{result:?}");
        assert!(result.dropped.is_empty(), "{:?}", result.dropped);
        let by_kind = |kind| result.dates.iter().find(|d| d.kind == kind).unwrap();
        assert_eq!(by_kind(ClaimKind::FirstClass).date, date(2026, 9, 8));
        let reading = by_kind(ClaimKind::Break);
        assert_eq!(
            (reading.date, reading.end),
            (date(2026, 10, 26), Some(date(2026, 10, 30)))
        );
        assert_eq!(by_kind(ClaimKind::LastClass).date, date(2026, 12, 8));
        assert_eq!(by_kind(ClaimKind::ExamPeriod).end, Some(date(2026, 12, 22)));
        let evidence = &by_kind(ClaimKind::FirstClass).evidence;
        assert_eq!(evidence.material_id, "demo/file/m1");
        assert_eq!(evidence.locator.as_deref(), Some("p. 1"));
        assert!(!evidence.derived);
    }

    #[test]
    fn invented_quotes_unknown_sources_and_dates_not_in_the_quote_are_dropped() {
        let mut claims = good_claims();
        claims.push(claim(
            ClaimKind::FinalExam,
            "12-15",
            None,
            "Final exam: December 15", // not in the outline
        ));
        claims.push(ExtractedClaim {
            source: "m9".into(),
            ..claim(ClaimKind::FinalExam, "12-15", None, "Final exam period")
        });
        // The quote exists but says another date.
        claims.push(claim(
            ClaimKind::LastClass,
            "12-09",
            None,
            "Last day of classes: Dec 8",
        ));
        // Too short to tie a date to its words.
        claims.push(claim(ClaimKind::FinalExam, "12-10", None, "Dec 10"));
        let result = validate(&extraction(claims, Vec::new()), &sources(), &context());
        assert_eq!(dropped(&result, DropReason::UnsupportedQuote), 2);
        assert_eq!(dropped(&result, DropReason::UnknownSource), 1);
        assert_eq!(dropped(&result, DropReason::DateNotInQuote), 1);
        assert_eq!(result.dates.len(), 4);
        assert!(!result.bad_output, "4 of 8 dropped is not more than half");
    }

    #[test]
    fn more_than_half_dropped_or_no_first_class_is_bad_output() {
        let mut claims = good_claims();
        for i in 0..5 {
            claims.push(claim(
                ClaimKind::FinalExam,
                "12-15",
                None,
                &format!("Invented exam text number {i}"),
            ));
        }
        let result = validate(&extraction(claims, Vec::new()), &sources(), &context());
        assert!(result.bad_output);
        let without_first = good_claims().into_iter().skip(1).collect();
        let result = validate(
            &extraction(without_first, Vec::new()),
            &sources(),
            &context(),
        );
        assert!(result.bad_output, "no first class");
    }

    /// CAL-42 (the validator part): last year's syllabus.
    #[test]
    fn last_years_syllabus_is_flagged() {
        let mut sources = sources();
        sources.get_mut("m1").unwrap().parts[0].text =
            "Classes begin Tuesday, September 9. Last day of classes: Dec 9.".into();
        let claims = vec![
            claim(
                ClaimKind::FirstClass,
                "09-09",
                None,
                "Classes begin Tuesday, September 9",
            ),
            claim(
                ClaimKind::LastClass,
                "12-09",
                None,
                "Last day of classes: Dec 9",
            ),
        ];
        let result = validate(&extraction(claims, Vec::new()), &sources, &context());
        assert_eq!(result.another_year.len(), 1);
        assert_eq!(result.another_year[0].kind, Some(ClaimKind::FirstClass));
        // A stated term of another year is flagged too.
        let mut stale = extraction(good_claims(), Vec::new());
        stale.stated_term.text = Some("Fall 2025".into());
        let result = validate(&stale, &super::tests::sources(), &context());
        assert!(result.stated_term_another_year);
        assert_eq!(
            result.dates[0].date.year(),
            2026,
            "the course's year still wins"
        );
    }

    #[test]
    fn dates_far_outside_the_frame_are_dropped() {
        let mut sources = sources();
        sources.get_mut("m1").unwrap().parts[0].text =
            "Classes begin Tuesday, September 8, 2026. Makeup test: 2027-06-15.".into();
        let claims = vec![
            claim(
                ClaimKind::FirstClass,
                "2026-09-08",
                None,
                "Classes begin Tuesday, September 8, 2026",
            ),
            claim(
                ClaimKind::FinalExam,
                "2027-06-15",
                None,
                "Makeup test: 2027-06-15",
            ),
        ];
        let result = validate(&extraction(claims, Vec::new()), &sources, &context());
        assert_eq!(dropped(&result, DropReason::OutsideFrame), 1);
    }

    fn row(week: i64, starts_on: Option<&str>, quote: &str, header: Option<&str>) -> ExtractedWeek {
        ExtractedWeek {
            week,
            starts_on: starts_on.map(Into::into),
            kind: WeekKind::Teaching,
            topic: Some("A topic\nwith a line break".into()),
            quote: quote.into(),
            header_quote: header.map(Into::into),
            source: "m2".into(),
        }
    }

    /// CAL-37 (the validator part): table rows with bare numbers use the header quote.
    #[test]
    fn schedule_rows_use_the_header_quote_and_derive_dates() {
        let weeks = vec![
            row(
                1,
                Some("09-08"),
                "1 Demo basics Sept 8",
                Some("Week | Topic | Date"),
            ),
            row(
                2,
                Some("09-15"),
                "2 Sampling Sept 15",
                Some("Week | Topic | Date"),
            ),
            row(3, Some("09-22"), "3 Methods", Some("Week | Topic | Date")),
            row(4, Some("09-29"), "Week 4: Review (Sept 29)", None),
        ];
        let result = validate(&extraction(good_claims(), weeks), &sources(), &context());
        assert!(!result.table_dropped);
        let rows: Vec<(u32, Option<NaiveDate>, bool)> = result
            .weeks
            .iter()
            .map(|w| (w.week, w.starts_on, w.evidence.derived))
            .collect();
        assert_eq!(
            rows,
            [
                (1, Some(date(2026, 9, 8)), false),
                (2, Some(date(2026, 9, 15)), false),
                (3, Some(date(2026, 9, 22)), true),
                (4, Some(date(2026, 9, 29)), false),
            ]
        );
        assert_eq!(
            result.weeks[0].topic.as_deref(),
            Some("A topic with a line break")
        );
    }

    #[test]
    fn a_table_losing_most_rows_is_dropped_whole() {
        let weeks = vec![
            row(1, Some("09-08"), "1 Demo basics Sept 8", None), // no header: bare number
            row(2, Some("09-15"), "2 Sampling Sept 15", None),
            row(3, None, "3 Methods", None),
            row(4, Some("09-29"), "Week 4: Review (Sept 29)", None),
        ];
        let result = validate(&extraction(good_claims(), weeks), &sources(), &context());
        assert!(result.table_dropped);
        assert!(result.weeks.is_empty());
        assert_eq!(dropped(&result, DropReason::TableDropped), 1);
        assert!(!result.bad_output, "the claims still give the first class");
    }

    #[test]
    fn a_header_after_the_row_does_not_count() {
        let mut sources = sources();
        sources.get_mut("m2").unwrap().parts[0].text =
            "1 Demo basics Sept 8\nWeek | Topic | Date".into();
        let weeks = vec![row(
            1,
            Some("09-08"),
            "1 Demo basics Sept 8",
            Some("Week | Topic | Date"),
        )];
        let result = validate(&extraction(good_claims(), weeks), &sources, &context());
        assert!(result.weeks.is_empty());
    }

    #[test]
    fn labels_are_plain_and_short() {
        let mut claims = good_claims();
        claims[1].label = format!("Reading\nweek {}", "x".repeat(200));
        let result = validate(&extraction(claims, Vec::new()), &sources(), &context());
        let label = &result
            .dates
            .iter()
            .find(|d| d.kind == ClaimKind::Break)
            .unwrap()
            .label;
        assert!(!label.contains('\n'));
        assert!(label.chars().count() <= MAX_LABEL_CHARS + 1);
    }

    #[test]
    fn stated_terms_parse() {
        assert_eq!(stated_term_start("Fall 2026"), Some(date(2026, 9, 1)));
        assert_eq!(
            stated_term_start("2027 Winter term"),
            Some(date(2027, 1, 1))
        );
        assert_eq!(stated_term_start("2026 秋季"), Some(date(2026, 9, 1)));
        assert_eq!(stated_term_start("Summer 2026 (F)"), Some(date(2026, 5, 1)));
        assert_eq!(stated_term_start("the course"), None);
    }
}
