//! The course AI policy gate on synthetic DEMO courses in every state (design §4.4, M1 DoD 2):
//! no text of a course that isn't `readable` ever reaches a prompt, hidden courses reach it not
//! at all, and a policy change between two runs is honoured.
//!
//! Every material holds a unique canary word; the tests look for it in the assembled prompt.

use chrono::{NaiveDate, TimeZone, Utc};
use pagelamp_core::ai::BlockReason;
use pagelamp_core::ai_gate::{
    ContextBudget, GateError, LeftOutReason, PlanScope, assemble, note_context, plan_context,
    week_context,
};
use pagelamp_core::model::*;
use pagelamp_core::store::Store;
use pagelamp_core::views::AsOf;

const SOURCE: &str = "folder:demo";
const BUDGET: ContextBudget = ContextBudget { max_chars: 200_000 };

fn at() -> AsOf {
    AsOf {
        now: Utc.with_ymd_and_hms(2026, 9, 24, 12, 0, 0).unwrap(),
        today: NaiveDate::from_ymd_opt(2026, 9, 24).unwrap(),
    }
}

/// A course with week-3 materials: slides with `canary` text, an assignment with its own
/// canary, and an external link.
fn add_course(store: &Store, code: &str, canary: &str) -> String {
    let id = format!("{SOURCE}/course/{code}");
    store
        .upsert_course(&CourseUpsert {
            id: id.clone(),
            source_id: SOURCE.into(),
            external_id: code.into(),
            code: Some(code.into()),
            name: format!("{code} Demo Studies"),
            term_start: NaiveDate::from_ymd_opt(2026, 9, 7),
            term_end: NaiveDate::from_ymd_opt(2026, 12, 18),
            url: None,
            syllabus_text: None,
            lms: Default::default(),
        })
        .unwrap();
    for (name, kind, text) in [
        (
            "Week 3 slides",
            MaterialKind::File,
            format!("Stomata open in light. {canary}slides"),
        ),
        (
            "Assignment 2",
            MaterialKind::File,
            format!("Answer all questions. {canary}assignment"),
        ),
        ("Course website", MaterialKind::ExternalLink, String::new()),
    ] {
        let material_id = format!("{id}/material/{name}");
        store
            .upsert_material(&MaterialUpsert {
                id: material_id.clone(),
                course_id: id.clone(),
                module_id: None,
                kind,
                title: name.into(),
                url: None,
                local_path: None,
                mime: None,
                published_at: None,
                week_hint: Some(3),
            })
            .unwrap();
        if !text.is_empty() {
            store
                .set_text_state(&material_id, TextStatus::Ok, None, Some("hash"))
                .unwrap();
            store
                .replace_chunks(
                    &material_id,
                    &[Chunk {
                        material_id: material_id.clone(),
                        ord: 0,
                        locator: Some("p. 1".into()),
                        text,
                    }],
                )
                .unwrap();
        }
    }
    id
}

/// DEMO101 readable, DEMO202 turned off, DEMO303 prohibited, DEMO404 hidden.
fn demo_store() -> Store {
    let store = Store::open_in_memory().unwrap();
    store
        .upsert_source(&SourceRecord {
            id: SOURCE.into(),
            kind: SourceKind::Folder,
            label: "Demo courses".into(),
            config: serde_json::json!({ "path": "/demo/courses" }),
            last_synced_at: None,
            last_error: None,
            last_error_kind: None,
        })
        .unwrap();
    add_course(&store, "DEMO101", "readablecanary");
    let off = add_course(&store, "DEMO202", "turnedoffcanary");
    store.set_course_ai_access(&off, false).unwrap();
    let prohibited = add_course(&store, "DEMO303", "withheldcanary");
    store
        .set_course_policy(&prohibited, AiPolicy::Prohibited, None)
        .unwrap();
    let hidden = add_course(&store, "DEMO404", "hiddencanary");
    store.set_course_hidden(&hidden, true).unwrap();
    store
}

fn blocked(result: Result<pagelamp_core::ai_gate::GatedContext, GateError>) -> BlockReason {
    match result {
        Err(GateError::Blocked(reason)) => reason,
        Err(other) => panic!("{other}"),
        Ok(context) => panic!("not blocked: {:?}", context.summary()),
    }
}

const ALL_CANARIES: [&str; 4] = [
    "readablecanary",
    "turnedoffcanary",
    "withheldcanary",
    "hiddencanary",
];

#[test]
fn an_explanation_gets_only_readable_text_without_assessments_or_links() {
    let store = demo_store();
    let context = week_context(&store, "DEMO101", Some(3), at(), BUDGET).unwrap();
    let prompt = assemble("Explain the week.", &context, None);
    let text = prompt.user_text();
    assert!(text.contains("readablecanaryslides"), "{text}");
    assert!(
        !text.contains("readablecanaryassignment"),
        "assessments are left out: {text}"
    );
    for other in &ALL_CANARIES[1..] {
        assert!(!text.contains(other), "{other} in {text}");
    }
    let summary = context.summary();
    assert_eq!(summary.materials_included, 1);
    let reasons: Vec<LeftOutReason> = summary.left_out.iter().map(|l| l.reason).collect();
    assert!(reasons.contains(&LeftOutReason::LooksLikeAssessment));
    assert!(reasons.contains(&LeftOutReason::ExternalLink));
    // Citations resolve to the included chunk only.
    let target = context.resolve_citation("c1").unwrap();
    assert_eq!(target.title, "Week 3 slides");
    assert_eq!(target.locator.as_deref(), Some("p. 1"));
    assert!(context.resolve_citation("c2").is_none());
    assert_eq!(context.manifest().materials.len(), 1);
    assert_eq!(context.manifest().materials[0].chunk_ords, vec![0]);
}

