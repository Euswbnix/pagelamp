//! A course whose Home is a page with links, and whose navigation shows neither Pages nor
//! Files: what a sync reads, what it leaves alone, and what it notes.

use pagelamp_core::coverage::{
    self, CourseCoverage, CourseHomeKind, CourseHomeState, CoverageArea, CoverageListState,
    CoverageReason,
};

use super::*;

/// How many requests Canvas got for `api_path` (under /api/v1), exactly that path.
async fn asked(f: &Fixture, api_path: &str) -> usize {
    let wanted = format!("/api/v1{api_path}");
    f.canvas
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|request| request.url.path() == wanted)
        .count()
}

fn record(f: &Fixture) -> CourseCoverage {
    coverage::read(&f.store(), &course101(f))
        .unwrap()
        .expect("a full sync writes the record")
}

/// The page slugs Canvas was asked for (the requests under /courses/101/pages/), sorted.
async fn pages_asked(f: &Fixture) -> Vec<String> {
    let mut slugs: Vec<String> = f
        .canvas
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter_map(|request| {
            request
                .url
                .path()
                .strip_prefix("/api/v1/courses/101/pages/")
                .map(str::to_string)
        })
        .collect();
    slugs.sort();
    slugs
}

/// A full sync at `now`: one the student starts, or the app's own (`automatic`).
async fn sync_at(f: &Fixture, automatic: bool, now: chrono::DateTime<Utc>) -> crate::SyncReport {
    let api = f.api_for(automatic);
    let options = SyncOptions {
        automatic,
        ..f.options(false)
    };
    crate::sync::Syncer {
        api: &api,
        db: &f.db,
        source_id: &f.source,
        options: &options,
        progress: &no_progress,
        now,
        follow_requests: Default::default(),
    }
    .run()
    .await
    .unwrap()
}

/// One course (101) with this navigation and these modules, and nothing else to read.
async fn course(f: &Fixture, default_view: Option<&str>, tabs: Value, modules: Value) {
    f.get("/users/self", json!({"id": 1, "name": "Demo Student"}))
        .await;
    f.get(
        "/courses",
        json!([{"id": 101, "name": "Intro to Demo Studies", "course_code": "DEMO101",
                "default_view": default_view}]),
    )
    .await;
    f.get("/courses/101/tabs", tabs).await;
    f.get("/courses/101/modules", modules).await;
    f.get("/courses/101/assignments", json!([])).await;
    f.get("/announcements", json!([])).await;
    f.get("/planner/items", json!([])).await;
}

/// The Home page of `hidden_lists` with this body, and the page its syllabus links to.
async fn home_page(f: &Fixture, body: &str) {
    f.get(
        "/courses/101/front_page",
        page(600, "home", "Welcome", body),
    )
    .await;
    f.get(
        "/courses/101/pages/notes-2",
        page(612, "notes-2", "Lecture notes 2", "<p>Chlorophyll.</p>"),
    )
    .await;
}

fn page(id: u64, slug: &str, title: &str, body: &str) -> Value {
    json!({"page_id": id, "url": slug, "title": title, "body": body,
           "updated_at": "2026-09-08T10:00:00Z"})
}

/// One course. Its Home is a page that is in no module; Pages and Files are hidden.
async fn hidden_lists(f: &Fixture, default_view: Option<&str>) {
    f.get("/users/self", json!({"id": 1, "name": "Demo Student"}))
        .await;
    f.get(
        "/courses",
        json!([{"id": 101, "name": "Intro to Demo Studies", "course_code": "DEMO101",
                "default_view": default_view,
                "syllabus_body": "<p>See the <a href=\"/courses/101/pages/notes-2\">notes</a>.</p>"}]),
    )
    .await;
    f.get(
        "/courses/101/tabs",
        json!([
            {"id": "home", "label": "Home", "type": "internal"},
            {"id": "modules", "label": "Modules", "type": "internal"},
            {"id": "grades", "label": "Grades", "type": "internal", "html_url": "/courses/101/grades"},
            {"id": "context_external_tool_7", "label": "Forum", "type": "external",
             "html_url": "/courses/101/external_tools/7"}
        ]),
    )
    .await;
    let item = |id: u64, kind: &str, title: &str| {
        json!({"id": id, "type": kind, "title": title,
               "html_url": format!("https://lms.example.edu/courses/101/modules/items/{id}")})
    };
    let mut intro = item(11, "Page", "Introduction");
    intro["page_url"] = json!("intro");
    let mut must_view = item(12, "Page", "Read me first");
    must_view["page_url"] = json!("must-view");
    must_view["completion_requirement"] = json!({"type": "must_view", "completed": false});
    f.get(
        "/courses/101/modules",
        json!([{"id": 1, "name": "Week 1", "position": 1, "items": [
            item(10, "SubHeader", "Start here"),
            intro,
            must_view,
            item(13, "Assignment", "Problem Set 1"),
            item(14, "Quiz", "Quiz 1"),
            item(15, "ExternalTool", "Lab booking")
        ]}]),
    )
    .await;
    f.get("/courses/101/assignments", json!([])).await;
    f.get(
        "/courses/101/pages/intro",
        page(
            601,
            "intro",
            "Introduction",
            "<p>Welcome to the course.</p>",
        ),
    )
    .await;
    f.get("/announcements", json!([])).await;
    f.get("/planner/items", json!([])).await;
}

/// The Home page and what it links to.
async fn home_with_links(f: &Fixture) {
    let body = r#"<h1>Welcome</h1>
        <p><a href="/courses/101/pages/notes-1">Lecture notes 1</a>,
           <a href="/courses/101/wiki/notes-2?module_item_id=3">Lecture notes 2</a>,
           the <a href="/courses/101/files/701/download?verifier=SECRET-HOME&amp;wrap=1">mock exam</a>
           and <a data-api-endpoint="https://lms.example.edu/api/v1/courses/101/files/702">its answers</a>.</p>
        <p><a href="/courses/101/assignments/9">Problem Set 1</a>, <a href="/courses/101/quizzes/4">Quiz 1</a>,
           the <a href="/courses/101/external_tools/2">forum</a>,
           <a href="/courses/202/pages/week-1">last year</a>, <a href="/files/703/download">a file</a>,
           <a href="https://example.org/reading.pdf">a reading</a>,
           <a href="/courses/101/pages/must-view">read me first</a>,
           <a href="/courses/101/pages/intro">the introduction</a>.</p>
        <img src="/courses/101/files/704/preview">"#
        .replace("https://lms.example.edu", &f.canvas.uri());
    f.get(
        "/courses/101/front_page",
        page(600, "home", "Welcome", &body),
    )
    .await;
    f.get(
        "/courses/101/pages/notes-1",
        page(
            611,
            "notes-1",
            "Lecture notes 1",
            r#"<p>Stomata open in the light. See <a href="/courses/101/pages/deep">more</a>,
               <a href="/courses/101/pages/notes-1">this page</a>, <a href="/courses/101/pages/home">home</a>
               and the <a href="/courses/101/files/705">figure</a>.</p>"#,
        ),
    )
    .await;
    f.get(
        "/courses/101/pages/notes-2",
        page(
            612,
            "notes-2",
            "Lecture notes 2",
            r#"<p>Chlorophyll absorbs light. Back to <a href="/courses/101/pages/notes-1">notes 1</a>.</p>"#,
        ),
    )
    .await;
    for id in [701, 702, 705] {
        f.get(
            &format!("/courses/101/files/{id}"),
            f.file(id, &format!("file-{id}.txt"), 20),
        )
        .await;
    }
}

fn entries(record: &CourseCoverage) -> Vec<(CoverageArea, CoverageReason, Option<&str>)> {
    record
        .not_read
        .iter()
        .map(|entry| (entry.area, entry.reason, entry.title.as_deref()))
        .collect()
}

