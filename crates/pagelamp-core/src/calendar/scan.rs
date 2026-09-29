//! The deterministic syllabus scan (docs/design/v0.3-course-calendar.md §7.2): no model, the
//! same output shape as a reader, checked by the same validator. It works for every course,
//! including `prohibited` ones (rule 8 is about AI reading text; this is local).
//!
//! - A line (or a table row) with a keyword class and a date gives one claim per keyword:
//!   first class ("classes begin", "first lecture", "开课"), last class ("last day of classes",
//!   "classes end"), break ("reading week", "study break", "no class", "holiday", "阅读周"),
//!   exam period ("final exam period", "exam period", "考试周"; needs a range) and final exam
//!   ("final exam" with one date). A keyword takes the first date after it on the line, else
//!   the nearest before it. The quote is the line (or the part of it around the keyword and
//!   the date, up to 300 characters).
//! - Schedule rows give week/date pairs: a line starting with a week token ("Week 3 (Sept
//!   21)"), or a bare leading number under a header line naming the Week column. Tables from
//!   HTML pages have one cell per line ("Week", "Topic", "Date", "1", "Demo basics", "Sept
//!   8"); their cells are joined back into rows by the header's column count (quote matching
//!   treats line breaks as spaces). Week 0 is accepted.
//! - The stated term: the first "Fall 2026"-style phrase.

use std::collections::{BTreeSet, HashMap};
use std::sync::LazyLock;

use regex::Regex;

use super::extraction::{
    CalendarExtraction, ClaimKind, ExtractedClaim, ExtractedWeek, NotFound, StatedTerm, WeekKind,
};
use super::validate::{MAX_QUOTE_CHARS, MIN_QUOTE_CHARS, SourceMaterial};
use crate::dates::parse::{
    NumericOrder, PartialDate, find_dates, normalise, numeric_order, week_number,
};

/// How far (in bytes of the line) a keyword looks for its date.
const KEYWORD_REACH: usize = 80;
/// Context kept on each side when a long line is cut to a quote.
const QUOTE_CONTEXT: usize = 40;
/// Header cells of a one-cell-per-line table: at most this many, each this short.
const MAX_HEADER_CELLS: usize = 6;
const MAX_HEADER_CELL_CHARS: usize = 40;
/// Highest week a scanned row may name.
const MAX_ROW_WEEK: u32 = 40;

fn regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("valid scan regex")
}

/// Keyword classes, tried on every line. Word boundaries are ASCII-only so Chinese text next
/// to a keyword doesn't hide it.
static KEYWORDS: LazyLock<Vec<(ClaimKind, Regex)>> = LazyLock::new(|| {
    vec![
        (
            ClaimKind::FirstClass,
            regex(
                r"(?i)(?-u:\b)(?:classes\s+(?:begin|start)|first\s+(?:class|lecture|day\s+of\s+classes)|lectures?\s+(?:begin|start))(?-u:\b)|开课|第一次课|首次课",
            ),
        ),
        (
            ClaimKind::LastClass,
            regex(
                r"(?i)(?-u:\b)(?:last\s+day\s+of\s+classes|last\s+(?:class|lecture)|classes\s+end|lectures\s+end)(?-u:\b)|最后一次课|最后一节课",
            ),
        ),
        (
            ClaimKind::ExamPeriod,
            regex(
                r"(?i)(?-u:\b)(?:(?:final\s+)?exam(?:ination)?\s+(?:period|weeks?)|final\s+assessment\s+period)(?-u:\b)|期末考试周|考试周|考試週",
            ),
        ),
        (
            ClaimKind::FinalExam,
            regex(r"(?i)(?-u:\b)final\s+exam(?:ination)?(?-u:\b)|期末考试"),
        ),
        (
            ClaimKind::Break,
            regex(
                r"(?i)(?-u:\b)(?:reading\s+(?:week|break)|study\s+break|fall\s+break|winter\s+break|spring\s+break|mid-?term\s+break|no\s+(?:class|classes|lectures?|tutorials?)|holiday|thanksgiving|family\s+day)(?-u:\b)|阅读周|閱讀週|放假|停课",
            ),
        ),
    ]
});