#[test]
fn courses_that_are_not_readable_are_blocked_with_their_reason() {
    let store = demo_store();
    assert_eq!(
        blocked(week_context(&store, "DEMO202", Some(3), at(), BUDGET)),
        BlockReason::CourseAiTurnedOff
    );
    assert_eq!(
        blocked(week_context(&store, "DEMO303", Some(3), at(), BUDGET)),
        BlockReason::CoursePolicyProhibited
    );
    assert_eq!(
        blocked(week_context(&store, "DEMO404", Some(3), at(), BUDGET)),
        BlockReason::CourseHidden
    );
    // A readable week with nothing readable in it.
    assert_eq!(
        blocked(week_context(&store, "DEMO101", Some(9), at(), BUDGET)),
        BlockReason::NoReadableMaterials
    );
}

#[test]
fn plans_and_notes_carry_structure_only_and_never_hidden_courses() {
    let store = demo_store();
    let scope = PlanScope {
        courses: Vec::new(),
        horizon_days: 14,
    };
    for context in [
        plan_context(&store, &scope, at()).unwrap(),
        note_context(&store, at()).unwrap(),
    ] {
        let prompt = assemble("Plan.", &context, None);
        let text = prompt.user_text();
        for canary in ALL_CANARIES {
            assert!(!text.contains(canary), "{canary} in {text}");
        }
        // Titles are structure: listed for every visible course, whatever its state.
        assert!(
            text.contains("DEMO202 Demo Studies") && text.contains("Week 3 slides"),
            "{text}"
        );
        assert!(
            !text.contains("DEMO404"),
            "hidden courses are left out: {text}"
        );
        assert!(context.summary().courses.iter().all(|c| !c.text_included));
        assert_eq!(context.summary().courses.len(), 3);
    }
    // Asking for a hidden course by name doesn't bring it in either.
    let scope = PlanScope {
        courses: vec!["DEMO404".into(), "DEMO101".into()],
        horizon_days: 14,
    };
    let context = plan_context(&store, &scope, at()).unwrap();
    let text = assemble("Plan.", &context, None).user_text().to_string();
    assert!(
        !text.contains("DEMO404") && text.contains("DEMO101"),
        "{text}"
    );
}

#[test]
fn every_policy_switch_and_visibility_combination_is_gated() {
    let policies = [
        AiPolicy::Unknown,
        AiPolicy::Prohibited,
        AiPolicy::LearningAid,
        AiPolicy::AllowedWithCitation,
        AiPolicy::Unrestricted,
    ];
    let mut cases = 0;
    for policy in policies {
        for access in [true, false] {
            for hidden in [false, true] {
                let store = demo_store();
                let id = format!("{SOURCE}/course/DEMO101");
                store.set_course_policy(&id, policy, None).unwrap();
                store.set_course_ai_access(&id, access).unwrap();
                store.set_course_hidden(&id, hidden).unwrap();
                let result = week_context(&store, "DEMO101", Some(3), at(), BUDGET);
                let readable = policy != AiPolicy::Prohibited && access && !hidden;
                match result {
                    Ok(context) => {
                        assert!(readable, "{policy:?} {access} {hidden}");
                        let prompt = assemble("Explain.", &context, None);
                        assert!(prompt.user_text().contains("readablecanaryslides"));
                    }
                    Err(GateError::Blocked(reason)) => {
                        assert!(!readable, "{policy:?} {access} {hidden}");
                        let expected = if hidden {
                            BlockReason::CourseHidden
                        } else if policy == AiPolicy::Prohibited {
                            BlockReason::CoursePolicyProhibited
                        } else {
                            BlockReason::CourseAiTurnedOff
                        };
                        assert_eq!(reason, expected, "{policy:?} {access} {hidden}");
                    }
                    Err(other) => panic!("{other}"),
                }
                // Structure contexts never carry text, whatever the state.
                let plan = plan_context(&store, &PlanScope::default(), at()).unwrap();
                let text = assemble("Plan.", &plan, None).user_text().to_string();
                assert!(!text.contains("readablecanary"), "{text}");
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 20);
}

#[test]
fn a_policy_change_between_two_runs_is_honoured() {
    let store = demo_store();
    assert!(week_context(&store, "DEMO101", Some(3), at(), BUDGET).is_ok());
    store
        .set_course_policy(
            &format!("{SOURCE}/course/DEMO101"),
            AiPolicy::Prohibited,
            None,
        )
        .unwrap();
    assert_eq!(
        blocked(week_context(&store, "DEMO101", Some(3), at(), BUDGET)),
        BlockReason::CoursePolicyProhibited
    );
}

#[test]
fn a_small_budget_trims_fairly_and_lists_what_did_not_fit() {
    let store = demo_store();
    let tiny = ContextBudget { max_chars: 5 };
    // The only readable material is longer than the budget: nothing fits, so nothing is sent.
    assert_eq!(
        blocked(week_context(&store, "DEMO101", Some(3), at(), tiny)),
        BlockReason::NoReadableMaterials
    );
}
