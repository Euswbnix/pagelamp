//! `pagelamp courses`, `course timeline`, `course calendar` and `course keep`: course weeks,
//! phases, calendars and the Past group (docs/design/v0.3-course-calendar.md §7.13). Every default and grouping comes
//! from the facade; this module only renders.

use chrono::NaiveDate;
use pagelamp_app::ai::GenEvent;
use pagelamp_app::{App, CourseCalendarView, ReadCalendarOptions};
use pagelamp_core::calendar::CourseCalendar;
use pagelamp_core::calendar::assemble::DateKind;
use pagelamp_core::calendar::candidates::CandidateLeftOut;
use pagelamp_core::model::{
    AiLabel, CalendarOrigin, Confidence, CourseGroup, CourseLifecycle, CoursePhase, CourseTimeline,
    EvidenceItem, TermAnchorSource,
};
use pagelamp_core::views::CourseSummary;

use crate::{ai_materials, confidence, print_json};

/// Which lifecycle groups `pagelamp courses` shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Groups {
    /// Current and Upcoming, with a count of the past courses.
    Active,
    Past,
    All,
}

impl Groups {
    fn shows(self, group: CourseGroup) -> bool {
        match self {
            Groups::Active => group != CourseGroup::Past,
            Groups::Past => group == CourseGroup::Past,
            Groups::All => true,
        }
    }
}

/// `pagelamp courses [--past|--all] [-v]`
pub fn list(app: &App, groups: Groups, evidence: bool, json: bool) -> anyhow::Result<()> {
    let all = app.list_courses()?;
    let courses: Vec<&CourseSummary> = all
        .iter()
        .filter(|summary| groups.shows(summary.lifecycle.group))
        .collect();
    if json {
        return print_json(&courses);
    }
    if all.is_empty() {
        println!(
            "No courses yet: add a source, then run `{} sync`.",
            pagelamp_core::brand::CLI_NAME
        );
        return Ok(());
    }
    for (group, title) in [
        (CourseGroup::Current, "Current"),
        (CourseGroup::Upcoming, "Upcoming"),
        (CourseGroup::Past, "Past"),
    ] {
        if !groups.shows(group) {
            continue;
        }
        let in_group: Vec<&CourseSummary> = courses
            .iter()
            .copied()
            .filter(|summary| summary.lifecycle.group == group)
            .collect();
        if in_group.is_empty() {
            continue;
        }
        println!("{title} ({})", in_group.len());
        for summary in in_group {
            println!("  {}", line(summary));
            if evidence {
                for text in summary
                    .timeline
                    .evidence
                    .iter()
                    .cloned()
                    .chain(lifecycle_lines(&summary.lifecycle))
                {
                    println!("      - {text}");
                }
            }
        }
    }
    let past = all
        .iter()
        .filter(|summary| summary.lifecycle.group == CourseGroup::Past)
        .count();
    if groups == Groups::Active && past > 0 {
        println!(
            "Past courses: {past} (`{} courses --past` lists them).",
            pagelamp_core::brand::CLI_NAME
        );
    }
    Ok(())
}

/// One course line: name, week or phase, next deadline, AI policy and access.
fn line(summary: &CourseSummary) -> String {
    let course = &summary.course;
    let next = summary
        .next_deadline
        .as_ref()
        .and_then(|d| {
            d.event.when().map(|w| {
                format!(
                    "  next: {} {}",
                    d.event.title,
                    w.with_timezone(&chrono::Local).format("%b %-d")
                )
            })
        })
        .unwrap_or_default();
    format!(
        "{:<40} {:<22} ai_policy={:<21} ai_materials={:<18}{}{}",
        course.display_name(),
        week_label(&summary.timeline, &summary.lifecycle),
        course.ai_policy.as_str(),
        ai_materials(summary.ai_materials),
        next,
        if course.hidden { "  [hidden]" } else { "" }
    )
}