#[tokio::test]
async fn a_home_page_and_what_it_links_to_are_read_without_asking_for_a_hidden_list() {
    let f = Fixture::new().await;
    hidden_lists(&f, Some("wiki")).await;
    home_with_links(&f).await;

    let report = f.sync(&f.options(false)).await.unwrap();
    // A hidden list is no warning: the record and the course's summary line say it.
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    // The course's line in the sync summary.
    let line = &report.course_summaries[0];
    assert_eq!(
        (
            line.pages,
            line.files,
            line.linked_pages,
            line.linked_files,
            line.not_read,
            line.pages_hidden,
            line.files_hidden
        ),
        // "Not read" in the summary is what went wrong or what the student can act on: the
        // page a module asks them to view and the link that was one step too far. Not the
        // navigation and the module items PageLamp never reads by rule.
        (4, 3, 2, 3, 2, true, true)
    );

    // Asked once: the Home page, the module's page, the two linked pages, the three linked
    // files. Never: a hidden list, the page the module asks the student to view, a page two
    // links away, a file outside the course or in an <img>, another course, an assignment or
    // a quiz.
    for (path, times) in [
        ("/courses/101/front_page", 1),
        ("/courses/101/pages/intro", 1),
        ("/courses/101/pages/notes-1", 1),
        ("/courses/101/pages/notes-2", 1),
        ("/courses/101/files/701", 1),
        ("/courses/101/files/702", 1),
        ("/courses/101/files/705", 1),
        ("/courses/101/pages", 0),
        ("/courses/101/files", 0),
        ("/courses/101/pages/must-view", 0),
        ("/courses/101/pages/deep", 0),
        ("/courses/101/pages/home", 0),
        ("/courses/101/files/703", 0),
        ("/courses/101/files/704", 0),
        ("/files/703", 0),
        ("/courses/101/assignments/9", 0),
        ("/courses/101/quizzes/4", 0),
        ("/courses/101/quizzes", 0),
        ("/courses/101/discussion_topics", 0),
        ("/courses/202/pages/week-1", 0),
        ("/courses/202/tabs", 0),
    ] {
        assert_eq!(asked(&f, path).await, times, "{path}");
    }

    // The pages are materials with their text; the files are known and not downloaded.
    let store = f.store();
    let demo = course101(&f);
    let materials = store.list_materials(&demo).unwrap();
    for (suffix, title) in [
        ("/page/600", "Welcome"),
        ("/page/601", "Introduction"),
        ("/page/611", "Lecture notes 1"),
        ("/page/612", "Lecture notes 2"),
    ] {
        let page = material(&materials, &format!("{}{suffix}", f.source));
        assert_eq!(
            (page.kind, page.title.as_str()),
            (MaterialKind::Page, title)
        );
        assert_eq!(page.text_status, TextStatus::Ok, "{suffix}");
    }
    for id in [701, 702, 705] {
        let file = material(&materials, &format!("{}/file/{id}", f.source));
        assert_eq!(file.text_status, TextStatus::NotDownloaded, "{id}");
    }
    assert!(!has_material(&f, "/page/must-view"));
    assert_eq!(store.search("stomata", None, 5).unwrap().len(), 1);
    // No address in the database carries the access parameter.
    for entry in std::fs::read_dir(f.db.parent().unwrap()).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            let bytes = std::fs::read(&path).unwrap();
            assert!(!String::from_utf8_lossy(&bytes).contains("SECRET-HOME"));
        }
    }

    // The record: what the Home is, what is hidden, and what wasn't read, most useful first.
    let record = record(&f);
    assert_eq!(record.home.kind, CourseHomeKind::Page);
    assert_eq!(record.home.state, CourseHomeState::Read);
    assert_eq!(
        record.home.material_id,
        Some(format!("{}/page/600", f.source))
    );
    assert_eq!(record.pages_list, CoverageListState::Hidden);
    assert_eq!(record.files_list, CoverageListState::Hidden);
    assert_eq!(record.modules_list, CoverageListState::Read);
    use CoverageArea as A;
    use CoverageReason as R;
    assert_eq!(
        entries(&record),
        [
            (A::Pages, R::IndexHidden, None),
            (A::Files, R::IndexHidden, None),
            (A::Pages, R::WouldMarkViewed, Some("Read me first")),
            (A::Pages, R::Capped, None),
            (A::Grades, R::NotRead, Some("Grades")),
            (A::ExternalTool, R::OutsideCanvas, Some("Forum")),
            (A::Assignments, R::ByRule, Some("Problem Set 1")),
            (A::Quizzes, R::ByRule, Some("Quiz 1")),
            (A::ExternalTool, R::OutsideCanvas, Some("Lab booking")),
            (A::Assignments, R::ByRule, None),
            (A::Quizzes, R::ByRule, None),
            (A::ExternalTool, R::OutsideCanvas, None),
            (A::Other, R::OtherCourse, None),
            (A::Files, R::Other, None),
        ]
    );
    let base = f.canvas.uri();
    assert_eq!(
        record.not_read[3].url.as_deref(),
        Some(format!("{base}/courses/101/pages/deep").as_str())
    );
    assert_eq!(
        record.not_read[9].url.as_deref(),
        Some(format!("{base}/courses/101/assignments/9").as_str())
    );
    assert_eq!(record.not_read_total, 14);
    assert_eq!(
        (
            record.counts.pages,
            record.counts.linked_pages,
            record.counts.files,
            record.counts.linked_files,
            record.counts.capped,
            record.counts.off_site_links
        ),
        (4, 2, 3, 3, 1, 1)
    );
    // The words of a link are nowhere in the record.
    let stored = serde_json::to_string(&record).unwrap();
    for words in ["mock exam", "last year", "a reading", "Stomata"] {
        assert!(!stored.contains(words), "{words}");
    }

    // The views give it, with the Home as a material and the files as one entry.
    let course = store.resolve_course("DEMO101").unwrap();
    let view = coverage::view(&store, &course, &materials)
        .unwrap()
        .unwrap();
    assert_eq!(view.home.unwrap().title, "Welcome");
    assert_eq!(
        (
            view.not_readable[0].area,
            view.not_readable[0].reason,
            view.not_readable[0].count
        ),
        (A::Files, R::NeedsDownload, 3)
    );
}

#[tokio::test]
async fn linked_pages_stay_and_an_automatic_sync_leaves_them_alone_for_a_day() {
    let f = Fixture::new().await;
    hidden_lists(&f, Some("wiki")).await;
    home_with_links(&f).await;
    f.sync(&f.options(false)).await.unwrap();
    let first = record(&f);

    // A sync the student starts reads everything again: nothing is lost, nothing doubles.
    f.sync(&f.options(false)).await.unwrap();
    for path in [
        "/courses/101/front_page",
        "/courses/101/pages/intro",
        "/courses/101/pages/notes-1",
        "/courses/101/pages/notes-2",
    ] {
        assert_eq!(asked(&f, path).await, 2, "{path}");
    }
    // Linked files PageLamp has were asked about less than a week ago.
    assert_eq!(asked(&f, "/courses/101/files/701").await, 1);
    for suffix in [
        "/page/600",
        "/page/611",
        "/page/612",
        "/file/701",
        "/file/705",
    ] {
        assert!(has_material(&f, suffix), "{suffix}");
    }
    let second = record(&f);
    assert_eq!(entries(&second), entries(&first));
    assert_eq!(second.counts, first.counts);
    assert_eq!(second.followed.keep().count(), 6, "{:?}", second.followed);

    // An automatic sync with the student at the app, the same day: no page is read again
    // (reading a page may show in Canvas as the student viewing it), and the pages, the files
    // and the record are as they were.
    let automatic = SyncOptions {
        automatic: true,
        ..f.options(false)
    };
    sync_with(&f.api_for(true), &f.db, &f.source, &automatic, &no_progress)
        .await
        .unwrap();
    for path in [
        "/courses/101/front_page",
        "/courses/101/pages/intro",
        "/courses/101/pages/notes-1",
        "/courses/101/pages/notes-2",
    ] {
        assert_eq!(asked(&f, path).await, 2, "{path}");
    }
    assert_eq!(asked(&f, "/courses/101/pages/must-view").await, 0);
    for suffix in [
        "/page/600",
        "/page/601",
        "/page/611",
        "/page/612",
        "/file/702",
    ] {
        assert!(has_material(&f, suffix), "{suffix}");
    }
    let third = record(&f);
    assert_eq!(third.home, first.home);
    assert_eq!(entries(&third), entries(&first));
    assert_eq!(third.counts, first.counts);
    assert_eq!(f.store().search("stomata", None, 5).unwrap().len(), 1);

    // A sync nobody is at the app for doesn't touch the record, and its summary says nothing
    // about what a full sync reads.
    let light = SyncOptions {
        automatic: true,
        user_level_only: true,
        ..f.options(false)
    };
    let report = sync_with(&f.api_for(true), &f.db, &f.source, &light, &no_progress)
        .await
        .unwrap();
    assert_eq!(record(&f), third);
    let line = &report.course_summaries[0];
    assert_eq!(
        (line.not_read, line.linked_pages, line.pages_hidden),
        (0, 0, false)
    );
}

