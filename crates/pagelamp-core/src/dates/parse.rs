//! The date grammar: dates as course materials write them (docs/design/v0.3-course-calendar.md
//! §7.2, §7.5 V4/V5). Used by the calendar validator (a proposed date must parse from its
//! quote) and the deterministic syllabus scan. Pure; no clock.
//!
//! Recognised, case-insensitively:
//! - English month names and abbreviations, with or without a dot ("Sept. 8", "September
//!   8th, 2026", "8 Sept"), with an optional weekday before or after ("Tuesday, Sept. 8",
//!   "Sept 8 (Tue)");
//! - ISO dates ("2026-09-08", "2026/09/08");
//! - Chinese dates ("9月8日", "2026年9月8日", "9月8日（周二）");
//! - numeric dates "9/22" or "22/9" (optionally "/2026" or "/26"): only when unambiguous (one
//!   part is above 12), or when the same material has an unambiguous numeric date that fixes
//!   the order (`numeric_order`). "9/8" alone is rejected.
//! - ranges: "Oct 26–30", "Oct 26 - Nov 1", "October 26 to November 1", "Mon Oct 26 – Fri Oct
//!   30", "10月26日至30日", "Dec. 10-22, 2026" (a trailing year applies to both ends).
//!
//! A date without a year is a `PartialDate`; `place_first` / `place_after` choose its year as
//! V5 says: relative to the course, and checked against a weekday when the text names one.
//!
//! Week tokens for schedule tables (`week_number`, `leading_week_number`) accept Week 0,
//! unlike `timeline::parse_week_hint` (material titles, 1-based).

use std::ops::Range;
use std::sync::LazyLock;

use chrono::{Datelike, NaiveDate, Weekday};
use regex::{Captures, Regex};

use super::{add_days, days_between};

/// A date as written: the year may be missing, the weekday is what the text says (if any).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PartialDate {
    pub year: Option<i32>,
    pub month: u32,
    pub day: u32,
    pub weekday: Option<Weekday>,
}

impl PartialDate {
    fn new(year: Option<i32>, month: u32, day: u32, weekday: Option<Weekday>) -> Option<Self> {
        // February 29 is checked with the year when it is placed.
        let possible = (1..=12).contains(&month)
            && (1..=31).contains(&day)
            && NaiveDate::from_ymd_opt(2024, month, day).is_some();
        possible.then_some(PartialDate {
            year,
            month,
            day,
            weekday,
        })
    }
}

/// How a date was written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateForm {
    Iso,
    MonthName,
    Chinese,
    Numeric,
}

/// A date (or range) found in a text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DateMention {
    pub start: PartialDate,
    /// The end of a range ("Oct 26–30" → Oct 30).
    pub end: Option<PartialDate>,
    /// Byte range in the text given to `find_dates`.
    pub range: Range<usize>,
    pub form: DateForm,
}

/// The order of numeric dates in a material.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumericOrder {
    /// "9/22" = September 22.
    MonthFirst,
    /// "22/9" = September 22.
    DayFirst,
}

const WEEKDAY: &str = r"(?:mon(?:day)?|tue(?:s(?:day)?)?|wed(?:nesday)?|thu(?:r(?:s(?:day)?)?)?|fri(?:day)?|sat(?:urday)?|sun(?:day)?)\.?";
const MONTH: &str = r"(?:jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|june?|july?|aug(?:ust)?|sep(?:t(?:ember)?)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?)\.?";

fn regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("valid date regex")
}

/// "Tuesday, September 8th, 2026", "Sept. 8", "Sept 8 (Tue)".
static MONTH_FIRST: LazyLock<Regex> = LazyLock::new(|| {
    regex(&format!(
        r"(?i)(?-u:\b)(?:(?P<wd>{WEEKDAY})\s*,?\s+)?(?P<mon>{MONTH})\s+(?P<day>\d{{1,2}})(?:st|nd|rd|th)?(?-u:\b)(?:\s*\(\s*(?P<wd2>{WEEKDAY})\s*\))?(?:\s*,?\s*(?P<year>\d{{4}})(?-u:\b))?"
    ))
});

/// "8 September 2026", "Tue 8 Sept", "8th of October".
static DAY_FIRST: LazyLock<Regex> = LazyLock::new(|| {
    regex(&format!(
        r"(?i)(?-u:\b)(?:(?P<wd>{WEEKDAY})\s*,?\s+)?(?P<day>\d{{1,2}})(?:st|nd|rd|th)?\s+(?:of\s+)?(?P<mon>{MONTH})(?-u:\b)(?:\s*,?\s*(?P<year>\d{{4}})(?-u:\b))?"
    ))
});