/// "week 4 (medium)", "reading week", "exams (after week 12)", "ended", "starts 2027-01-11"…
pub fn week_label(timeline: &CourseTimeline, lifecycle: &CourseLifecycle) -> String {
    let week = |week: u32, c: Confidence| format!("week {week} ({})", confidence(c));
    match timeline.phase {
        CoursePhase::Teaching | CoursePhase::Unknown => match timeline.current_week {
            Some(n) => week(n, timeline.confidence),
            None => "week ?".to_string(),
        },
        CoursePhase::Break => {
            let name = timeline
                .current_break_kind
                .map_or("break", |kind| match kind.as_str() {
                    "reading_week" => "reading week",
                    "winter_break" => "winter break",
                    "holiday" => "holiday",
                    _ => "break",
                });
            match (timeline.current_week, timeline.break_after_week) {
                (Some(n), _) => format!("{name} (week {n})"),
                (None, Some(after)) => format!("{name} (after week {after})"),
                (None, None) => name.to_string(),
            }
        }
        CoursePhase::ExamPeriod => match timeline.last_teaching_week {
            Some(after) => format!("exams (after week {after})"),
            None => "exams".to_string(),
        },
        CoursePhase::Ended => "ended".to_string(),
        CoursePhase::NotStarted => match lifecycle.starts_on {
            Some(start) => format!("starts {start}"),
            None => "not started".to_string(),
        },
    }
}

fn lifecycle_lines(lifecycle: &CourseLifecycle) -> impl Iterator<Item = String> + '_ {
    lifecycle.evidence_items.iter().map(EvidenceItem::english)
}

/// `pagelamp course timeline <course>`: where the course is, the dates used and not used,
/// and the evidence.
pub fn timeline(app: &App, course: &str, json: bool) -> anyhow::Result<()> {
    let overview = app.course_overview(course)?;
    if json {
        return print_json(&serde_json::json!({
            "course": overview.course,
            "timeline": overview.timeline,
            "lifecycle": overview.lifecycle,
        }));
    }
    let timeline = &overview.timeline;
    let lifecycle = &overview.lifecycle;
    println!("{}", overview.course.display_name());
    let phase = match timeline.phase {
        // The label names the other phases already ("exams (after week 12)", "ended").
        CoursePhase::Teaching | CoursePhase::Unknown => {
            format!(" · {}", timeline.phase.as_str())
        }
        _ => String::new(),
    };
    println!(
        "  Now:       {}{phase} (phase {})",
        week_label(timeline, lifecycle),
        confidence(timeline.phase_confidence)
    );
    println!(
        "  Lifecycle: {} ({}){}",
        lifecycle.state.as_str(),
        confidence(lifecycle.confidence),
        if lifecycle.suggest_removal {
            " — suggested for removal"
        } else {
            ""
        }
    );
    println!("  Dates:     {}", dates_used(timeline));
    if let Some(label) = &timeline.term.ai_label {
        println!(
            "             read by {} · {} on {}, confirmed by you",
            label.backend_label,
            label.model,
            label.created_at.format("%Y-%m-%d")
        );
    }
    for rejected in &timeline.term.not_used {
        let span = match (rejected.start, rejected.end) {
            (Some(start), Some(end)) => format!("{start} → {end}"),
            (Some(start), None) => format!("from {start}"),
            (None, Some(end)) if rejected.end_only => format!("end {end}"),
            (None, Some(end)) => format!("until {end}"),
            (None, None) => "no dates".to_string(),
        };
        println!(
            "  Not used:  {} ({span}): {}",
            source_name(rejected.source),
            rejected.reason.as_str().replace('_', " ")
        );
    }
    println!("  Evidence:");
    for text in timeline
        .evidence
        .iter()
        .cloned()
        .chain(lifecycle_lines(lifecycle))
    {
        println!("    - {text}");
    }
    Ok(())
}

fn dates_used(timeline: &CourseTimeline) -> String {
    let term = &timeline.term;
    let Some(first) = term.teaching.first() else {
        return "none known — set them with `course term <course> --start … --end …`".to_string();
    };
    let end = term
        .exams_end
        .or(term.teaching.last().and_then(|s| s.last_class))
        .map_or_else(|| "?".to_string(), |end| end.to_string());
    let origin = match term.anchor_origin {
        Some(CalendarOrigin::Legacy) => " — set in PageLamp 0.1, check them",
        _ => "",
    };
    format!(
        "{} → {end} from {} ({}){origin}",
        first.first_class,
        source_name(term.anchor),
        confidence(term.anchor_confidence)
    )
}