#[tokio::test]
async fn the_home_page_is_asked_for_only_when_the_home_is_a_page() {
    // The Home shows the modules: no request.
    let f = Fixture::new().await;
    hidden_lists(&f, Some("modules")).await;
    let report = f.sync(&f.options(false)).await.unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(asked(&f, "/courses/101/front_page").await, 0);
    let home = record(&f).home;
    assert_eq!(
        (home.kind, home.state, home.material_id),
        (CourseHomeKind::Modules, CourseHomeState::NotAPage, None)
    );

    // Canvas doesn't say and the Pages list is hidden: one request; no front page is fine.
    let f = Fixture::new().await;
    hidden_lists(&f, None).await;
    let report = f.sync(&f.options(false)).await.unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(asked(&f, "/courses/101/front_page").await, 1);
    let record_now = record(&f);
    assert_eq!(record_now.home.state, CourseHomeState::Missing);
    assert!(
        !entries(&record_now)
            .iter()
            .any(|(area, _, _)| *area == CoverageArea::Home)
    );

    // Not for this student: noted, and the sync goes on. A 401 Canvas doesn't explain
    // doesn't stop it either.
    for status in [403, 401] {
        let f = Fixture::new().await;
        hidden_lists(&f, Some("wiki")).await;
        Mock::given(method("GET"))
            .and(path("/api/v1/courses/101/front_page"))
            .respond_with(ResponseTemplate::new(status).set_body_string("no"))
            .mount(&f.canvas)
            .await;
        let report = f.sync(&f.options(false)).await.unwrap();
        assert!(
            report.warnings.is_empty(),
            "{status}: {:?}",
            report.warnings
        );
        let record_now = record(&f);
        assert_eq!(record_now.home.state, CourseHomeState::Failed, "{status}");
        assert_eq!(
            entries(&record_now)[0],
            (CoverageArea::Home, CoverageReason::FailedThisSync, None)
        );
        assert!(has_material(&f, "/page/601"), "the rest was read");
    }
}

#[tokio::test]
async fn a_listed_front_page_needs_no_request_of_its_own() {
    let f = Fixture::new().await;
    f.standard().await;
    // Course 101 lists its pages; one of them is the front page.
    Mock::given(method("GET"))
        .and(path("/api/v1/courses/101/pages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"page_id": 601, "url": "week-1-overview", "title": "Overview",
             "updated_at": "2026-09-08T10:00:00Z", "front_page": true}
        ])))
        .with_priority(1)
        .mount(&f.canvas)
        .await;
    let report = f.sync(&f.options(false)).await.unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(asked(&f, "/courses/101/front_page").await, 0);
    let home = record(&f).home;
    assert_eq!(home.state, CourseHomeState::Read);
    assert_eq!(home.material_id, Some(format!("{}/page/601", f.source)));
    // Course 202 shows no Pages tab and Canvas doesn't say what its Home is: asked once,
    // and "no front page" is no warning.
    assert_eq!(asked(&f, "/courses/202/front_page").await, 1);
}

#[tokio::test]
async fn a_failed_linked_page_or_file_is_noted_and_the_sync_goes_on() {
    let f = Fixture::new().await;
    hidden_lists(&f, Some("wiki")).await;
    // notes-1 answers a 401 Canvas doesn't explain, file 701 a 500; the rest is fine.
    Mock::given(method("GET"))
        .and(path("/api/v1/courses/101/pages/notes-1"))
        .respond_with(ResponseTemplate::new(401).set_body_string("who are you"))
        .with_priority(1)
        .mount(&f.canvas)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/courses/101/files/701"))
        .respond_with(ResponseTemplate::new(500))
        .with_priority(1)
        .mount(&f.canvas)
        .await;
    home_with_links(&f).await;

    let report = f.sync(&f.options(false)).await.unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert!(has_material(&f, "/page/612"));
    assert!(has_material(&f, "/file/702"));
    assert!(!has_material(&f, "/page/611"));
    assert!(!has_material(&f, "/file/701"));
    let record_now = record(&f);
    let failed: Vec<_> = record_now
        .not_read
        .iter()
        .filter(|entry| entry.reason == CoverageReason::FailedThisSync)
        .map(|entry| (entry.area, entry.url.clone().unwrap()))
        .collect();
    let base = f.canvas.uri();
    assert_eq!(
        failed,
        [
            (
                CoverageArea::Pages,
                format!("{base}/courses/101/pages/notes-1")
            ),
            (CoverageArea::Files, format!("{base}/courses/101/files/701")),
        ]
    );
}

#[tokio::test]
async fn no_page_is_read_when_the_modules_could_not_be_read() {
    let f = Fixture::new().await;
    // The module list fails: which pages the student is asked to view isn't known.
    Mock::given(method("GET"))
        .and(path("/api/v1/courses/101/modules"))
        .respond_with(ResponseTemplate::new(500))
        .with_priority(1)
        .mount(&f.canvas)
        .await;
    hidden_lists(&f, Some("wiki")).await;
    home_with_links(&f).await;
    f.sync(&f.options(false)).await.unwrap();
    // Not the Home page (it could be a page a module asks the student to view), and nothing a
    // text links to: the one text that was read, the syllabus, links to a page.
    for path in [
        "/courses/101/front_page",
        "/courses/101/pages/notes-1",
        "/courses/101/pages/notes-2",
        "/courses/101/pages/must-view",
        "/courses/101/pages/intro",
        "/courses/101/files/701",
    ] {
        assert_eq!(asked(&f, path).await, 0, "{path}");
    }
    let record_now = record(&f);
    assert_eq!(record_now.modules_list, CoverageListState::Failed);
    assert_eq!(record_now.home.state, CourseHomeState::Failed);
    let base = f.canvas.uri();
    let failed: Vec<_> = record_now
        .not_read
        .iter()
        .filter(|entry| entry.reason == CoverageReason::FailedThisSync)
        .map(|entry| (entry.area, entry.url.clone().unwrap_or_default()))
        .collect();
    assert_eq!(
        failed,
        [
            (CoverageArea::Home, format!("{base}/courses/101")),
            (
                CoverageArea::Pages,
                format!("{base}/courses/101/pages/notes-2")
            ),
        ]
    );
}

/// The Home page read by an earlier sync stays when a later sync can't ask for it: its
/// material, and that it is the Home.
#[tokio::test]
async fn the_home_page_of_an_earlier_sync_stays_when_it_cannot_be_asked_for() {
    let f = Fixture::new().await;
    hidden_lists(&f, Some("wiki")).await;
    home_with_links(&f).await;
    f.sync(&f.options(false)).await.unwrap();
    let first = record(&f);
    let home = first.home.material_id.clone().unwrap();

    // The module list fails in the next sync, and in the one after it the Home page does.
    for failing in [
        "/api/v1/courses/101/modules",
        "/api/v1/courses/101/front_page",
    ] {
        f.canvas.reset().await;
        Mock::given(method("GET"))
            .and(path(failing))
            .respond_with(ResponseTemplate::new(500))
            .with_priority(1)
            .mount(&f.canvas)
            .await;
        hidden_lists(&f, Some("wiki")).await;
        home_with_links(&f).await;
        f.sync(&f.options(false)).await.unwrap();
        let now = record(&f);
        assert_eq!(
            now.home.material_id.as_deref(),
            Some(home.as_str()),
            "{failing}"
        );
        assert_eq!(now.home.state, CourseHomeState::Failed, "{failing}");
        assert_eq!(
            entries(&now)[0],
            (CoverageArea::Home, CoverageReason::FailedThisSync, None),
            "{failing}"
        );
        // Still the Home, not a page a link led to; and its text is still there.
        assert_eq!(
            now.counts.linked_pages, first.counts.linked_pages,
            "{failing}"
        );
        assert!(has_material(&f, "/page/600"), "{failing}");
        let store = f.store();
        let course = store.resolve_course("DEMO101").unwrap();
        let materials = store.list_materials(&course.id).unwrap();
        let view = coverage::view(&store, &course, &materials)
            .unwrap()
            .unwrap();
        assert_eq!(view.home.unwrap().title, "Welcome", "{failing}");
        // The page itself is asked for by neither path.
        assert_eq!(asked(&f, "/courses/101/pages/home").await, 0, "{failing}");
    }
}