/// How many keyword-class phrases `text` has (candidate scoring counts them as calendar words).
pub(super) fn keyword_count(text: &str) -> usize {
    KEYWORDS
        .iter()
        .map(|(_, keyword)| keyword.find_iter(text).count())
        .sum()
}

/// "Fall 2026", "Winter term 2027", "2026 Fall".
static STATED_TERM: LazyLock<Regex> = LazyLock::new(|| {
    regex(
        r"(?i)(?-u:\b)(?:(?:fall|autumn|winter|spring|summer)(?:\s+term)?\s+(?:19|20)\d{2}|(?:19|20)\d{2}\s+(?:fall|autumn|winter|spring|summer))(?-u:\b)",
    )
});

/// A row that starts with a week token.
static ROW_WEEK_TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    regex(r"(?i)^\s*(?:(?:week|wk)\s*[-–—.:#]?\s*\d{1,2}(?-u:\b)|第\s*\d{1,2}\s*[周週])")
});

/// A bare leading number.
static ROW_NUMBER: LazyLock<Regex> = LazyLock::new(|| regex(r"^\s*(\d{1,2})(?:\s|$|[.)|:\t])"));

/// A one-line header naming the week column, with other columns after a separator.
static HEADER_LINE: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(?i)^\s*(?:week|wk|周次|周|週)(?-u:\b)?\s*(?:\||\t|\s{2,}|,)\s*\S"));

/// The first cell of a one-cell-per-line table's header.
static HEADER_CELL: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(?i)^\s*(?:week|wk|周次|周|週)\s*$"));