fn source_name(source: TermAnchorSource) -> &'static str {
    match source {
        TermAnchorSource::StudentConfirmed => "your dates",
        TermAnchorSource::LmsCourseDates => "the LMS course dates",
        TermAnchorSource::LmsTerm => "the LMS term",
        TermAnchorSource::FolderConfig => "the folder's dates",
        TermAnchorSource::InstitutionCalendar => "the school calendar",
        TermAnchorSource::PublishedWeekLabels => "the week numbers of posted materials",
        TermAnchorSource::NoAnchor => "nothing",
    }
}

/// `pagelamp course keep <course> [--until D | --clear]`
pub fn keep(
    app: &App,
    course: &str,
    until: Option<NaiveDate>,
    clear: bool,
    json: bool,
) -> anyhow::Result<()> {
    let course = if clear {
        app.clear_keep_course_current(course)?
    } else {
        app.keep_course_current(course, until)?
    };
    let lifecycle = app.course_overview(&course.id)?.lifecycle;
    if json {
        return print_json(&serde_json::json!({
            "course_id": course.id,
            "kept_current_until": lifecycle.kept_current_until,
            "lifecycle": lifecycle.state,
        }));
    }
    match lifecycle.kept_current_until {
        Some(until) => println!("{} counts as current until {until}.", course.display_name()),
        None => println!(
            "{} is back to its own dates ({}).",
            course.display_name(),
            lifecycle.state.as_str()
        ),
    }
    Ok(())
}

/// What `pagelamp course calendar` does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalendarAction {
    Show,
    Scan,
    Read { over_budget: bool },
    Accept(i64),
    Dismiss(i64),
}

/// `pagelamp course calendar <course> [--scan|--read|--accept N|--dismiss N]`
pub async fn calendar(
    app: &App,
    course: &str,
    action: CalendarAction,
    json: bool,
) -> anyhow::Result<()> {
    match action {
        CalendarAction::Show => {}
        CalendarAction::Scan => match app.scan_course_calendar(course)? {
            Some(proposal) if !json => {
                println!("Proposal #{} from the syllabus scan.", proposal.id)
            }
            Some(_) => {}
            None if !json => println!("The scan found nothing new to propose."),
            None => {}
        },
        CalendarAction::Read { over_budget } => {
            let generation_id = format!("cli-{}", std::process::id());
            let options = ReadCalendarOptions {
                override_budget: over_budget,
            };
            let proposal = app
                .read_course_calendar(course, &generation_id, options, |event| {
                    if let GenEvent::Started {
                        backend_label,
                        model,
                        ..
                    } = event
                    {
                        eprintln!("Reading with {backend_label} · {model}…");
                    }
                })
                .await?;
            if !json {
                println!("Proposal #{} read by AI.", proposal.id);
                if proposal.sharing_reminder {
                    println!("{}", crate::text::sharing_reminder_note());
                }
            }
        }
        CalendarAction::Accept(id) => {
            app.accept_calendar_proposal(id, None)?;
        }
        CalendarAction::Dismiss(id) => app.dismiss_calendar_proposal(id)?,
    }
    let view = app.course_calendar(course)?;
    if json {
        return print_json(&view);
    }
    print_calendar_view(&view);
    Ok(())
}