#[tokio::test]
async fn more_links_than_the_limit_are_cut_the_same_way_every_time() {
    let f = Fixture::new().await;
    hidden_lists(&f, Some("wiki")).await;
    let links: String = (0..45)
        .map(|n| format!(r#"<a href="/courses/101/pages/p{n:02}">p</a> "#))
        .collect();
    f.get(
        "/courses/101/front_page",
        page(600, "home", "Welcome", &links),
    )
    .await;
    for n in 0..45 {
        f.get(
            &format!("/courses/101/pages/p{n:02}"),
            page(1000 + n, &format!("p{n:02}"), "A page", "<p>text</p>"),
        )
        .await;
    }
    for _ in 0..2 {
        f.sync(&f.options(false)).await.unwrap();
        let record_now = record(&f);
        assert_eq!(record_now.counts.linked_pages, 40);
        // The Home page's links come first, then the syllabus's one.
        assert_eq!(record_now.counts.capped, 6);
        let capped: Vec<String> = record_now
            .not_read
            .iter()
            .filter(|entry| entry.reason == CoverageReason::Capped)
            .map(|entry| entry.url.clone().unwrap())
            .collect();
        let base = f.canvas.uri();
        assert_eq!(
            capped,
            (40..45)
                .map(|n| format!("p{n}"))
                .chain(["notes-2".to_string()])
                .map(|slug| format!("{base}/courses/101/pages/{slug}"))
                .collect::<Vec<_>>()
        );
    }
    assert_eq!(asked(&f, "/courses/101/pages/p39").await, 2);
    assert_eq!(asked(&f, "/courses/101/pages/p40").await, 0);
}

/// The downloads of the three files the Home page of `home_with_links` leads to.
async fn linked_downloads(f: &Fixture) {
    for id in [701, 702, 705] {
        Mock::given(method("GET"))
            .and(path(format!("/files/{id}/download")))
            .respond_with(ResponseTemplate::new(302).insert_header(
                "Location",
                format!("{}/blob/{id}?sig=demo", f.storage.uri()).as_str(),
            ))
            .mount(&f.canvas)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/blob/{id}")))
            .respond_with(ResponseTemplate::new(200).set_body_string(format!("xylem text {id}")))
            .mount(&f.storage)
            .await;
    }
}

#[tokio::test]
async fn every_request_for_a_linked_item_is_an_allow_listed_get() {
    let f = Fixture::new().await;
    hidden_lists(&f, Some("wiki")).await;
    home_with_links(&f).await;
    linked_downloads(&f).await;
    f.sync(&f.options(false)).await.unwrap();
    let report = f.sync(&f.options(true)).await.unwrap();
    assert_eq!(report.files_downloaded, 3, "{:?}", report.warnings);
    assert_eq!(asked(&f, "/courses/101/front_page").await, 2);
    super::assert_only_allow_listed_gets(&f).await;
}

#[tokio::test]
async fn a_download_takes_linked_files_too() {
    let f = Fixture::new().await;
    hidden_lists(&f, Some("wiki")).await;
    home_with_links(&f).await;
    f.sync(&f.options(false)).await.unwrap();
    linked_downloads(&f).await;
    // Asked about less than a week ago, but a download needs each file's address again.
    let report = f.sync(&f.options(true)).await.unwrap();
    assert_eq!(report.files_downloaded, 3, "{:?}", report.warnings);
    assert_eq!(asked(&f, "/courses/101/files/701").await, 2);
    assert_eq!(f.store().search("xylem", None, 5).unwrap().len(), 3);
    let course = f.store().resolve_course("DEMO101").unwrap();
    let materials = f.store().list_materials(&course.id).unwrap();
    let view = coverage::view(&f.store(), &course, &materials)
        .unwrap()
        .unwrap();
    assert!(
        !view
            .not_readable
            .iter()
            .any(|entry| entry.reason == CoverageReason::NeedsDownload),
        "{:?}",
        view.not_readable
    );
}

#[tokio::test]
async fn a_linked_file_canvas_no_longer_has_stays_and_is_noted() {
    let f = Fixture::new().await;
    hidden_lists(&f, Some("wiki")).await;
    home_with_links(&f).await;
    f.sync(&f.options(false)).await.unwrap();
    // The last check is more than a week old, and Canvas no longer has file 701.
    let store = f.store();
    let demo = course101(&f);
    let mut old = record(&f);
    for seen in old.followed.files.values_mut() {
        seen.checked_at = Some(Utc::now() - TimeDelta::days(8));
    }
    coverage::write(&store, &demo, &old).unwrap();
    Mock::given(method("GET"))
        .and(path("/api/v1/courses/101/files/701"))
        .respond_with(ResponseTemplate::new(404))
        .with_priority(1)
        .mount(&f.canvas)
        .await;
    f.sync(&f.options(false)).await.unwrap();
    assert_eq!(asked(&f, "/courses/101/files/701").await, 2);
    assert_eq!(asked(&f, "/courses/101/files/702").await, 2);
    assert!(has_material(&f, "/file/701"), "nothing is deleted");
    let record_now = record(&f);
    assert!(record_now.followed.files[&format!("{}/file/701", f.source)].gone);
    let gone: Vec<_> = record_now
        .not_read
        .iter()
        .filter(|entry| entry.reason == CoverageReason::NoLongerInCanvas)
        .map(|entry| entry.title.clone().unwrap())
        .collect();
    assert_eq!(gone, ["file-701.txt"]);

    // The next sync doesn't ask about it again (it was asked about today) and still says so;
    // and a file Canvas no longer has isn't one the student could download.
    f.sync(&f.options(false)).await.unwrap();
    assert_eq!(asked(&f, "/courses/101/files/701").await, 2);
    let later = record(&f);
    assert!(later.followed.files[&format!("{}/file/701", f.source)].gone);
    assert_eq!(
        entries(&later)
            .iter()
            .filter(|(_, reason, _)| *reason == CoverageReason::NoLongerInCanvas)
            .count(),
        1
    );
    let course = store.resolve_course("DEMO101").unwrap();
    let materials = store.list_materials(&course.id).unwrap();
    let view = coverage::view(&store, &course, &materials)
        .unwrap()
        .unwrap();
    assert_eq!(
        (view.not_readable[0].reason, view.not_readable[0].count),
        (CoverageReason::NeedsDownload, 2)
    );
}

#[tokio::test]
async fn one_sync_asks_for_a_limited_number_of_linked_items_over_all_courses() {
    let f = Fixture::new().await;
    hidden_lists(&f, Some("wiki")).await;
    home_with_links(&f).await;
    // One request for a linked item is left in this sync.
    let api = f.api();
    let options = f.options(false);
    crate::sync::Syncer {
        api: &api,
        db: &f.db,
        source_id: &f.source,
        options: &options,
        progress: &no_progress,
        now: Utc::now(),
        follow_requests: (crate::cover::MAX_FOLLOW_REQUESTS - 1).into(),
    }
    .run()
    .await
    .unwrap();
    // The pages a sync reads anyway aren't counted; of the linked ones only the first is read.
    assert_eq!(asked(&f, "/courses/101/front_page").await, 1);
    assert_eq!(asked(&f, "/courses/101/pages/intro").await, 1);
    assert_eq!(asked(&f, "/courses/101/pages/notes-1").await, 1);
    assert_eq!(asked(&f, "/courses/101/pages/notes-2").await, 0);
    for id in [701, 702, 705] {
        assert_eq!(asked(&f, &format!("/courses/101/files/{id}")).await, 0);
    }
    let record_now = record(&f);
    let base = f.canvas.uri();
    let capped: Vec<String> = record_now
        .not_read
        .iter()
        .filter(|entry| entry.reason == CoverageReason::Capped)
        .map(|entry| entry.url.clone().unwrap())
        .collect();
    assert_eq!(
        capped,
        [
            // In the order the sync met them: what the one linked page it read links to,
            // then the next linked page, then the files.
            format!("{base}/courses/101/pages/deep"),
            format!("{base}/courses/101/pages/notes-2"),
            format!("{base}/courses/101/files/701"),
            format!("{base}/courses/101/files/702"),
            format!("{base}/courses/101/files/705"),
        ]
    );
}

/// Answers, and stops the sync as the student's Stop would.
struct StopsTheSync(pagelamp_core::source::CancelFlag, Value);
impl wiremock::Respond for StopsTheSync {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        self.0.cancel();
        ResponseTemplate::new(200).set_body_json(self.1.clone())
    }
}

#[tokio::test]
async fn a_sync_stopped_while_it_follows_links_asks_for_nothing_more() {
    let f = Fixture::new().await;
    hidden_lists(&f, Some("wiki")).await;
    let cancel = pagelamp_core::source::CancelFlag::new();
    Mock::given(method("GET"))
        .and(path("/api/v1/courses/101/pages/notes-1"))
        .respond_with(StopsTheSync(
            cancel.clone(),
            page(611, "notes-1", "Lecture notes 1", "<p>text</p>"),
        ))
        .with_priority(1)
        .mount(&f.canvas)
        .await;
    home_with_links(&f).await;
    let mut options = f.options(false);
    options.extractor = pagelamp_core::ingest::Extractor::default().cancellable(cancel);
    let err = f.sync(&options).await.unwrap_err();
    assert!(err.cancelled, "{err:?}");
    assert_eq!(asked(&f, "/courses/101/pages/notes-1").await, 1);
    assert_eq!(asked(&f, "/courses/101/pages/notes-2").await, 0);
    assert_eq!(asked(&f, "/courses/101/files/701").await, 0);
    // Nothing of the course was written: no record either.
    assert_eq!(coverage::read(&f.store(), &course101(&f)).unwrap(), None);
}

#[tokio::test]
async fn listed_pages_are_scanned_once_after_an_update_and_a_must_view_page_is_never_read() {
    let f = Fixture::new().await;
    f.get("/users/self", json!({"id": 1, "name": "Demo Student"}))
        .await;
    f.get(
        "/courses",
        json!([{"id": 101, "name": "Intro to Demo Studies", "course_code": "DEMO101",
                "default_view": "modules"}]),
    )
    .await;
    // Pages are listed; Files are hidden.
    f.get(
        "/courses/101/tabs",
        json!([{"id": "home"}, {"id": "modules"}, {"id": "pages"}]),
    )
    .await;
    f.get(
        "/courses/101/modules",
        json!([{"id": 1, "name": "Week 1", "position": 1, "items": [
            {"id": 12, "type": "Page", "page_url": "must-view", "title": "Read me first",
             "completion_requirement": {"type": "must_view", "completed": false}}
        ]}]),
    )
    .await;
    f.get(
        "/courses/101/pages",
        json!([
            {"page_id": 601, "url": "week-1", "title": "Week 1", "updated_at": "2026-09-08T10:00:00Z"},
            {"page_id": 602, "url": "must-view", "title": "Read me first", "updated_at": "2026-09-08T10:00:00Z"}
        ]),
    )
    .await;
    f.get(
        "/courses/101/pages/week-1",
        page(
            601,
            "week-1",
            "Week 1",
            r#"<p>Photosynthesis. The <a href="/courses/101/files/701/download">slides</a>.</p>"#,
        ),
    )
    .await;
    f.get("/courses/101/files/701", f.file(701, "slides.txt", 20))
        .await;
    f.get("/courses/101/assignments", json!([])).await;
    f.get("/announcements", json!([])).await;
    f.get("/planner/items", json!([])).await;

    f.sync(&f.options(false)).await.unwrap();
    assert_eq!(asked(&f, "/courses/101/pages/week-1").await, 1);
    assert!(has_material(&f, "/file/701"));
    // The page a module asks the student to view is listed, never read: it has a row and
    // no text.
    assert_eq!(asked(&f, "/courses/101/pages/must-view").await, 0);
    let materials = f.store().list_materials(&course101(&f)).unwrap();
    assert_ne!(
        material(&materials, &format!("{}/page/602", f.source)).text_status,
        TextStatus::Ok
    );
    assert_eq!(
        entries(&record(&f))[1],
        (
            CoverageArea::Pages,
            CoverageReason::WouldMarkViewed,
            Some("Read me first")
        )
    );

    // Unchanged: not read again, and its link is still followed from the record.
    f.sync(&f.options(false)).await.unwrap();
    assert_eq!(asked(&f, "/courses/101/pages/week-1").await, 1);
    assert_eq!(record(&f).counts.linked_files, 1);

    // As after an update from a version that kept no record: the unchanged page is read once
    // more, because only its text says which files it links to.
    coverage::remove(&f.store(), &course101(&f)).unwrap();
    f.sync(&f.options(false)).await.unwrap();
    assert_eq!(asked(&f, "/courses/101/pages/week-1").await, 2);
    f.sync(&f.options(false)).await.unwrap();
    assert_eq!(asked(&f, "/courses/101/pages/week-1").await, 2);
    assert_eq!(asked(&f, "/courses/101/pages/must-view").await, 0);
    assert!(has_material(&f, "/file/701"));
}

#[tokio::test]
async fn an_automatic_sync_a_day_later_reads_a_linked_page_once_more() {
    let f = Fixture::new().await;
    hidden_lists(&f, Some("wiki")).await;
    home_with_links(&f).await;
    let start = Utc::now();
    sync_at(&f, false, start).await;
    // An hour later nothing is read again; 25 hours later each page is, once; and an hour
    // after that nothing is.
    for (hours, times) in [(1, 1), (25, 2), (26, 2)] {
        sync_at(&f, true, start + TimeDelta::hours(hours)).await;
        for path in [
            "/courses/101/front_page",
            "/courses/101/pages/intro",
            "/courses/101/pages/notes-1",
            "/courses/101/pages/notes-2",
        ] {
            assert_eq!(asked(&f, path).await, times, "{path} after {hours} h");
        }
        // A linked file is asked about again after a week.
        assert_eq!(asked(&f, "/courses/101/files/701").await, 1);
        assert_eq!(asked(&f, "/courses/101/pages/must-view").await, 0);
    }
    assert_eq!(record(&f).counts.linked_pages, 2);
}

/// Pages and Files are both in the navigation, and both lists are read completely.
async fn both_lists(f: &Fixture, page_updated: &str, body: &str, listed_files: &[u64]) {
    course(
        f,
        Some("modules"),
        json!([{"id": "home"}, {"id": "modules"}, {"id": "pages"}, {"id": "files"}]),
        json!([]),
    )
    .await;
    let listed = json!({"page_id": 601, "url": "week-1", "title": "Week 1",
                        "updated_at": page_updated});
    f.get("/courses/101/pages", json!([listed])).await;
    let mut read = listed.clone();
    read["body"] = json!(body);
    f.get("/courses/101/pages/week-1", read).await;
    let files: Vec<Value> = listed_files
        .iter()
        .map(|id| f.file(*id, &format!("file-{id}.txt"), 20))
        .collect();
    f.get("/courses/101/files", json!(files)).await;
    f.get("/courses/101/files/701", f.file(701, "handout.txt", 20))
        .await;
}

#[tokio::test]
async fn with_both_lists_read_a_file_only_a_link_names_stays() {
    let f = Fixture::new().await;
    let linking = r#"<p>Xylem. The <a href="/courses/101/files/701/download">handout</a>
        and the <a href="/courses/101/files/501">slides</a>.</p>"#;
    both_lists(&f, "2026-09-08T10:00:00Z", linking, &[501, 502]).await;
    let report = f.sync(&f.options(false)).await.unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let first = record(&f);
    assert_eq!(
        (first.pages_list, first.files_list),
        (CoverageListState::Read, CoverageListState::Read)
    );
    // The list gives file 501; only the link names file 701.
    assert_eq!(asked(&f, "/courses/101/files/501").await, 0);
    assert_eq!(asked(&f, "/courses/101/files/701").await, 1);
    assert_eq!(first.counts.linked_files, 1);
    for suffix in ["/file/501", "/file/502", "/file/701"] {
        assert!(has_material(&f, suffix), "{suffix}");
    }

    // Again: the Files list was read completely and doesn't have file 701. It stays.
    f.sync(&f.options(false)).await.unwrap();
    assert_eq!(asked(&f, "/courses/101/pages/week-1").await, 1);
    assert_eq!(asked(&f, "/courses/101/files/701").await, 1);
    assert!(has_material(&f, "/file/701"));

    // The page changes and no longer links to it, and the list loses file 502: what a list
    // gave goes with the list, what a link once led to stays.
    f.canvas.reset().await;
    both_lists(
        &f,
        "2026-09-15T10:00:00Z",
        "<p>Xylem and phloem.</p>",
        &[501],
    )
    .await;
    f.sync(&f.options(false)).await.unwrap();
    assert_eq!(asked(&f, "/courses/101/pages/week-1").await, 1);
    assert_eq!(asked(&f, "/courses/101/files/701").await, 0);
    assert!(has_material(&f, "/file/501"));
    assert!(!has_material(&f, "/file/502"));
    assert!(has_material(&f, "/file/701"));
    let third = record(&f);
    assert_eq!(third.counts.linked_files, 1);
    assert!(
        third
            .followed
            .files
            .contains_key(&format!("{}/file/701", f.source))
    );
}

#[tokio::test]
async fn with_the_pages_list_read_a_link_to_a_page_it_does_not_have_is_not_asked_for() {
    let f = Fixture::new().await;
    // Another address of a listed page, or a page this student can't open.
    let body = r#"<p>See <a href="/courses/101/pages/week-one">the old address</a>,
        <a href="/courses/101/pages/601">a number</a> and
        <a href="/courses/101/pages/drafts">a draft</a>.</p>"#;
    both_lists(&f, "2026-09-08T10:00:00Z", body, &[501]).await;
    f.sync(&f.options(false)).await.unwrap();
    assert_eq!(pages_asked(&f).await, ["week-1"]);
    let record_now = record(&f);
    assert_eq!(record_now.not_read, []);
    assert_eq!(record_now.not_read_total, 0);
    assert_eq!(record_now.counts.linked_pages, 0);
}

#[tokio::test]
async fn a_link_that_could_be_another_address_of_a_must_view_page_is_not_asked_for() {
    let f = Fixture::new().await;
    hidden_lists(&f, Some("wiki")).await;
    // The module asks the student to view "Read me first" (must-view). No Pages list says
    // which slugs the course has.
    let suspects = [
        "602",
        "page_id:602",
        "Must-View",
        "read-me-first",
        "must-view-2",
        "must",
    ];
    let body: String = suspects
        .iter()
        .chain(&["notes-1"])
        .map(|slug| format!(r#"<a href="/courses/101/pages/{slug}">a page</a> "#))
        .collect();
    home_page(&f, &body).await;
    f.get(
        "/courses/101/pages/notes-1",
        page(611, "notes-1", "Lecture notes 1", "<p>Stomata.</p>"),
    )
    .await;
    f.sync(&f.options(false)).await.unwrap();
    assert_eq!(pages_asked(&f).await, ["intro", "notes-1", "notes-2"]);
    let base = f.canvas.uri();
    let record_now = record(&f);
    let would_mark: Vec<_> = record_now
        .not_read
        .iter()
        .filter(|entry| entry.reason == CoverageReason::WouldMarkViewed)
        .map(|entry| (entry.title.as_deref(), entry.url.clone().unwrap()))
        .collect();
    let mut expected = vec![(
        Some("Read me first"),
        "https://lms.example.edu/courses/101/modules/items/12".to_string(),
    )];
    expected.extend(
        suspects
            .iter()
            .map(|slug| (None, format!("{base}/courses/101/pages/{slug}"))),
    );
    assert_eq!(would_mark, expected);

    // With nothing left to view first, a number is an address like any other.
    let f = Fixture::new().await;
    course(
        &f,
        Some("wiki"),
        json!([{"id": "home"}, {"id": "modules"}]),
        json!([{"id": 1, "name": "Week 1", "position": 1, "items": [
            {"id": 12, "type": "Page", "page_url": "must-view", "title": "Read me first",
             "completion_requirement": {"type": "must_view", "completed": true}}
        ]}]),
    )
    .await;
    f.get(
        "/courses/101/front_page",
        page(
            600,
            "home",
            "Welcome",
            r#"<a href="/courses/101/pages/602">a page</a>"#,
        ),
    )
    .await;
    for slug in ["must-view", "602"] {
        f.get(
            &format!("/courses/101/pages/{slug}"),
            page(602, "must-view", "Read me first", "<p>Ribosomes.</p>"),
        )
        .await;
    }
    f.sync(&f.options(false)).await.unwrap();
    assert_eq!(pages_asked(&f).await, ["602", "must-view"]);
}

#[tokio::test]
async fn a_linked_page_that_is_a_must_view_page_is_not_kept_and_the_student_is_told() {
    let f = Fixture::new().await;
    hidden_lists(&f, Some("wiki")).await;
    // An address the page had before it was renamed: nothing in it says which page it is.
    home_page(
        &f,
        r#"<a href="/courses/101/pages/welcome-week">start here</a>"#,
    )
    .await;
    f.get(
        "/courses/101/pages/welcome-week",
        page(
            602,
            "must-view",
            "Read me first",
            "<p>Mitochondria make ATP.</p>",
        ),
    )
    .await;
    let told = "DEMO101: PageLamp opened \"Read me first\", a page a module asks you to view, \
                through a link with another address. Canvas may show it as viewed.";

    let report = f.sync(&f.options(false)).await.unwrap();
    assert_eq!(asked(&f, "/courses/101/pages/welcome-week").await, 1);
    assert!(
        report.warnings.iter().any(|warning| warning == told),
        "{:?}",
        report.warnings
    );
    // Its text isn't kept.
    assert!(!has_material(&f, "/page/602"));
    assert!(
        f.store()
            .search("mitochondria", None, 5)
            .unwrap()
            .is_empty()
    );
    // The address is remembered with the page, which is still one to view first.
    let record_now = record(&f);
    let remembered = &record_now.followed.pages[&format!("{}/page/602", f.source)];
    assert_eq!(
        (remembered.slug.as_str(), remembered.also.as_slice()),
        ("must-view", &["welcome-week".to_string()][..])
    );
    assert!(entries(&record_now).contains(&(
        CoverageArea::Pages,
        CoverageReason::WouldMarkViewed,
        Some("Read me first")
    )));
    assert_eq!(record_now.counts.linked_pages, 1, "notes-2 alone");

    // The next sync doesn't ask for that address again, and has nothing to tell.
    let report = f.sync(&f.options(false)).await.unwrap();
    assert_eq!(asked(&f, "/courses/101/pages/welcome-week").await, 1);
    assert_eq!(asked(&f, "/courses/101/pages/must-view").await, 0);
    assert!(
        !report.warnings.iter().any(|warning| warning == told),
        "{:?}",
        report.warnings
    );
    assert!(!has_material(&f, "/page/602"));
}

#[tokio::test]
async fn another_address_of_a_page_is_remembered_and_never_replaces_the_page() {
    let f = Fixture::new().await;
    hidden_lists(&f, Some("wiki")).await;
    // "introduction" is another address of the module's page (intro), "lecture-notes-1" of a
    // linked page (notes-1).
    home_page(
        &f,
        r#"<a href="/courses/101/pages/introduction">intro</a>
           <a href="/courses/101/pages/notes-1">notes</a>
           <a href="/courses/101/pages/lecture-notes-1">the same notes</a>"#,
    )
    .await;
    f.get(
        "/courses/101/pages/introduction",
        page(
            601,
            "intro",
            "Introduction",
            "<p>Welcome to the course.</p>",
        ),
    )
    .await;
    for slug in ["notes-1", "lecture-notes-1"] {
        f.get(
            &format!("/courses/101/pages/{slug}"),
            page(611, "notes-1", "Lecture notes 1", "<p>Stomata.</p>"),
        )
        .await;
    }
    let id = |n: u32| format!("{}/page/{n}", f.source);
    for sync in 1..=2 {
        f.sync(&f.options(false)).await.unwrap();
        // Each other address is asked for once, in the first sync.
        assert_eq!(asked(&f, "/courses/101/pages/introduction").await, 1);
        assert_eq!(asked(&f, "/courses/101/pages/lecture-notes-1").await, 1);
        assert_eq!(asked(&f, "/courses/101/pages/intro").await, sync);
        assert_eq!(asked(&f, "/courses/101/pages/notes-1").await, sync);
        let record_now = record(&f);
        let intro = &record_now.followed.pages[&id(601)];
        assert_eq!(
            (intro.slug.as_str(), intro.also.as_slice(), intro.linked),
            ("intro", &["introduction".to_string()][..], false),
            "sync {sync}"
        );
        let notes = &record_now.followed.pages[&id(611)];
        assert_eq!(
            (notes.slug.as_str(), notes.also.as_slice(), notes.linked),
            ("notes-1", &["lecture-notes-1".to_string()][..], true),
            "sync {sync}"
        );
        assert_eq!(record_now.counts.linked_pages, 2, "sync {sync}");
        // The module's page keeps its place in the module.
        let materials = f.store().list_materials(&course101(&f)).unwrap();
        let page = material(&materials, "/page/601");
        assert!(page.module_id.is_some(), "sync {sync}");
        assert_eq!(page.title, "Introduction");
    }
}

#[tokio::test]
async fn a_home_page_a_module_asks_the_student_to_view_is_read_once_and_the_student_is_told() {
    let f = Fixture::new().await;
    hidden_lists(&f, Some("wiki")).await;
    // The Home page is the module's page the student is asked to view. Its slug isn't known
    // before Canvas answers.
    f.get(
        "/courses/101/front_page",
        page(602, "must-view", "Read me first", "<p>Ribosomes.</p>"),
    )
    .await;
    f.get(
        "/courses/101/pages/notes-2",
        page(612, "notes-2", "Lecture notes 2", "<p>Chlorophyll.</p>"),
    )
    .await;
    let told = "DEMO101: PageLamp read the Home page \"Read me first\", which a module asks \
                you to view. Canvas may show it as viewed.";
    let must_view = (
        CoverageArea::Pages,
        CoverageReason::WouldMarkViewed,
        Some("Read me first"),
    );

    let report = f.sync(&f.options(false)).await.unwrap();
    assert!(
        report.warnings.iter().any(|warning| warning == told),
        "{:?}",
        report.warnings
    );
    assert_eq!(asked(&f, "/courses/101/front_page").await, 1);
    let first = record(&f);
    assert_eq!(first.home.state, CourseHomeState::Read);
    assert_eq!(
        first.home.material_id,
        Some(format!("{}/page/602", f.source))
    );
    // It was read: the record doesn't also say it wasn't.
    assert!(!entries(&first).contains(&must_view));
    assert_eq!(f.store().search("ribosomes", None, 5).unwrap().len(), 1);

    // From now on the record knows which page the Home is: not asked for again while the
    // module asks the student to view it.
    let report = f.sync(&f.options(false)).await.unwrap();
    assert!(
        !report.warnings.iter().any(|warning| warning == told),
        "{:?}",
        report.warnings
    );
    assert_eq!(asked(&f, "/courses/101/front_page").await, 1);
    assert_eq!(asked(&f, "/courses/101/pages/must-view").await, 0);
    let second = record(&f);
    assert_eq!(second.home, first.home);
    assert!(entries(&second).contains(&must_view));
    assert_eq!(f.store().search("ribosomes", None, 5).unwrap().len(), 1);
}

/// A course whose Pages list is read and names one page, changed at `updated`.
async fn one_listed_page(f: &Fixture, updated: &str, body: &str) {
    course(
        f,
        Some("modules"),
        json!([{"id": "home"}, {"id": "modules"}, {"id": "pages"}]),
        json!([]),
    )
    .await;
    let listed = json!({"page_id": 601, "url": "week-1", "title": "Week 1", "updated_at": updated});
    f.get("/courses/101/pages", json!([listed])).await;
    let mut read = listed.clone();
    read["body"] = json!(body);
    f.get("/courses/101/pages/week-1", read).await;
}

#[tokio::test]
async fn a_page_that_changed_while_it_could_not_be_read_is_read_when_it_can_be() {
    let f = Fixture::new().await;
    one_listed_page(&f, "2026-09-08T10:00:00Z", "<p>Xylem.</p>").await;
    f.sync(&f.options(false)).await.unwrap();
    assert_eq!(asked(&f, "/courses/101/pages/week-1").await, 1);

    // The page is edited, and in this sync the module list fails: no page is read.
    f.canvas.reset().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/courses/101/modules"))
        .respond_with(ResponseTemplate::new(500))
        .with_priority(1)
        .mount(&f.canvas)
        .await;
    one_listed_page(&f, "2026-09-15T10:00:00Z", "<p>Phloem.</p>").await;
    f.sync(&f.options(false)).await.unwrap();
    assert_eq!(asked(&f, "/courses/101/pages/week-1").await, 0);
    assert_eq!(f.store().search("xylem", None, 5).unwrap().len(), 1);

    // The next sync can read it, and does: the text PageLamp has is still the old one.
    f.canvas.reset().await;
    one_listed_page(&f, "2026-09-15T10:00:00Z", "<p>Phloem.</p>").await;
    f.sync(&f.options(false)).await.unwrap();
    assert_eq!(asked(&f, "/courses/101/pages/week-1").await, 1);
    assert_eq!(f.store().search("phloem", None, 5).unwrap().len(), 1);
    assert!(f.store().search("xylem", None, 5).unwrap().is_empty());
}

#[tokio::test]
async fn a_locked_linked_page_is_still_noted_by_a_sync_that_does_not_ask_again() {
    let f = Fixture::new().await;
    hidden_lists(&f, Some("wiki")).await;
    home_page(&f, r#"<a href="/courses/101/pages/notes-1">notes</a>"#).await;
    f.get(
        "/courses/101/pages/notes-1",
        json!({"page_id": 611, "url": "notes-1", "title": "Lecture notes 1",
               "locked_for_user": true}),
    )
    .await;
    let start = Utc::now();
    let locked = (
        CoverageArea::Pages,
        CoverageReason::Locked,
        Some("Lecture notes 1"),
    );
    let id = format!("{}/page/611", f.source);
    for (automatic, hours) in [(false, 0), (true, 1)] {
        sync_at(&f, automatic, start + TimeDelta::hours(hours)).await;
        assert_eq!(asked(&f, "/courses/101/pages/notes-1").await, 1);
        let record_now = record(&f);
        assert!(
            entries(&record_now).contains(&locked),
            "automatic: {automatic}"
        );
        assert_eq!(
            record_now.unread.get(&id),
            Some(&CoverageReason::Locked),
            "automatic: {automatic}"
        );
        let materials = f.store().list_materials(&course101(&f)).unwrap();
        assert_ne!(
            material(&materials, "/page/611").text_status,
            TextStatus::Ok
        );
    }
}

#[tokio::test]
async fn a_listed_front_page_is_not_the_home_when_the_home_shows_the_modules() {
    let f = Fixture::new().await;
    course(
        &f,
        Some("modules"),
        json!([{"id": "home"}, {"id": "modules"}, {"id": "pages"}]),
        json!([]),
    )
    .await;
    let listed = json!({"page_id": 601, "url": "week-1", "title": "Week 1",
                        "updated_at": "2026-09-08T10:00:00Z", "front_page": true});
    f.get("/courses/101/pages", json!([listed])).await;
    let mut read = listed.clone();
    read["body"] = json!("<p>Xylem.</p>");
    f.get("/courses/101/pages/week-1", read).await;
    f.sync(&f.options(false)).await.unwrap();
    assert_eq!(asked(&f, "/courses/101/front_page").await, 0);
    let home = record(&f).home;
    assert_eq!(
        (home.kind, home.state, home.material_id),
        (CourseHomeKind::Modules, CourseHomeState::NotAPage, None)
    );
    // It is a page of the list like any other.
    assert!(has_material(&f, "/page/601"));
}

#[tokio::test]
async fn a_dead_link_takes_a_place_under_the_limit() {
    let f = Fixture::new().await;
    hidden_lists(&f, Some("wiki")).await;
    let links: String = (0..45)
        .map(|n| format!(r#"<a href="/courses/101/pages/p{n:02}">p</a> "#))
        .collect();
    home_page(&f, &links).await;
    // The first five lead nowhere.
    for n in 5..45 {
        f.get(
            &format!("/courses/101/pages/p{n:02}"),
            page(1000 + n, &format!("p{n:02}"), "A page", "<p>text</p>"),
        )
        .await;
    }
    let start = Utc::now();
    // A sync the student starts, then the app's own an hour later, which reads no page again
    // that it has: the same forty links are taken, the same six are left.
    for (automatic, hours, dead, alive) in [(false, 0, 1, 1), (true, 1, 2, 1)] {
        sync_at(&f, automatic, start + TimeDelta::hours(hours)).await;
        assert_eq!(asked(&f, "/courses/101/pages/p00").await, dead);
        assert_eq!(asked(&f, "/courses/101/pages/p04").await, dead);
        assert_eq!(asked(&f, "/courses/101/pages/p05").await, alive);
        assert_eq!(asked(&f, "/courses/101/pages/p39").await, alive);
        assert_eq!(asked(&f, "/courses/101/pages/p40").await, 0);
        assert_eq!(asked(&f, "/courses/101/pages/notes-2").await, 0);
        let record_now = record(&f);
        assert_eq!(record_now.counts.linked_pages, 35);
        assert_eq!(record_now.counts.capped, 6);
        let failed = record_now
            .not_read
            .iter()
            .filter(|entry| entry.reason == CoverageReason::FailedThisSync)
            .count();
        assert_eq!(failed, 5);
    }
}

#[tokio::test]
async fn more_linked_files_than_the_limit_are_left_alone() {
    let f = Fixture::new().await;
    hidden_lists(&f, Some("wiki")).await;
    let links: String = (2000..2151)
        .map(|id| format!(r#"<a href="/courses/101/files/{id}">f</a> "#))
        .collect();
    home_page(&f, &links).await;
    for id in 2000..2151 {
        f.get(
            &format!("/courses/101/files/{id}"),
            f.file(id, &format!("file-{id}.txt"), 20),
        )
        .await;
    }
    let base = f.canvas.uri();
    // The second sync asks about no file again (each was asked about today), and still
    // leaves the same one out.
    for _ in 0..2 {
        f.sync(&f.options(false)).await.unwrap();
        assert_eq!(asked(&f, "/courses/101/files/2000").await, 1);
        assert_eq!(asked(&f, "/courses/101/files/2149").await, 1);
        assert_eq!(asked(&f, "/courses/101/files/2150").await, 0);
        let record_now = record(&f);
        assert_eq!(record_now.counts.linked_files, 150);
        let capped: Vec<_> = record_now
            .not_read
            .iter()
            .filter(|entry| entry.reason == CoverageReason::Capped)
            .map(|entry| (entry.area, entry.url.clone().unwrap()))
            .collect();
        assert_eq!(
            capped,
            [(
                CoverageArea::Files,
                format!("{base}/courses/101/files/2150")
            )]
        );
    }
}

#[tokio::test]
async fn pages_are_read_when_canvas_says_the_student_has_no_modules() {
    for status in [403, 404] {
        let f = Fixture::new().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/courses/101/modules"))
            .respond_with(ResponseTemplate::new(status))
            .with_priority(1)
            .mount(&f.canvas)
            .await;
        hidden_lists(&f, Some("wiki")).await;
        home_with_links(&f).await;
        f.get(
            "/courses/101/pages/must-view",
            page(602, "must-view", "Read me first", "<p>Ribosomes.</p>"),
        )
        .await;
        f.sync(&f.options(false)).await.unwrap();
        // That is an answer: no module asks the student to view anything, so every page the
        // Home page links to is read.
        assert_eq!(
            pages_asked(&f).await,
            ["intro", "must-view", "notes-1", "notes-2"],
            "{status}"
        );
        assert_eq!(asked(&f, "/courses/101/front_page").await, 1, "{status}");
        let record_now = record(&f);
        assert_eq!(
            record_now.modules_list,
            CoverageListState::Hidden,
            "{status}"
        );
        assert_eq!(record_now.home.state, CourseHomeState::Read, "{status}");
        assert!(
            !entries(&record_now)
                .iter()
                .any(|(_, reason, _)| *reason == CoverageReason::FailedThisSync),
            "{status}"
        );
    }
}

#[tokio::test]
async fn a_failed_module_list_stops_every_page_read_with_a_visible_pages_list_too() {
    let f = Fixture::new().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/courses/101/modules"))
        .respond_with(ResponseTemplate::new(500))
        .with_priority(1)
        .mount(&f.canvas)
        .await;
    f.standard().await;
    f.sync(&f.options(false)).await.unwrap();
    // The list is read; the page it names isn't, since a module could ask the student to
    // view it.
    assert_eq!(asked(&f, "/courses/101/pages").await, 1);
    assert_eq!(pages_asked(&f).await, Vec::<String>::new());
    assert_eq!(asked(&f, "/courses/101/front_page").await, 0);
    let record_now = record(&f);
    assert_eq!(record_now.modules_list, CoverageListState::Failed);
    assert_eq!(record_now.pages_list, CoverageListState::Read);
    assert!(entries(&record_now).contains(&(
        CoverageArea::Pages,
        CoverageReason::FailedThisSync,
        None
    )));
    // The page is listed without its text, and the record says why.
    let materials = f.store().list_materials(&course101(&f)).unwrap();
    assert_ne!(
        material(&materials, "/page/601").text_status,
        TextStatus::Ok
    );
    assert_eq!(
        record_now.unread.get(&format!("{}/page/601", f.source)),
        Some(&CoverageReason::FailedThisSync)
    );
}

#[tokio::test]
async fn a_stopped_sync_asks_for_neither_the_home_page_nor_a_linked_file() {
    use pagelamp_core::ingest::Extractor;
    use pagelamp_core::source::CancelFlag;

    // Stopped while the announcements are read: the Home page would be the next request.
    let f = Fixture::new().await;
    let cancel = CancelFlag::new();
    Mock::given(method("GET"))
        .and(path("/api/v1/announcements"))
        .respond_with(StopsTheSync(cancel.clone(), json!([])))
        .with_priority(1)
        .mount(&f.canvas)
        .await;
    hidden_lists(&f, Some("wiki")).await;
    home_with_links(&f).await;
    let mut options = f.options(false);
    options.extractor = Extractor::default().cancellable(cancel);
    let err = f.sync(&options).await.unwrap_err();
    assert!(err.cancelled, "{err:?}");
    assert_eq!(asked(&f, "/courses/101/assignments").await, 1);
    assert_eq!(asked(&f, "/courses/101/front_page").await, 0);

    // Stopped while the last linked page is read: the linked files would be next.
    let f = Fixture::new().await;
    let cancel = CancelFlag::new();
    Mock::given(method("GET"))
        .and(path("/api/v1/courses/101/pages/notes-2"))
        .respond_with(StopsTheSync(
            cancel.clone(),
            page(612, "notes-2", "Lecture notes 2", "<p>text</p>"),
        ))
        .with_priority(1)
        .mount(&f.canvas)
        .await;
    hidden_lists(&f, Some("wiki")).await;
    home_with_links(&f).await;
    let mut options = f.options(false);
    options.extractor = Extractor::default().cancellable(cancel);
    let err = f.sync(&options).await.unwrap_err();
    assert!(err.cancelled, "{err:?}");
    assert_eq!(asked(&f, "/courses/101/pages/notes-2").await, 1);
    for id in [701, 702, 705] {
        assert_eq!(asked(&f, &format!("/courses/101/files/{id}")).await, 0);
    }
}

#[tokio::test]
async fn a_body_with_more_links_than_are_looked_at_is_noted() {
    let f = Fixture::new().await;
    hidden_lists(&f, Some("wiki")).await;
    let many: String = (0..=pagelamp_extract::links::MAX_LINKS)
        .map(|n| format!(r#"<a href="https://example.org/{n}">a reading</a> "#))
        .collect();
    home_page(&f, &many).await;
    let start = Utc::now();
    let base = f.canvas.uri();
    // The app's own sync an hour later doesn't read the Home page again, and still says so.
    for (automatic, hours) in [(false, 0), (true, 1)] {
        let report = sync_at(&f, automatic, start + TimeDelta::hours(hours)).await;
        assert_eq!(asked(&f, "/courses/101/front_page").await, 1);
        let record_now = record(&f);
        let capped: Vec<_> = record_now
            .not_read
            .iter()
            .filter(|entry| entry.reason == CoverageReason::Capped)
            .map(|entry| (entry.area, entry.title.clone(), entry.url.clone().unwrap()))
            .collect();
        assert_eq!(
            capped,
            [(
                CoverageArea::Pages,
                Some("Welcome".to_string()),
                format!("{base}/courses/101/pages/home")
            )],
            "automatic: {automatic}"
        );
        assert_eq!(record_now.counts.capped, 1);
        // The summary counts it with the page the module asks the student to view.
        assert_eq!(report.course_summaries[0].not_read, 2);
    }
}

#[tokio::test]
async fn a_link_no_request_can_be_built_from_is_left_alone() {
    let f = Fixture::new().await;
    hidden_lists(&f, Some("wiki")).await;
    let with_a_user = f.canvas.uri().replace("http://", "http://alice:hunter2@");
    let body = format!(
        r#"<a href="/courses/101/files/{}">a file</a>
           <a href="{with_a_user}/courses/101/pages/notes-1">notes</a>
           <a href="{with_a_user}/courses/101/quizzes/4">a quiz</a>
           <a href="/courses/101/pages/{}">a page</a>"#,
        "7".repeat(65_000),
        "a".repeat(3_000)
    );
    home_page(&f, &body).await;
    // The sync doesn't fail over them, and asks for none of them.
    f.sync(&f.options(false)).await.unwrap();
    assert_eq!(pages_asked(&f).await, ["intro", "notes-2"]);
    assert!(
        !f.canvas
            .received_requests()
            .await
            .unwrap()
            .iter()
            .any(|request| request.url.path().contains("/files/"))
    );
    // An address with a user name or a password in it isn't kept.
    let stored = serde_json::to_string(&record(&f)).unwrap();
    assert!(!stored.contains("alice") && !stored.contains("hunter2"));
    assert!(!stored.contains("quizzes/4"));
}

#[tokio::test]
async fn a_link_to_an_announcement_that_was_synced_is_not_noted() {
    let f = Fixture::new().await;
    let posted = (Utc::now() - TimeDelta::days(2)).to_rfc3339();
    Mock::given(method("GET"))
        .and(path("/api/v1/announcements"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": 701, "title": "Room change", "posted_at": posted,
             "message": "<p>Room 2. See <a href=\"/courses/101/discussion_topics/702\">the thread</a>.</p>"}
        ])))
        .with_priority(1)
        .mount(&f.canvas)
        .await;
    hidden_lists(&f, Some("wiki")).await;
    home_page(
        &f,
        r#"<a href="/courses/101/discussion_topics/701">the room change</a>
           <a href="/courses/101/discussion_topics/999">a discussion</a>"#,
    )
    .await;
    f.sync(&f.options(false)).await.unwrap();
    assert!(has_material(&f, "/announcement/701"));
    let base = f.canvas.uri();
    let discussions: Vec<String> = record(&f)
        .not_read
        .iter()
        .filter(|entry| entry.area == CoverageArea::Discussions)
        .map(|entry| entry.url.clone().unwrap())
        .collect();
    // The announcement is read; the two discussions aren't.
    assert_eq!(
        discussions,
        [
            format!("{base}/courses/101/discussion_topics/999"),
            format!("{base}/courses/101/discussion_topics/702"),
        ]
    );
}

#[tokio::test]
async fn when_the_navigation_cannot_be_read_no_list_is_asked_for_and_nothing_is_removed() {
    let f = Fixture::new().await;
    f.standard().await;
    f.sync(&f.options(false)).await.unwrap();
    // Only the Files list names file 503.
    assert!(has_material(&f, "/file/503"));

    f.canvas.reset().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/courses/101/tabs"))
        .respond_with(ResponseTemplate::new(500))
        .with_priority(1)
        .mount(&f.canvas)
        .await;
    f.standard().await;
    for (id, name) in [(501, "slides.txt"), (502, "notes.txt")] {
        f.get(&format!("/courses/101/files/{id}"), f.file(id, name, 20))
            .await;
    }
    let report = f.sync(&f.options(false)).await.unwrap();
    // Which lists the course hides isn't known: neither is asked for.
    assert_eq!(asked(&f, "/courses/101/files").await, 0);
    assert_eq!(asked(&f, "/courses/101/pages").await, 0);
    // The one warning says what couldn't be read.
    let about_101: Vec<&String> = report
        .warnings
        .iter()
        .filter(|warning| warning.starts_with("DEMO101:"))
        .collect();
    assert_eq!(about_101.len(), 1, "{:?}", report.warnings);
    assert!(
        about_101[0].starts_with("DEMO101: tabs not available"),
        "{:?}",
        report.warnings
    );
    // What the modules name is read as before, and nothing is removed.
    assert_eq!(asked(&f, "/courses/101/pages/week-1-overview").await, 1);
    assert_eq!(asked(&f, "/courses/101/files/501").await, 1);
    for suffix in ["/file/501", "/file/502", "/file/503", "/page/601"] {
        assert!(has_material(&f, suffix), "{suffix}");
    }
    let record_now = record(&f);
    assert_eq!(
        (record_now.pages_list, record_now.files_list),
        (CoverageListState::Failed, CoverageListState::Failed)
    );
    for area in [CoverageArea::Pages, CoverageArea::Files] {
        assert!(entries(&record_now).contains(&(area, CoverageReason::FailedThisSync, None)));
    }
}