/// "2026-09-08", "2026/09/08".
static ISO: LazyLock<Regex> = LazyLock::new(|| {
    regex(r"(?-u:\b)(?P<year>\d{4})[-/](?P<mon>\d{1,2})[-/](?P<day>\d{1,2})(?-u:\b)")
});

/// "9月8日", "2026年9月8日", "9月8日（周二）".
static CHINESE: LazyLock<Regex> = LazyLock::new(|| {
    regex(
        r"(?:(?P<year>\d{4})\s*年\s*)?(?P<mon>\d{1,2})\s*月\s*(?P<day>\d{1,2})\s*[日号號](?:\s*[（(]?\s*(?:星期|周|週|礼拜|禮拜)(?P<cwd>[一二三四五六日天])\s*[）)]?)?",
    )
});

/// "9/22", "22/9/2026", "Tue 9/22".
static NUMERIC: LazyLock<Regex> = LazyLock::new(|| {
    regex(&format!(
        r"(?i)(?:(?-u:\b)(?P<wd>{WEEKDAY})\s*,?\s+)?(?-u:\b)(?P<a>\d{{1,2}})/(?P<b>\d{{1,2}})(?:/(?P<year>\d{{4}}|\d{{2}}))?(?-u:\b)(?:\s*\(\s*(?P<wd2>{WEEKDAY})\s*\))?"
    ))
});

/// Between the ends of a range.
static RANGE_JOIN: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(?i)^\s*(?:-|–|—|~|～|to|through|thru|until|till|至|到)\s*"));

/// A bare end day after a range joiner: "30", "30th", "30日", "30, 2026".
static END_DAY: LazyLock<Regex> = LazyLock::new(|| {
    regex(r"(?i)^(?P<day>\d{1,2})(?:st|nd|rd|th)?(?:\s*[日号號])?(?:\s*,\s*(?P<year>\d{4}))?")
});

/// What must not follow a bare end day (then it is the start of something else).
static NOT_A_BARE_DAY: LazyLock<Regex> =
    LazyLock::new(|| regex(&format!(r"(?i)^\s*(?:/|:|\.\d|{MONTH}|\s*月)")));

/// Full-width digits, the full-width slash, odd spaces and hyphens to their ASCII forms.
/// `find_dates` works on (and reports byte ranges into) this normalised text.
pub fn normalise(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '０'..='９' => char::from_digit(u32::from(c) - u32::from('０'), 10).unwrap_or(c),
            '／' => '/',
            '\u{00a0}' | '\u{2009}' | '\u{202f}' | '\u{3000}' => ' ',
            '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2212}' => '-',
            other => other,
        })
        .collect()
}

/// The order numeric dates have in `material`: fixed by any unambiguous one ("9/22" →
/// month first, "22/9" → day first). None when there is none, or when both kinds appear.
pub fn numeric_order(material: &str) -> Option<NumericOrder> {
    let text = normalise(material);
    let mut month_first = false;
    let mut day_first = false;
    for captures in NUMERIC.captures_iter(&text) {
        let (Some(a), Some(b)) = (number(&captures, "a"), number(&captures, "b")) else {
            continue;
        };
        match (a, b) {
            (1..=12, 13..=31) => month_first = true,
            (13..=31, 1..=12) => day_first = true,
            _ => {}
        }
    }
    match (month_first, day_first) {
        (true, false) => Some(NumericOrder::MonthFirst),
        (false, true) => Some(NumericOrder::DayFirst),
        _ => None,
    }
}

/// Every date and date range in `text` (normalised first; ranges point into the normalised
/// text), in order. Ambiguous numeric dates are left out unless `order` says how to read them.
pub fn find_dates(text: &str, order: Option<NumericOrder>) -> Vec<DateMention> {
    let text = normalise(text);
    let mut found: Vec<DateMention> = Vec::new();
    for captures in MONTH_FIRST.captures_iter(&text) {
        found.extend(month_name_date(&captures));
    }
    for captures in DAY_FIRST.captures_iter(&text) {
        found.extend(month_name_date(&captures));
    }
    for captures in ISO.captures_iter(&text) {
        found.extend(simple_date(&captures, DateForm::Iso, None));
    }
    for captures in CHINESE.captures_iter(&text) {
        let weekday = captures
            .name("cwd")
            .and_then(|m| chinese_weekday(m.as_str()));
        found.extend(simple_date(&captures, DateForm::Chinese, weekday));
    }
    for captures in NUMERIC.captures_iter(&text) {
        found.extend(numeric_date(&captures, order));
    }
    // Overlapping matches: keep the earliest-starting (the longest on a tie). Between two
    // month-name readings ("Lecture 3 Sept 10": "3 Sept" and "Sept 10"), the one that ends
    // later wins, i.e. month first.
    found.sort_by_key(|m| (m.range.start, std::cmp::Reverse(m.range.end)));
    let mut kept: Vec<DateMention> = Vec::new();
    for mention in found {
        match kept.last() {
            Some(last) if mention.range.start < last.range.end => {
                // Two month-name readings of "3 Sept 10": prefer "Sept 10" (month first).
                if last.form == DateForm::MonthName
                    && mention.form == DateForm::MonthName
                    && mention.range.end > last.range.end
                {
                    kept.pop();
                    kept.push(mention);
                }
            }
            _ => kept.push(mention),
        }
    }
    join_ranges(&text, kept)
}

