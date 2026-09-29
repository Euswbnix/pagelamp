//! The weekly digest (v0.3; design §5.3): what the student should know this week, with no model.
//! Reminders and the "weekly digest" notification are built from it; notification text uses
//! titles and codes only, never material text.
//!
//! Per visible course: the current week and this week's materials (count and titles), and the
//! deadlines of the next 7 days. Plus the latest study plan's progress: last week's items done
//! and planned, and today's items.

use chrono::Duration;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{AsOf, Deadline, deadlines, week_materials};
use crate::Result;
use crate::model::{Confidence, StudyPlanItem, Timestamp};
use crate::store::Store;

/// Days ahead whose deadlines are in the digest.
pub const DIGEST_DEADLINE_DAYS: u32 = 7;
/// Material titles listed per course (the count covers the rest).
pub const DIGEST_MAX_TITLES: usize = 5;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WeeklyDigest {
    pub generated_at: Timestamp,
    pub courses: Vec<DigestCourse>,
    /// `None` when there is no saved study plan.
    pub plan: Option<DigestPlan>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DigestCourse {
    pub course_id: String,
    pub code: Option<String>,
    pub name: String,
    pub week: Option<u32>,
    pub confidence: Confidence,
    /// This week's materials.
    pub material_count: u32,
    /// The first `DIGEST_MAX_TITLES` of them.
    pub material_titles: Vec<String>,
    /// Due in the next `DIGEST_DEADLINE_DAYS` days, soonest first.
    pub deadlines: Vec<Deadline>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DigestPlan {
    /// Items of the last 7 days (today excluded), and how many are done.
    pub last_week_planned: u32,
    pub last_week_done: u32,
    pub today: Vec<StudyPlanItem>,
}

/// The digest for `at` (visible courses only).
pub fn weekly_digest(store: &Store, at: AsOf) -> Result<WeeklyDigest> {
    let mut courses = Vec::new();
    for course in store.list_courses(false)? {
        let week = week_materials(store, &course.id, None, false, at)?;
        let upcoming = deadlines(store, Some(&course.id), DIGEST_DEADLINE_DAYS, 0, false, at)?;
        courses.push(DigestCourse {
            course_id: course.id.clone(),
            code: course.code.clone(),
            name: course.name.clone(),
            week: week.week,
            confidence: week.timeline.confidence,
            material_count: u32::try_from(week.materials.len()).unwrap_or(u32::MAX),
            material_titles: week
                .materials
                .iter()
                .take(DIGEST_MAX_TITLES)
                .map(|m| m.title.clone())
                .collect(),
            deadlines: upcoming,
        });
    }
    let plan = store.latest_study_plan()?.map(|stored| {
        let week_ago = at.today - Duration::days(7);
        let last_week: Vec<&StudyPlanItem> = stored
            .plan
            .items
            .iter()
            .filter(|item| item.date >= week_ago && item.date < at.today)
            .collect();
        DigestPlan {
            last_week_planned: u32::try_from(last_week.len()).unwrap_or(u32::MAX),
            last_week_done: u32::try_from(last_week.iter().filter(|i| i.done).count())
                .unwrap_or(u32::MAX),
            today: stored
                .plan
                .items
                .iter()
                .filter(|item| item.date == at.today)
                .cloned()
                .collect(),
        }
    });
    Ok(WeeklyDigest {
        generated_at: at.now,
        courses,
        plan,
    })
}
