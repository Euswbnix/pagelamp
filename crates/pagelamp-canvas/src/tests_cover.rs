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
    // Hidden lists are in the record now; the Files warning stays as it was.
    assert_eq!(
        report.warnings,
        ["DEMO101: Files tab hidden, used module items only"]
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
    // (reading a page shows in Canvas as the student viewing it), and the pages, the files
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

    // A sync nobody is at the app for doesn't touch the record.
    let light = SyncOptions {
        automatic: true,
        user_level_only: true,
        ..f.options(false)
    };
    sync_with(&f.api_for(true), &f.db, &f.source, &light, &no_progress)
        .await
        .unwrap();
    assert_eq!(record(&f), third);
}

#[tokio::test]
async fn the_home_page_is_asked_for_only_when_the_home_is_a_page() {
    // The Home shows the modules: no request.
    let f = Fixture::new().await;
    hidden_lists(&f, Some("modules")).await;
    let report = f.sync(&f.options(false)).await.unwrap();
    assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
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
    assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
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
        assert_eq!(report.warnings.len(), 1, "{status}: {:?}", report.warnings);
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
    assert_eq!(
        report.warnings,
        ["DEMO202: Files tab hidden, used module items only"]
    );
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
    assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
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
async fn links_are_not_followed_when_the_modules_could_not_be_read() {
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
    // The Home page itself is asked for; what it links to isn't (files don't count as viewed
    // pages, so they are).
    assert_eq!(asked(&f, "/courses/101/front_page").await, 1);
    assert_eq!(asked(&f, "/courses/101/pages/notes-1").await, 0);
    assert_eq!(asked(&f, "/courses/101/pages/notes-2").await, 0);
    assert_eq!(asked(&f, "/courses/101/pages/must-view").await, 0);
    assert_eq!(asked(&f, "/courses/101/pages/intro").await, 0);
    assert_eq!(asked(&f, "/courses/101/files/701").await, 1);
    let record_now = record(&f);
    assert_eq!(record_now.modules_list, CoverageListState::Failed);
    assert_eq!(
        record_now
            .not_read
            .iter()
            .filter(|entry| entry.area == CoverageArea::Pages
                && entry.reason == CoverageReason::FailedThisSync)
            .count(),
        4,
        "the two linked pages and the two module pages, which only the Home page names now"
    );
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

#[tokio::test]
async fn a_download_takes_linked_files_too() {
    let f = Fixture::new().await;
    hidden_lists(&f, Some("wiki")).await;
    home_with_links(&f).await;
    f.sync(&f.options(false)).await.unwrap();
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