/// Joins "A – B" and "A – 30" into ranges.
fn join_ranges(text: &str, mentions: Vec<DateMention>) -> Vec<DateMention> {
    let mut out: Vec<DateMention> = Vec::new();
    let mut iter = mentions.into_iter().peekable();
    while let Some(mut mention) = iter.next() {
        let rest = &text[mention.range.end..];
        if let Some(join) = RANGE_JOIN.find(rest) {
            let after = mention.range.end + join.end();
            // A full date right after the joiner (an optional weekday in between is part of it).
            if let Some(next) = iter.peek()
                && next.range.start == after
                && ends_after(&mention.start, &next.start)
            {
                let next = iter.next().expect("peeked");
                mention.end = Some(next.start);
                mention.range.end = next.range.end;
                // "Oct 26 – Nov 1, 2026": a year at the end applies to the start too.
                if mention.start.year.is_none() {
                    mention.start.year = next.start.year;
                }
                out.push(mention);
                continue;
            }
            // A bare day: "Oct 26–30".
            let tail = &text[after..];
            if let Some(captures) = END_DAY.captures(tail) {
                let whole = captures.get(0).expect("match");
                let not_bare = NOT_A_BARE_DAY.is_match(&tail[captures["day"].len()..]);
                let day: Option<u32> = captures["day"].parse().ok();
                if !not_bare
                    && let Some(day) = day
                    && day > mention.start.day
                    && let Some(end) =
                        PartialDate::new(mention.start.year, mention.start.month, day, None)
                {
                    let year = captures
                        .name("year")
                        .and_then(|m| m.as_str().parse::<i32>().ok());
                    mention.end = Some(PartialDate {
                        year: year.or(end.year),
                        ..end
                    });
                    if mention.start.year.is_none() {
                        mention.start.year = year;
                    }
                    mention.range.end = after + whole.end();
                }
            }
        }
        out.push(mention);
    }
    out
}

/// True when `end` can close a range starting at `start` (ignoring missing years).
fn ends_after(start: &PartialDate, end: &PartialDate) -> bool {
    match (start.year, end.year) {
        (Some(a), Some(b)) if a != b => b > a,
        _ => (end.month, end.day) >= (start.month, start.day) || end.month < start.month,
    }
}

fn number(captures: &Captures<'_>, name: &str) -> Option<u32> {
    captures.name(name)?.as_str().parse().ok()
}

fn month_name_date(captures: &Captures<'_>) -> Option<DateMention> {
    let month = month_number(&captures["mon"])?;
    let day = number(captures, "day")?;
    let year = captures.name("year").and_then(|m| m.as_str().parse().ok());
    let weekday = captures
        .name("wd")
        .or_else(|| captures.name("wd2"))
        .and_then(|m| weekday_of(m.as_str()));
    let whole = captures.get(0)?;
    Some(DateMention {
        start: PartialDate::new(year, month, day, weekday)?,
        end: None,
        range: whole.range(),
        form: DateForm::MonthName,
    })
}

fn simple_date(
    captures: &Captures<'_>,
    form: DateForm,
    weekday: Option<Weekday>,
) -> Option<DateMention> {
    let year = captures.name("year").and_then(|m| m.as_str().parse().ok());
    let whole = captures.get(0)?;
    Some(DateMention {
        start: PartialDate::new(
            year,
            number(captures, "mon")?,
            number(captures, "day")?,
            weekday,
        )?,
        end: None,
        range: whole.range(),
        form,
    })
}