fn print_calendar_view(view: &CourseCalendarView) {
    match &view.accepted {
        Some(accepted) => {
            let stale = if accepted.stale {
                " — a quoted material changed; read it again?"
            } else {
                ""
            };
            println!(
                "In force: {}{stale}",
                provenance(accepted.origin, accepted.ai_label.as_ref())
            );
            print_course_calendar(&accepted.calendar, "  ");
        }
        None => println!("No calendar in force."),
    }
    for proposal in &view.proposals {
        let verdict = if proposal.passing {
            "no conflicts".to_string()
        } else {
            format!("{} conflict(s) to choose between", proposal.conflicts.len())
        };
        let week = proposal
            .resulting_week_today
            .map_or_else(|| "no week".to_string(), |week| format!("week {week}"));
        println!(
            "Proposal #{}: {}; {verdict}; after accepting: {week}, {}",
            proposal.id,
            provenance(proposal.origin, proposal.ai_label.as_ref()),
            phase_word(proposal.resulting_phase)
        );
        for date in &proposal.dates {
            let when = match date.end {
                Some(end) => format!("{} – {end}", date.date),
                None => date.date.to_string(),
            };
            println!("  {:<11} {when}", kind_word(date.kind));
            for evidence in &date.evidence {
                if let Some(quote) = &evidence.quote {
                    let place = evidence
                        .locator
                        .as_deref()
                        .map_or_else(String::new, |l| format!(", {l}"));
                    println!("      \"{quote}\" — {}{place}", evidence.title);
                }
            }
        }
        let dropped: u32 = proposal.dropped.iter().map(|d| d.count).sum();
        if dropped > 0 {
            println!("  {dropped} item(s) left out: their words weren't in the materials");
        }
    }
    println!("Materials a reading uses:");
    for candidate in &view.candidates {
        let state = match candidate.left_out {
            None => "read".to_string(),
            Some(reason) => format!("not read: {}", left_out_word(reason)),
        };
        let download = if candidate.downloadable {
            " (can be downloaded)"
        } else {
            ""
        };
        println!("  [{state}] {}{download}", candidate.title);
    }
    if let Some(reason) = view.blocked {
        println!("Reading with AI: not now ({})", reason.as_str());
    }
}

fn print_course_calendar(calendar: &CourseCalendar, indent: &str) {
    for segment in &calendar.segments {
        let end = segment
            .last_class
            .map_or_else(|| "?".to_string(), |d| d.to_string());
        println!(
            "{indent}Classes {} – {end} (weeks from {})",
            segment.first_class, segment.first_week_number
        );
    }
    for item in &calendar.breaks {
        println!(
            "{indent}Break {} – {}{}",
            item.span.start,
            item.span.end,
            if item.numbered { " (numbered)" } else { "" }
        );
    }
    if let Some(exams) = calendar.exam_period {
        println!("{indent}Exams {} – {}", exams.start, exams.end);
    }
}

fn provenance(origin: CalendarOrigin, label: Option<&AiLabel>) -> String {
    match (origin, label) {
        (_, Some(label)) => format!(
            "read by AI · {} · {}{}",
            label.backend_label,
            label.model,
            if label.on_device {
                " (on this computer — check the dates)"
            } else {
                ""
            }
        ),
        (CalendarOrigin::Scan, None) => "from the syllabus scan".to_string(),
        (CalendarOrigin::User, None) => "your dates".to_string(),
        (CalendarOrigin::Legacy, None) => "set in PageLamp 0.1, check them".to_string(),
        (other, None) => other.as_str().to_string(),
    }
}

fn kind_word(kind: DateKind) -> &'static str {
    match kind {
        DateKind::FirstClass => "first class",
        DateKind::LastClass => "last class",
        DateKind::BreakSpan => "break",
        DateKind::ExamPeriod => "exams",
        DateKind::FinalExam => "final exam",
        DateKind::WeekStart => "week",
    }
}

fn left_out_word(reason: CandidateLeftOut) -> &'static str {
    match reason {
        CandidateLeftOut::NoText => "no text",
        CandidateLeftOut::Scanned => "scanned, no text",
        CandidateLeftOut::OverBudget => "over the limit",
        CandidateLeftOut::ExcludedByStudent => "you removed it",
    }
}

fn phase_word(phase: CoursePhase) -> &'static str {
    match phase {
        CoursePhase::NotStarted => "not started",
        CoursePhase::Teaching => "teaching",
        CoursePhase::Break => "break",
        CoursePhase::ExamPeriod => "exams",
        CoursePhase::Ended => "ended",
        CoursePhase::Unknown => "phase unknown",
    }
}
