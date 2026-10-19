//! `pagelamp courses`, `course timeline`, `course calendar`, `course keep` and removing courses
//! (`course remove`, `removed`, `restore`, `purge`, `forget`): course weeks, phases, calendars,
//! the Past group and removal (docs/design/v0.3-course-calendar.md §7.13). Every default and
//! grouping comes from the facade; this module only renders.

use chrono::NaiveDate;
use pagelamp_app::ai::{GenEvent, GenNoticeCode};
use pagelamp_app::{
    App, CourseCalendarView, ReadCalendarOptions, RemoveOptions, RestoreFailure, TombstoneState,
};
use pagelamp_core::calendar::CourseCalendar;
use pagelamp_core::calendar::assemble::DateKind;
use pagelamp_core::calendar::candidates::CandidateLeftOut;
use pagelamp_core::model::{
    AiLabel, CalendarOrigin, Confidence, CourseGroup, CourseLifecycle, CoursePhase, CourseTimeline,
    EvidenceCode, EvidenceItem, LifecycleState, TermAnchorSource,
};
use pagelamp_core::term::session::parse_uoft_session;
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
        "{:<40} {:<22} ai_policy={:<21} ai_materials={:<18}{}{}{}",
        course.display_name(),
        week_label(&summary.timeline, &summary.lifecycle),
        course.ai_policy.as_str(),
        ai_materials(summary.ai_materials),
        next,
        if course.hidden { "  [hidden]" } else { "" },
        // An automatic sync found it; no full sync has read its modules and materials yet.
        if summary.structure_pending {
            "  [materials not read yet: sync to read them]"
        } else {
            ""
        }
    )
}

/// A break week by the student's own dates. An upcoming course keeps its place in the week
/// views there, as in a teaching week of those dates (the facade's rule in
/// `pagelamp_core::views`): it has no current week then, so the week alone doesn't tell.
fn own_break(timeline: &CourseTimeline) -> bool {
    timeline.phase == CoursePhase::Break
        && timeline.term.anchor == TermAnchorSource::StudentConfirmed
}