fn numeric_date(captures: &Captures<'_>, order: Option<NumericOrder>) -> Option<DateMention> {
    let (a, b) = (number(captures, "a")?, number(captures, "b")?);
    let (month, day) = match (a, b) {
        (1..=12, 13..=31) => (a, b),
        (13..=31, 1..=12) => (b, a),
        (1..=12, 1..=12) => match order? {
            NumericOrder::MonthFirst => (a, b),
            NumericOrder::DayFirst => (b, a),
        },
        _ => return None,
    };
    let year = captures.name("year").and_then(|m| {
        let text = m.as_str();
        let value: i32 = text.parse().ok()?;
        Some(if text.len() == 2 { 2000 + value } else { value })
    });
    let weekday = captures
        .name("wd")
        .or_else(|| captures.name("wd2"))
        .and_then(|m| weekday_of(m.as_str()));
    Some(DateMention {
        start: PartialDate::new(year, month, day, weekday)?,
        end: None,
        range: captures.get(0)?.range(),
        form: DateForm::Numeric,
    })
}

fn month_number(text: &str) -> Option<u32> {
    let name = text.trim_end_matches('.').to_ascii_lowercase();
    let month = match name.get(..3)? {
        "jan" => 1,
        "feb" => 2,
        "mar" => 3,
        "apr" => 4,
        "may" => 5,
        "jun" => 6,
        "jul" => 7,
        "aug" => 8,
        "sep" => 9,
        "oct" => 10,
        "nov" => 11,
        "dec" => 12,
        _ => return None,
    };
    Some(month)
}

fn weekday_of(text: &str) -> Option<Weekday> {
    let name = text.trim_end_matches('.').to_ascii_lowercase();
    Some(match name.get(..3)? {
        "mon" => Weekday::Mon,
        "tue" => Weekday::Tue,
        "wed" => Weekday::Wed,
        "thu" => Weekday::Thu,
        "fri" => Weekday::Fri,
        "sat" => Weekday::Sat,
        "sun" => Weekday::Sun,
        _ => return None,
    })
}

fn chinese_weekday(text: &str) -> Option<Weekday> {
    Some(match text {
        "一" => Weekday::Mon,
        "二" => Weekday::Tue,
        "三" => Weekday::Wed,
        "四" => Weekday::Thu,
        "五" => Weekday::Fri,
        "六" => Weekday::Sat,
        "日" | "天" => Weekday::Sun,
        _ => return None,
    })
}

// ---------------------------------------------------------------------------------------------
// Week tokens in schedule tables
// ---------------------------------------------------------------------------------------------

/// "Week 0", "week 3", "Wk 12", "第3周": a schedule's week number (0 allowed).
static WEEK_TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    regex(
        r"(?i)(?:(?-u:\b)(?:week|wk)\s*[-–—.:#]?\s*(?P<n>\d{1,2})(?-u:\b)|第\s*(?P<zh>\d{1,2})\s*[周週])",
    )
});

/// A header cell that names the week column.
static WEEK_HEADER: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(?i)(?:(?-u:\b)(?:week|wk)(?-u:\b)|[周週])"));

/// A row that starts with a bare number ("1  Demo basics").
static LEADING_NUMBER: LazyLock<Regex> =
    LazyLock::new(|| regex(r"^\s*(?P<n>\d{1,2})(?:\s|$|[.)|:\t])"));

/// Largest week number a schedule may have (a full-year course has ~26).
const MAX_SCHEDULE_WEEK: u32 = 40;

/// The first week token in `text` (Week 0 allowed).
pub fn week_number(text: &str) -> Option<u32> {
    let text = normalise(text);
    WEEK_TOKEN.captures_iter(&text).find_map(|captures| {
        let n: u32 = captures
            .name("n")
            .or_else(|| captures.name("zh"))?
            .as_str()
            .parse()
            .ok()?;
        (n <= MAX_SCHEDULE_WEEK).then_some(n)
    })
}

/// A table row's week: its own week token, or a leading bare number when the column's
/// `header` names the week (V4, review finding 4).
pub fn leading_week_number(row: &str, header: Option<&str>) -> Option<u32> {
    if let Some(week) = week_number(row) {
        return Some(week);
    }
    let header = header?;
    if !WEEK_HEADER.is_match(header) {
        return None;
    }
    let row = normalise(row);
    let n: u32 = LEADING_NUMBER.captures(&row)?["n"].parse().ok()?;
    (n <= MAX_SCHEDULE_WEEK).then_some(n)
}

// ---------------------------------------------------------------------------------------------
// Years (V5)
// ---------------------------------------------------------------------------------------------