/// Scan `sources` (keyed by the handle the claims will name, e.g. the material id).
pub fn scan(sources: &HashMap<String, SourceMaterial>) -> CalendarExtraction {
    let mut claims: Vec<ExtractedClaim> = Vec::new();
    let mut weeks: Vec<ExtractedWeek> = Vec::new();
    let mut stated_term = StatedTerm::default();
    let mut keys: Vec<&String> = sources.keys().collect();
    keys.sort();
    for key in keys {
        let source = &sources[key];
        let all_text: String = source
            .parts
            .iter()
            .map(|p| p.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let order = numeric_order(&all_text);
        if stated_term.text.is_none()
            && let Some(found) = STATED_TERM.find(&all_text)
        {
            stated_term = StatedTerm {
                text: Some(found.as_str().to_string()),
                quote: Some(found.as_str().to_string()),
                source: Some(key.clone()),
            };
        }
        for part in &source.parts {
            let lines: Vec<String> = part.text.lines().map(normalise).collect();
            let mut index = 0;
            let mut header: Option<String> = None;
            while index < lines.len() {
                let line = lines[index].trim();
                // A one-cell-per-line table: "Week" followed by its other header cells.
                if HEADER_CELL.is_match(line)
                    && let Some((columns, header_quote)) = cell_header(&lines, index)
                {
                    index += columns;
                    while let Some(row) = cell_row(&lines, index, columns) {
                        push_row(
                            &row,
                            Some(&header_quote),
                            order,
                            key,
                            &mut weeks,
                            &mut claims,
                        );
                        index += columns;
                    }
                    continue;
                }
                if line.is_empty() {
                    header = None;
                } else if HEADER_LINE.is_match(line) {
                    header = Some(line.to_string());
                } else if ROW_WEEK_TOKEN.is_match(line) {
                    push_row(line, None, order, key, &mut weeks, &mut claims);
                } else if header.is_some() && ROW_NUMBER.is_match(line) {
                    push_row(line, header.as_deref(), order, key, &mut weeks, &mut claims);
                } else {
                    header = None;
                    claims.extend(line_claims(line, order, key));
                }
                index += 1;
            }
        }
    }
    dedupe(&mut claims);
    let not_found = not_found(&claims, &weeks);
    CalendarExtraction {
        stated_term,
        claims,
        weeks,
        not_found,
    }
}

/// The header of a one-cell-per-line table starting at `index` ("Week"): its column count and
/// its cells joined by spaces, when rows follow.
fn cell_header(lines: &[String], index: usize) -> Option<(usize, String)> {
    let mut cells = vec![lines[index].trim().to_string()];
    for line in lines.iter().skip(index + 1).take(MAX_HEADER_CELLS) {
        let cell = line.trim();
        if cell.is_empty()
            || ROW_NUMBER.is_match(cell)
            || cell.chars().count() > MAX_HEADER_CELL_CHARS
        {
            break;
        }
        cells.push(cell.to_string());
    }
    let columns = cells.len();
    let first_row = lines.get(index + columns)?.trim();
    (columns >= 2 && ROW_NUMBER.is_match(first_row)).then(|| (columns, cells.join(" ")))
}

/// The row of a one-cell-per-line table at `index`: `columns` cells joined by spaces, when the
/// first is a bare week number.
fn cell_row(lines: &[String], index: usize, columns: usize) -> Option<String> {
    let cells = lines.get(index..index + columns)?;
    let first = cells[0].trim();
    let number: u32 = ROW_NUMBER.captures(first)?[1].parse().ok()?;
    if number > MAX_ROW_WEEK || first.chars().any(|c| c.is_alphabetic()) {
        return None;
    }
    Some(cells.iter().map(|c| c.trim()).collect::<Vec<_>>().join(" "))
}

/// One schedule row (and any keyword claims on it).
fn push_row(
    row: &str,
    header: Option<&str>,
    order: Option<NumericOrder>,
    source: &str,
    weeks: &mut Vec<ExtractedWeek>,
    claims: &mut Vec<ExtractedClaim>,
) {
    let row = row.trim();
    let length = row.chars().count();
    if !(MIN_QUOTE_CHARS..=MAX_QUOTE_CHARS).contains(&length) {
        return;
    }
    let week = week_number(row)
        .or_else(|| header.and_then(|_| ROW_NUMBER.captures(row)?[1].parse::<u32>().ok()));
    let Some(week) = week.filter(|w| *w <= MAX_ROW_WEEK) else {
        return;
    };
    let starts_on = find_dates(row, order).first().map(|m| date_text(&m.start));
    let row_claims = line_claims(row, order, source);
    let kind = if row_claims.iter().any(|c| c.kind == ClaimKind::Break) {
        WeekKind::Break
    } else if row_claims
        .iter()
        .any(|c| matches!(c.kind, ClaimKind::ExamPeriod | ClaimKind::FinalExam))
    {
        WeekKind::Exam
    } else {
        WeekKind::Teaching
    };
    claims.extend(row_claims);
    weeks.push(ExtractedWeek {
        week: i64::from(week),
        starts_on,
        kind,
        topic: None,
        quote: row.to_string(),
        header_quote: if week_number(row).is_some() {
            None
        } else {
            header.map(str::to_string)
        },
        source: source.to_string(),
    });
}

/// The claims of one line: each keyword with its date.
fn line_claims(line: &str, order: Option<NumericOrder>, source: &str) -> Vec<ExtractedClaim> {
    let line = line.trim();
    let mentions = find_dates(line, order);
    if mentions.is_empty() {
        return Vec::new();
    }
    let mut keywords: Vec<(ClaimKind, std::ops::Range<usize>, String)> = Vec::new();
    for (kind, pattern) in KEYWORDS.iter() {
        for found in pattern.find_iter(line) {
            // "final exam period" is the exam period, not a final exam.
            let covered = keywords
                .iter()
                .any(|(_, range, _)| range.start <= found.start() && found.end() <= range.end);
            if !covered {
                keywords.push((*kind, found.range(), found.as_str().to_string()));
            }
        }
    }
    keywords.sort_by_key(|(_, range, _)| range.start);
    let mut used: BTreeSet<usize> = BTreeSet::new();
    let mut out = Vec::new();
    for (kind, range, label) in keywords {
        let after = mentions.iter().enumerate().find(|(i, m)| {
            !used.contains(i)
                && m.range.start >= range.end
                && m.range.start - range.end <= KEYWORD_REACH
        });
        let before = || {
            mentions.iter().enumerate().rev().find(|(i, m)| {
                !used.contains(i)
                    && m.range.end <= range.start
                    && range.start - m.range.end <= KEYWORD_REACH
            })
        };
        let Some((index, mention)) = after.or_else(before) else {
            continue;
        };
        let kind = match (kind, mention.end.is_some()) {
            (ClaimKind::ExamPeriod, false) => continue,
            (ClaimKind::FinalExam, true) => ClaimKind::ExamPeriod,
            (kind, _) => kind,
        };
        let Some(quote) = quote_around(
            line,
            range.start.min(mention.range.start),
            range.end.max(mention.range.end),
        ) else {
            continue;
        };
        used.insert(index);
        out.push(ExtractedClaim {
            kind,
            date: date_text(&mention.start),
            end_date: mention.end.as_ref().map(date_text),
            label,
            quote,
            source: source.to_string(),
        });
    }
    out
}

/// The line, or the part of it around `start..end` (plus some context), as a quote of at most
/// `MAX_QUOTE_CHARS` characters; None when even the core is too long or too short.
fn quote_around(line: &str, start: usize, end: usize) -> Option<String> {
    let length = line.chars().count();
    if length <= MAX_QUOTE_CHARS {
        return (length >= MIN_QUOTE_CHARS).then(|| line.to_string());
    }
    let mut from = start.saturating_sub(QUOTE_CONTEXT);
    while !line.is_char_boundary(from) {
        from -= 1;
    }
    let mut to = (end + QUOTE_CONTEXT).min(line.len());
    while !line.is_char_boundary(to) {
        to += 1;
    }
    let quote = line[from..to].trim();
    let chars = quote.chars().count();
    (MIN_QUOTE_CHARS..=MAX_QUOTE_CHARS)
        .contains(&chars)
        .then(|| quote.to_string())
        .or_else(|| {
            let core = line.get(start..end)?.trim();
            let chars = core.chars().count();
            (MIN_QUOTE_CHARS..=MAX_QUOTE_CHARS)
                .contains(&chars)
                .then(|| core.to_string())
        })
}

/// "2026-09-08" or "09-08".
fn date_text(date: &PartialDate) -> String {
    match date.year {
        Some(year) => format!("{year:04}-{:02}-{:02}", date.month, date.day),
        None => format!("{:02}-{:02}", date.month, date.day),
    }
}

/// The same claim found twice (e.g. a heading and its paragraph) is one claim.
fn dedupe(claims: &mut Vec<ExtractedClaim>) {
    let mut seen = BTreeSet::new();
    claims.retain(|c| seen.insert((c.kind, c.date.clone(), c.end_date.clone(), c.source.clone())));
}

fn not_found(claims: &[ExtractedClaim], weeks: &[ExtractedWeek]) -> Vec<NotFound> {
    let has = |kind: ClaimKind| claims.iter().any(|c| c.kind == kind);
    let mut out = Vec::new();
    if !has(ClaimKind::FirstClass) {
        out.push(NotFound::FirstClass);
    }
    if !has(ClaimKind::LastClass) {
        out.push(NotFound::LastClass);
    }
    if !has(ClaimKind::Break) {
        out.push(NotFound::Breaks);
    }
    if !has(ClaimKind::ExamPeriod) {
        out.push(NotFound::ExamPeriod);
    }
    if !has(ClaimKind::FinalExam) {
        out.push(NotFound::FinalExam);
    }
    if weeks.is_empty() {
        out.push(NotFound::Weeks);
    }
    out
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;
    use crate::calendar::assemble::{AssembleInput, CrossChecks, DateKind, assemble};
    use crate::calendar::text::TextPart;
    use crate::calendar::validate::{ValidationContext, validate};
    use crate::term::{BreakKind, CoursePhase};

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn material(id: &str, parts: Vec<TextPart>) -> (String, SourceMaterial) {
        (
            id.to_string(),
            SourceMaterial {
                material_id: id.to_string(),
                title: id.to_string(),
                url: None,
                published_at: None,
                parts,
            },
        )
    }

    fn text(id: &str, body: &str) -> (String, SourceMaterial) {
        material(
            id,
            vec![TextPart {
                locator: Some("p. 1".into()),
                text: body.into(),
            }],
        )
    }

    fn context() -> ValidationContext {
        ValidationContext {
            today: Some(date(2026, 9, 28)),
            outer_frame: None,
            session_start: Some(date(2026, 9, 1)),
            week_one_monday: None,
            lms_term_start: None,
        }
    }

    /// Scan → validate → assemble.
    fn calendar_of(
        sources: &HashMap<String, SourceMaterial>,
        today: NaiveDate,
    ) -> crate::calendar::assemble::Assembled {
        let extraction = scan(sources);
        let validated = validate(&extraction, sources, &context());
        assemble(&AssembleInput {
            validated: &validated,
            checks: CrossChecks::default(),
            current: None,
            current_week: None,
            current_phase: CoursePhase::Unknown,
            today,
            full_year: false,
        })
        .unwrap_or_else(|_| panic!("bad output: {extraction:#?} {validated:#?}"))
    }

    const OUTLINE: &str = "DEM332H5 Demo Methods — Fall 2026\n\
        Instructor: Dr. Demo. Office hours: Tue 2-4pm.\n\
        Important dates\n\
        Classes begin: Tuesday, September 8\n\
        Thanksgiving (no class): October 12\n\
        Reading week: Oct 26–30\n\
        Last day of classes: Dec 8; final exam period: Dec 10–22\n\
        Assignment 1 due Oct 9 at 11:59pm.";

    #[test]
    fn an_outline_scans_into_a_calendar() {
        let sources = HashMap::from([text("outline", OUTLINE)]);
        let extraction = scan(&sources);
        assert_eq!(extraction.stated_term.text.as_deref(), Some("Fall 2026"));
        let kinds: Vec<ClaimKind> = extraction.claims.iter().map(|c| c.kind).collect();
        assert_eq!(
            kinds,
            [
                ClaimKind::FirstClass,
                ClaimKind::Break,
                ClaimKind::Break,
                ClaimKind::LastClass,
                ClaimKind::ExamPeriod
            ]
        );
        assert!(extraction.not_found.contains(&NotFound::Weeks));
        let a = calendar_of(&sources, date(2026, 10, 28));
        assert_eq!(a.calendar.segments[0].first_class, date(2026, 9, 8));
        assert_eq!(a.calendar.segments[0].last_class, Some(date(2026, 12, 8)));
        assert_eq!(
            a.calendar.exam_period.map(|p| p.end),
            Some(date(2026, 12, 22))
        );
        let kinds: Vec<BreakKind> = a.calendar.breaks.iter().map(|b| b.kind).collect();
        assert_eq!(kinds, [BreakKind::Holiday, BreakKind::ReadingWeek]);
        assert!(a.passing, "{:?}", a.conflicts);
        assert_eq!(a.resulting_phase, CoursePhase::Break);
    }

    /// CAL-37 (the scan part): an HTML schedule table, one cell per line.
    #[test]
    fn an_html_schedule_table_gives_week_rows() {
        let segments = pagelamp_extract::extract_html(
            "<h2>Schedule</h2><table><tr><th>Week</th><th>Topic</th><th>Date</th></tr>\
             <tr><td>0</td><td>Orientation</td><td>Sept 8</td></tr>\
             <tr><td>1</td><td>Demo basics</td><td>Sept 15</td></tr>\
             <tr><td>2</td><td>Sampling</td><td>Sept 22</td></tr>\
             <tr><td>3</td><td>Methods</td><td>Sept 29</td></tr></table>",
        );
        let parts: Vec<TextPart> = segments
            .into_iter()
            .map(|s| TextPart {
                locator: s.locator,
                text: s.text,
            })
            .collect();
        let sources = HashMap::from([
            material("schedule", parts),
            text(
                "outline",
                "Classes begin: Tuesday, September 8. Last day of classes: Dec 8.",
            ),
        ]);
        let extraction = scan(&sources);
        let rows: Vec<(i64, Option<&str>)> = extraction
            .weeks
            .iter()
            .map(|w| (w.week, w.starts_on.as_deref()))
            .collect();
        assert_eq!(
            rows,
            [
                (0, Some("09-08")),
                (1, Some("09-15")),
                (2, Some("09-22")),
                (3, Some("09-29"))
            ]
        );
        assert_eq!(
            extraction.weeks[0].header_quote.as_deref(),
            Some("Week Topic Date")
        );
        let a = calendar_of(&sources, date(2026, 9, 28));
        assert_eq!(a.calendar.weeks.len(), 4);
        assert_eq!(a.calendar.segments[0].first_week_number, 0);
        assert_eq!(a.resulting_week_today, Some(3));
    }

    #[test]
    fn text_tables_and_week_tokens_give_rows() {
        let body = "Week  Topic  Date\n1  Demo basics  Sept 8\n2  Sampling  Sept 15\n\
                    Week 3 (Sept 22): Methods\nWeek 7 Reading week (Oct 26–30)\n\
                    Classes begin September 8";
        let sources = HashMap::from([text("schedule", body)]);
        let extraction = scan(&sources);
        let rows: Vec<(i64, WeekKind, bool)> = extraction
            .weeks
            .iter()
            .map(|w| (w.week, w.kind, w.header_quote.is_some()))
            .collect();
        assert_eq!(
            rows,
            [
                (1, WeekKind::Teaching, true),
                (2, WeekKind::Teaching, true),
                (3, WeekKind::Teaching, false),
                (7, WeekKind::Break, false)
            ]
        );
        let a = calendar_of(&sources, date(2026, 9, 28));
        assert!(a.calendar.breaks.iter().any(|b| b.numbered));
        assert!(
            a.dates
                .iter()
                .any(|d| d.kind == DateKind::WeekStart && d.week == Some(7))
        );
    }

    #[test]
    fn a_chinese_outline_scans() {
        let body = "课程安排（2026 秋季）\n开课：9月8日（周二）\n阅读周：10月26日至30日\n\
                    最后一次课：12月8日\n期末考试周：12月10日-22日";
        let sources = HashMap::from([text("大纲", body)]);
        let a = calendar_of(&sources, date(2026, 9, 28));
        assert_eq!(a.calendar.segments[0].first_class, date(2026, 9, 8));
        assert_eq!(a.calendar.segments[0].last_class, Some(date(2026, 12, 8)));
        assert_eq!(a.calendar.breaks[0].kind, BreakKind::ReadingWeek);
        assert_eq!(
            a.calendar.exam_period.map(|p| p.start),
            Some(date(2026, 12, 10))
        );
    }

    #[test]
    fn noise_gives_no_claims() {
        let body = "No class quiz next week.\nAssignment 1 due Oct 9.\nOffice hours Tue 2-4pm.\n\
                    The final exam is worth 40%.\nExam period details will follow.";
        let extraction = scan(&HashMap::from([text("notes", body)]));
        assert!(extraction.claims.is_empty(), "{:?}", extraction.claims);
        assert!(extraction.weeks.is_empty());
        // A single date for an exam "period" isn't a period; a final exam on one day is.
        let extraction = scan(&HashMap::from([text(
            "notes",
            "Exam period begins Dec 10.\nFinal exam: December 15, 9am",
        )]));
        let kinds: Vec<ClaimKind> = extraction.claims.iter().map(|c| c.kind).collect();
        assert_eq!(kinds, [ClaimKind::FinalExam]);
    }

    #[test]
    fn long_lines_are_quoted_around_the_date() {
        let filler = "Please read the course policies carefully before the term starts. ".repeat(6);
        let body = format!("{filler}Classes begin Tuesday, September 8 in room DH 2060. {filler}");
        let extraction = scan(&HashMap::from([text("outline", &body)]));
        let claim = &extraction.claims[0];
        assert!(claim.quote.contains("Classes begin Tuesday, September 8"));
        assert!(claim.quote.chars().count() <= MAX_QUOTE_CHARS);
        assert!(body.contains(&claim.quote));
    }
}