/// "week 4 (medium)", "reading week", "exams (after week 12)", "ended", "starts 2027-01-11"…
/// A course that is over, inactive or hasn't started has no week: its lifecycle says which.
pub fn week_label(timeline: &CourseTimeline, lifecycle: &CourseLifecycle) -> String {
    let starts = || match lifecycle.starts_on {
        Some(start) => format!("starts {start}"),
        None => "not started".to_string(),
    };
    match lifecycle.state {
        LifecycleState::Ended => return "ended".to_string(),
        LifecycleState::Inactive => return "inactive".to_string(),
        // (With a week, or in a break of its own dates: the student's dates put an upcoming
        // course there, and it is shown like a running one.)
        LifecycleState::Upcoming if timeline.current_week.is_none() && !own_break(timeline) => {
            return starts();
        }
        LifecycleState::Upcoming
        | LifecycleState::Current
        | LifecycleState::Finishing
        | LifecycleState::Unknown => {}
    }
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
        CoursePhase::NotStarted => starts(),
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
        // The label names the other phases already ("exams (after week 12)", "ended"), and a
        // course outside the week views by its lifecycle ("inactive").
        CoursePhase::Teaching | CoursePhase::Unknown
            if lifecycle.state.in_week_views() || timeline.current_week.is_some() =>
        {
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
    let confidence = confidence(term.anchor_confidence);
    let source = match lender(timeline) {
        // "from DEM240H5 … (same session, medium)": one bracket, like the other sources.
        Some(course) => format!("{course} (same session, {confidence})"),
        None => format!("{} ({confidence})", source_name(term.anchor)),
    };
    format!("{} → {end} from {source}{origin}", first.first_class)
}

/// The course that lent the dates, when they are another course's of the same session.
fn lender(timeline: &CourseTimeline) -> Option<&str> {
    if timeline.term.anchor != TermAnchorSource::SameSession {
        return None;
    }
    timeline
        .evidence_items
        .iter()
        .find(|item| {
            matches!(
                item.evidence_code(),
                Some(EvidenceCode::SameSessionAnchor | EvidenceCode::SameSessionUnlocks)
            )
        })
        .and_then(|item| item.param("course"))
}

fn source_name(source: TermAnchorSource) -> &'static str {
    match source {
        TermAnchorSource::StudentConfirmed => "your dates",
        TermAnchorSource::LmsCourseDates => "the LMS course dates",
        TermAnchorSource::LmsTerm => "the LMS term",
        TermAnchorSource::FolderConfig => "the folder's dates",
        TermAnchorSource::InstitutionCalendar => "the school calendar",
        TermAnchorSource::SameSession => "another course of the same session",
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
            let reminded = std::sync::atomic::AtomicBool::new(false);
            let read = app
                .read_course_calendar(course, &generation_id, options, |event| match event {
                    GenEvent::Started {
                        backend_label,
                        model,
                        ..
                    } => eprintln!("Reading with {backend_label} · {model}…"),
                    GenEvent::Notice {
                        code: GenNoticeCode::MaterialSharingReminder,
                    } => reminded.store(true, std::sync::atomic::Ordering::Relaxed),
                    _ => {}
                })
                .await;
            // A reading that failed after sending still reminds, before its error.
            let proposal = match read {
                Ok(proposal) => proposal,
                Err(err) => {
                    if reminded.load(std::sync::atomic::Ordering::Relaxed) {
                        eprintln!("{}", crate::text::sharing_reminder_after_failure_note());
                    }
                    return Err(err.into());
                }
            };
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

/// `pagelamp course copy-dates <from> [<to>…] [--same-session]`: the student's dates of one
/// course become the dates of other courses of its session. Without courses to copy to, it
/// lists the session's courses.
pub fn copy_dates(
    app: &App,
    from: &str,
    to: Vec<String>,
    same_session: bool,
    json: bool,
) -> anyhow::Result<()> {
    let from_course = app.course_overview(from)?.course;
    let from_name = from_course.display_name();
    let session = app.session_courses(&from_course.id)?;
    let Some(code) = &session.session else {
        // The code may end in a session code all the same: then its school (its LMS host, or
        // a folder course's `institution`) isn't one whose sessions PageLamp reads.
        if parse_uoft_session(from_course.code.as_deref(), &from_course.name).is_some() {
            anyhow::bail!(
                "{from_name}'s school isn't one PageLamp reads session codes for, so no other \
                 course shares its dates."
            );
        }
        anyhow::bail!(
            "{from_name} has no session code at the end of its code or name, so no other course \
             shares its dates."
        );
    };
    if session.campus.is_none() {
        anyhow::bail!(
            "{from_name}'s code doesn't say its campus (the digit after the course number), so \
             PageLamp can't tell which courses of session {code} share its dates."
        );
    }
    if to.is_empty() && !same_session {
        if json {
            return print_json(&session);
        }
        if session.courses.is_empty() {
            println!("{from_name} is the only course of session {code}.");
            return Ok(());
        }
        println!("Courses of session {code} besides {from_name}:");
        for course in &session.courses {
            let own = match (course.student_start, course.student_end) {
                _ if course.own_calendar => {
                    " — counts its weeks from a calendar of its own, which a copy leaves alone"
                        .to_string()
                }
                (None, None) => String::new(),
                (start, end) => format!(
                    " — has its own dates ({} → {}), which would be replaced",
                    start.map_or_else(|| "?".to_string(), |d| d.to_string()),
                    end.map_or_else(|| "?".to_string(), |d| d.to_string())
                ),
            };
            println!(
                "  {}{}{own}",
                course.code.as_deref().unwrap_or(&course.name),
                if course.hidden { " (hidden)" } else { "" }
            );
        }
        println!("Name the courses to copy to, or use --same-session for all of them.");
        return Ok(());
    }
    let to = if same_session {
        session
            .courses
            .iter()
            .map(|c| c.course_id.clone())
            .collect()
    } else {
        to
    };
    let report = app.apply_course_dates_to(&from_course.id, to)?;
    if json {
        return print_json(&report);
    }
    // A course by its code (or its name without one); anything else as it was given.
    let label = |id: &str| -> String {
        let course = session
            .courses
            .iter()
            .find(|c| c.course_id == id)
            .map(|c| (c.code.clone(), c.name.clone()))
            .or_else(|| {
                app.course_overview(id)
                    .ok()
                    .map(|o| (o.course.code, o.course.name))
            });
        match course {
            Some((code, name)) => code.unwrap_or(name),
            None => id.to_string(),
        }
    };
    if report.applied.is_empty() {
        println!("No course got the dates of {from_name}.");
    } else {
        println!("The dates of {from_name} are now also the dates of:");
        for id in &report.applied {
            println!("  {}", label(id));
        }
    }
    for skipped in &report.skipped {
        println!(
            "{}",
            crate::text::copy_skipped(skipped.reason, &label(&skipped.course_id))
        );
    }
    Ok(())
}

/// `pagelamp course remove <courses…> [--dry-run] [--now] [--keep-files] [--delete-backup]`
pub async fn remove(
    app: &App,
    courses: Vec<String>,
    dry_run: bool,
    options: RemoveOptions,
    json: bool,
) -> anyhow::Result<()> {
    if dry_run {
        let preview = app.removal_preview(courses)?;
        if json {
            return print_json(&preview);
        }
        for item in &preview.items {
            let label = item.code.as_deref().unwrap_or(&item.name);
            println!(
                "{label} — {} materials, {} deadlines",
                item.materials, item.deadlines
            );
            if item.downloaded_files > 0 {
                println!(
                    "  {} downloaded files ({} KB) go to the Trash (--keep-files keeps them)",
                    item.downloaded_files,
                    item.downloaded_bytes.div_ceil(1024)
                );
            }
            if item.own_folder_untouched {
                println!("  Your folder isn't changed; PageLamp stops reading it.");
            }
            if item.custom_settings {
                println!("  Your settings for it are kept for a restore.");
            }
            if item.cannot_sync_again {
                println!("  Canvas restricts this course: it can't be synced again.");
            }
        }
        if let Some(backup) = &preview.backup {
            println!(
                "The pre-update backup ({} days old) still holds their text{}.",
                backup.age_days,
                if backup.delete_by_default {
                    "; consider --delete-backup"
                } else {
                    ": keep it until you know the update works"
                }
            );
        }
        return Ok(());
    }
    let report = app.remove_courses(courses, options).await?;
    if json {
        return print_json(&report);
    }
    for removed in &report.removed {
        let label = removed.code.as_deref().unwrap_or(&removed.name);
        match removed.purge_in_days {
            Some(days) if !report.purged_now => println!(
                "Removed {label}. Its data is deleted in {days} days; undo with `{} course restore \"{}\"`.",
                pagelamp_core::brand::CLI_NAME,
                removed.removed_id
            ),
            _ => println!("Deleted {label}. Restoring it means syncing it again."),
        }
        if removed.files_pending {
            println!("  Its downloaded files couldn't be moved to the Trash; they are kept.");
        }
    }
    if report.backup_deleted {
        println!("The pre-update backup was deleted.");
    } else if report.backup_failed {
        println!("The pre-update backup couldn't be deleted; it is still in the data folder.");
    }
    Ok(())
}

/// `pagelamp course removed`
pub fn removed(app: &App, json: bool) -> anyhow::Result<()> {
    let removed = app.removed_courses()?;
    if json {
        return print_json(&removed);
    }
    if removed.is_empty() {
        println!("No removed courses.");
    }
    for course in &removed {
        let label = course.code.as_deref().unwrap_or(&course.name);
        let state = match (course.state, course.purge_in_days) {
            (TombstoneState::Pending, Some(days)) => {
                format!("deleted in {days} days (undo possible)")
            }
            (TombstoneState::Restoring, _) => "being restored".to_string(),
            _ => "deleted; restoring means syncing again".to_string(),
        };
        let files = if course.files_pending {
            "; downloaded files wait for the Trash"
        } else {
            ""
        };
        println!("{label} — {state}{files}  [{}]", course.removed_id);
    }
    Ok(())
}

/// `pagelamp course restore <removed id>`
pub async fn restore(app: &App, removed_id: &str, json: bool) -> anyhow::Result<()> {
    let outcome = app.restore_course(removed_id).await?;
    if json {
        return print_json(&outcome);
    }
    match (outcome.restored, outcome.failure) {
        (true, _) => println!("Restored."),
        (false, Some(RestoreFailure::Offline)) => println!("Not restored: you seem to be offline."),
        (false, Some(RestoreFailure::AccessRestricted)) => {
            println!("Not restored: Canvas restricts access to this course.")
        }
        (false, Some(RestoreFailure::NotListed)) => {
            println!("Not restored: Canvas no longer lists this course.")
        }
        (false, _) => println!("Not restored: the sync didn't bring it back."),
    }
    Ok(())
}

/// `pagelamp course purge [<removed ids…>] [--permanent]`
pub async fn purge(
    app: &App,
    removed_ids: Vec<String>,
    permanent: bool,
    json: bool,
) -> anyhow::Result<()> {
    let ids = (!removed_ids.is_empty()).then_some(removed_ids);
    let report = app.purge_removed_courses(ids, permanent).await?;
    if json {
        return print_json(&report);
    }
    println!("Deleted {} course(s).", report.purged.len());
    if !report.files_pending.is_empty() {
        println!(
            "{} course(s)' downloaded files couldn't be moved to the Trash and are kept (--permanent deletes them).",
            report.files_pending.len()
        );
    }
    if report.backup_deleted {
        println!("The pre-update backup was deleted.");
    } else if report.backup_failed {
        println!("The pre-update backup couldn't be deleted; it is still in the data folder.");
    }
    Ok(())
}