/// Where a date without (or with) a year lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    Date(NaiveDate),
    /// Two candidate years fit the weekday.
    AmbiguousYear,
    /// The weekday fits no candidate year (or the written year): the text is probably from
    /// another year's syllabus.
    WeekdayMismatch,
    /// No such date (e.g. February 30, or February 29 in a common year).
    Invalid,
}

/// A weekday picks among candidate years within this many days of the anchor.
const WEEKDAY_WINDOW_DAYS: i64 = 183;
/// Other dates take the smallest year from the first class minus this many days…
const BEFORE_FIRST_CLASS_DAYS: i64 = 7;
/// …up to this many days after it (13 months).
const AFTER_FIRST_CLASS_DAYS: i64 = 396;

/// The first class's date: the candidate year nearest to `around` (a date with a year from the
/// syllabus, the session, the resolved week 1, the LMS term, or today — V5), within 6 months.
pub fn place_first(date: PartialDate, around: NaiveDate) -> Placement {
    if let Some(year) = date.year {
        return with_year(date, year);
    }
    let candidates = candidates(date, around);
    if candidates.is_empty() {
        return Placement::Invalid;
    }
    if date.weekday.is_some() {
        return by_weekday(date, &candidates, around);
    }
    let nearest = candidates
        .into_iter()
        .min_by_key(|candidate| days_between(around, *candidate).abs())
        .expect("not empty");
    Placement::Date(nearest)
}

/// Any other date of the course: the smallest year that puts it on or after `first_class`
/// minus 7 days and within 13 months of it; a weekday in the text must fit (else the weekday
/// picks among candidates within 6 months of the first class).
pub fn place_after(date: PartialDate, first_class: NaiveDate) -> Placement {
    if let Some(year) = date.year {
        return with_year(date, year);
    }
    let candidates = candidates(date, first_class);
    if candidates.is_empty() {
        return Placement::Invalid;
    }
    let from = add_days(first_class, -BEFORE_FIRST_CLASS_DAYS);
    let smallest = candidates.iter().copied().find(|candidate| {
        *candidate >= from && days_between(first_class, *candidate) <= AFTER_FIRST_CLASS_DAYS
    });
    match (smallest, date.weekday) {
        (Some(found), None) => Placement::Date(found),
        (Some(found), Some(weekday)) if found.weekday() == weekday => Placement::Date(found),
        (_, Some(_)) => by_weekday(date, &candidates, first_class),
        (None, None) => Placement::Invalid,
    }
}

/// The end of a range: the smallest year that puts it on or after its start.
pub fn place_end(end: PartialDate, start: NaiveDate) -> Placement {
    if let Some(year) = end.year {
        return with_year(end, year);
    }
    let found = [start.year(), start.year() + 1]
        .into_iter()
        .filter_map(|year| NaiveDate::from_ymd_opt(year, end.month, end.day))
        .find(|candidate| *candidate >= start);
    match found {
        Some(found) if end.weekday.is_none_or(|w| found.weekday() == w) => Placement::Date(found),
        Some(_) => Placement::WeekdayMismatch,
        None => Placement::Invalid,
    }
}

fn with_year(date: PartialDate, year: i32) -> Placement {
    match NaiveDate::from_ymd_opt(year, date.month, date.day) {
        Some(found) if date.weekday.is_none_or(|w| found.weekday() == w) => Placement::Date(found),
        Some(_) => Placement::WeekdayMismatch,
        None => Placement::Invalid,
    }
}

/// The date in the years around `anchor`, ascending.
fn candidates(date: PartialDate, anchor: NaiveDate) -> Vec<NaiveDate> {
    (anchor.year() - 1..=anchor.year() + 1)
        .filter_map(|year| NaiveDate::from_ymd_opt(year, date.month, date.day))
        .collect()
}

fn by_weekday(date: PartialDate, candidates: &[NaiveDate], anchor: NaiveDate) -> Placement {
    let fitting: Vec<NaiveDate> = candidates
        .iter()
        .copied()
        .filter(|c| days_between(anchor, *c).abs() <= WEEKDAY_WINDOW_DAYS)
        .filter(|c| Some(c.weekday()) == date.weekday)
        .collect();
    match fitting[..] {
        [only] => Placement::Date(only),
        [] => Placement::WeekdayMismatch,
        _ => Placement::AmbiguousYear,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn partial(month: u32, day: u32) -> PartialDate {
        PartialDate {
            year: None,
            month,
            day,
            weekday: None,
        }
    }

    /// (month, day) of a range's start, and of its end if any.
    type Found = ((u32, u32), Option<(u32, u32)>);

    /// (month, day) pairs of the starts and ends found in `text`.
    fn found(text: &str, order: Option<NumericOrder>) -> Vec<Found> {
        find_dates(text, order)
            .iter()
            .map(|m| {
                (
                    (m.start.month, m.start.day),
                    m.end.map(|e| (e.month, e.day)),
                )
            })
            .collect()
    }

    #[test]
    fn month_names_abbreviations_ordinals_and_weekdays() {
        for (text, month, day) in [
            ("Classes begin September 8", 9, 8),
            ("classes begin Sept. 8th", 9, 8),
            ("first lecture: Sep 8", 9, 8),
            ("Tuesday, September 8, 2026", 9, 8),
            ("Tues., Sept. 8", 9, 8),
            ("Sept 8 (Tue)", 9, 8),
            ("8 September 2026", 9, 8),
            ("the 8th of September", 9, 8),
            ("Dec. 10", 12, 10),
            ("MAY 5", 5, 5),
        ] {
            let dates = find_dates(text, None);
            assert_eq!(dates.len(), 1, "{text}: {dates:?}");
            assert_eq!(
                (dates[0].start.month, dates[0].start.day),
                (month, day),
                "{text}"
            );
            assert_eq!(dates[0].form, DateForm::MonthName, "{text}");
        }
        let with_weekday = &find_dates("Tuesday, September 8, 2026", None)[0];
        assert_eq!(with_weekday.start.weekday, Some(Weekday::Tue));
        assert_eq!(with_weekday.start.year, Some(2026));
        assert_eq!(
            find_dates("Sept 8 (Tue)", None)[0].start.weekday,
            Some(Weekday::Tue)
        );
    }

    #[test]
    fn iso_and_chinese_dates() {
        assert_eq!(found("Exam: 2026-12-15", None), [((12, 15), None)]);
        assert_eq!(found("2026/09/08", None), [((9, 8), None)]);
        assert_eq!(found("开学：9月8日", None), [((9, 8), None)]);
        let full = &find_dates("2026年9月8日（周二）", None)[0];
        assert_eq!(full.start.year, Some(2026));
        assert_eq!(full.start.weekday, Some(Weekday::Tue));
        assert_eq!(full.form, DateForm::Chinese);
        assert_eq!(found("１０月２６日", None), [((10, 26), None)]);
    }

    #[test]
    fn ranges() {
        assert_eq!(
            found("Reading week: Oct 26–30", None),
            [((10, 26), Some((10, 30)))]
        );
        assert_eq!(found("Oct 26 - 30", None), [((10, 26), Some((10, 30)))]);
        assert_eq!(
            found("October 26 to November 1", None),
            [((10, 26), Some((11, 1)))]
        );
        assert_eq!(
            found("Mon Oct 26 – Fri Oct 30", None),
            [((10, 26), Some((10, 30)))]
        );
        assert_eq!(
            found("阅读周：10月26日至30日", None),
            [((10, 26), Some((10, 30)))]
        );
        assert_eq!(found("10月26日-11月1日", None), [((10, 26), Some((11, 1)))]);
        let exams = &find_dates("Final exam period: Dec. 10-22, 2026", None)[0];
        assert_eq!(exams.start.year, Some(2026));
        assert_eq!(exams.end.map(|e| (e.day, e.year)), Some((22, Some(2026))));
        // "Dec 22 – Jan 8" crosses the year.
        assert_eq!(found("Dec 22 – Jan 8", None), [((12, 22), Some((1, 8)))]);
        // Not ranges: a time after a date, and a backwards day.
        assert_eq!(found("Sept 8 - 10:30am", None), [((9, 8), None)]);
        assert_eq!(found("Oct 26 - 3 readings", None), [((10, 26), None)]);
    }

    /// CAL-39 (the grammar part): unambiguous numeric dates only, unless the material fixes
    /// the order.
    #[test]
    fn numeric_dates_need_an_unambiguous_order() {
        assert_eq!(found("Midterm 9/22", None), [((9, 22), None)]);
        assert_eq!(found("Midterm 22/9", None), [((9, 22), None)]);
        assert!(found("Quiz 9/8", None).is_empty());
        let material = "Midterm 9/22. Quiz 9/8.";
        let order = numeric_order(material);
        assert_eq!(order, Some(NumericOrder::MonthFirst));
        assert_eq!(found("Quiz 9/8", order), [((9, 8), None)]);
        assert_eq!(
            numeric_order("Test 22/9 and quiz 8/9"),
            Some(NumericOrder::DayFirst)
        );
        assert_eq!(
            found("quiz 8/9", Some(NumericOrder::DayFirst)),
            [((9, 8), None)]
        );
        assert_eq!(numeric_order("9/22 and 22/9"), None);
        assert_eq!(numeric_order("nothing"), None);
        let with_year = &find_dates("Due Tue 9/22/26", None)[0];
        assert_eq!(with_year.start.year, Some(2026));
        assert_eq!(with_year.start.weekday, Some(Weekday::Tue));
        assert!(found("13/13", None).is_empty());
        assert!(found("page 3/12", None).is_empty());
    }

    #[test]
    fn plain_text_is_not_a_date() {
        for text in [
            "May the course go well",
            "March forward with Assignment 1",
            "Lecture 3 at 10:30",
            "Dec 2026 exam period",
            "Room 2026 in building 9",
            "Week 3 slides",
        ] {
            assert!(find_dates(text, None).is_empty(), "{text}");
        }
    }

    #[test]
    fn syllabus_noise_is_not_a_date() {
        for text in [
            "Office hours: Tue 2-4pm, DH 2060",
            "Prerequisite: CSC148/108",
            "ISBN 978-0-13-468599-1",
            "Worth 25% of the grade; chapters 3-5",
            "Version 2.3 of the notes",
            "Sep 2026 intake",
        ] {
            assert!(find_dates(text, None).is_empty(), "{text}");
        }
        // Dates among the noise are still found.
        assert_eq!(
            found("Assignment 2 (10%) due Oct 9 at 11:59pm", None),
            [((10, 9), None)]
        );
        assert_eq!(
            found("Tests: 10/15-10/19", None),
            [((10, 15), Some((10, 19)))]
        );
        assert_eq!(found("May 5-6", None), [((5, 5), Some((5, 6)))]);
    }

    #[test]
    fn several_dates_in_one_line() {
        assert_eq!(
            found("Week 1 (Sept 8) … Week 2 (Sept 15)", None),
            [((9, 8), None), ((9, 15), None)]
        );
        // "3 Sept 10": the month-name reading that ends later wins.
        assert_eq!(found("Lecture 3 Sept 10", None), [((9, 10), None)]);
    }

    #[test]
    fn ranges_point_into_the_normalised_text() {
        let text = "Reading week: Oct 26–30.";
        let mention = &find_dates(text, None)[0];
        assert_eq!(&normalise(text)[mention.range.clone()], "Oct 26–30");
    }

    #[test]
    fn week_tokens_allow_week_zero() {
        assert_eq!(week_number("Week 0: welcome"), Some(0));
        assert_eq!(week_number("wk.12"), Some(12));
        assert_eq!(week_number("第3周 复习"), Some(3));
        assert_eq!(week_number("Weekly quiz"), None);
        assert_eq!(week_number("Week 99"), None);
        // A table row with a bare leading number under a "Week" header.
        assert_eq!(
            leading_week_number("1  Demo basics", Some("Week | Topic")),
            Some(1)
        );
        assert_eq!(
            leading_week_number("0 Orientation", Some("Wk  Topic")),
            Some(0)
        );
        assert_eq!(
            leading_week_number("1  Demo basics", Some("Topic | Reading")),
            None
        );
        assert_eq!(leading_week_number("1  Demo basics", None), None);
        assert_eq!(leading_week_number("Week 4 — Methods", None), Some(4));
    }

    /// CAL-36 (the year part): years follow the course, not the calendar.
    #[test]
    fn year_inference_is_relative_to_the_syllabus() {
        // A Fall syllabus read in October: the first class is this September…
        let first = place_first(partial(9, 8), date(2026, 10, 15));
        assert_eq!(first, Placement::Date(date(2026, 9, 8)));
        // …and the December exams follow it.
        assert_eq!(
            place_after(partial(12, 10), date(2026, 9, 8)),
            Placement::Date(date(2026, 12, 10))
        );
        // A full-year course's "April 9" is in the next year.
        assert_eq!(
            place_after(partial(4, 9), date(2026, 9, 8)),
            Placement::Date(date(2027, 4, 9))
        );
        // A date a few days before the first class stays in its year.
        assert_eq!(
            place_after(partial(9, 3), date(2026, 9, 8)),
            Placement::Date(date(2026, 9, 3))
        );
        // Read in December for a January course.
        assert_eq!(
            place_first(partial(1, 11), date(2026, 12, 1)),
            Placement::Date(date(2027, 1, 11))
        );
        // A range's end after its start.
        assert_eq!(
            place_end(partial(1, 8), date(2026, 12, 22)),
            Placement::Date(date(2027, 1, 8))
        );
    }

    /// CAL-42 (the year part): "Tuesday, September 9" doesn't fit 2026.
    #[test]
    fn last_years_weekday_is_a_mismatch() {
        let tuesday_sept_9 = PartialDate {
            weekday: Some(Weekday::Tue),
            ..partial(9, 9)
        };
        assert_eq!(
            place_first(tuesday_sept_9, date(2026, 9, 1)),
            Placement::WeekdayMismatch
        );
        // The right weekday fits.
        let wednesday = PartialDate {
            weekday: Some(Weekday::Wed),
            ..partial(9, 9)
        };
        assert_eq!(
            place_first(wednesday, date(2026, 9, 1)),
            Placement::Date(date(2026, 9, 9))
        );
        // A written year that disagrees with the weekday.
        let written = PartialDate {
            year: Some(2026),
            ..tuesday_sept_9
        };
        assert_eq!(
            place_first(written, date(2026, 9, 1)),
            Placement::WeekdayMismatch
        );
        // A weekday that fixes a later date's year.
        let friday_april_9 = PartialDate {
            weekday: Some(Weekday::Fri),
            ..partial(4, 9)
        };
        assert_eq!(
            place_after(friday_april_9, date(2026, 9, 8)),
            Placement::Date(date(2027, 4, 9))
        );
        // February 29 exists only in leap years.
        assert_eq!(
            place_first(partial(2, 29), date(2026, 9, 1)),
            Placement::Invalid
        );
        assert_eq!(
            place_first(partial(2, 29), date(2027, 9, 1)),
            Placement::Date(date(2028, 2, 29))
        );
    }

    /// Every day of a year, in every written form, parses back to itself, and with its weekday
    /// it lands in the right year; ranges of every length up to five weeks join.
    #[test]
    fn every_day_in_every_form_round_trips() {
        let mut day = date(2026, 1, 1);
        while day.year() == 2026 {
            let (m, d) = (day.month(), day.day());
            let mut forms = vec![
                day.format("%B %-d").to_string(),
                day.format("%b %-d").to_string(),
                day.format("%b. %-d").to_string(),
                day.format("%A, %B %-d, %Y").to_string(),
                day.format("%a %b %-d").to_string(),
                day.format("%-d %B %Y").to_string(),
                day.format("%Y-%m-%d").to_string(),
                format!("{m}月{d}日"),
                format!("{}年{m}月{d}日", day.year()),
            ];
            if day.format("%B").to_string() == "September" {
                forms.push(format!("Sept. {d}"));
            }
            for form in &forms {
                let text = format!("Class — {form} — room 101");
                let dates = find_dates(&text, None);
                assert_eq!(dates.len(), 1, "{form}: {dates:?}");
                let start = dates[0].start;
                assert_eq!((start.month, start.day), (m, d), "{form}");
                // Anchored a month earlier (the resolver's week 1, say).
                let placed = place_first(start, add_days(day, -30));
                assert_eq!(placed, Placement::Date(day), "{form}");
                assert_eq!(
                    place_after(start, add_days(day, -30)),
                    Placement::Date(day),
                    "{form}"
                );
            }
            // Numeric: month first when unambiguous or when the order is known.
            let numeric = format!("{m}/{d}");
            let order = Some(NumericOrder::MonthFirst);
            let parsed = find_dates(&numeric, order);
            assert_eq!(parsed.len(), 1, "{numeric}");
            assert_eq!((parsed[0].start.month, parsed[0].start.day), (m, d));
            let unordered = find_dates(&numeric, None);
            assert_eq!(unordered.is_empty(), d <= 12, "{numeric}");
            let day_first = find_dates(&format!("{d}/{m}"), Some(NumericOrder::DayFirst));
            assert_eq!((day_first[0].start.month, day_first[0].start.day), (m, d));
            // Ranges from this day.
            for length in [1, 4, 6, 13, 34] {
                let end = add_days(day, length);
                let text = if end.month() == m {
                    format!("{} {}–{}", day.format("%b"), d, end.day())
                } else {
                    format!("{} – {}", day.format("%B %-d"), end.format("%B %-d"))
                };
                let found = find_dates(&text, None);
                assert_eq!(found.len(), 1, "{text}: {found:?}");
                let end_date = found[0].end.expect("a range");
                let start_date = match place_first(found[0].start, day) {
                    Placement::Date(start) => start,
                    other => panic!("{text}: {other:?}"),
                };
                assert_eq!(
                    place_end(end_date, start_date),
                    Placement::Date(end),
                    "{text}"
                );
            }
            day = add_days(day, 1);
        }
    }
}
