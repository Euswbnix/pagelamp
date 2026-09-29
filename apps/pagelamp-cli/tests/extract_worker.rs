//! The extraction worker end to end: the real `pagelamp` binary (with its counting allocator)
//! as `pagelamp extract-worker`, driven by `pagelamp_extract::worker`. Synthetic files only:
//! the PDF "bombs" are built here with `lopdf`.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, Stream, dictionary};
use pagelamp_extract::ExtractError;
use pagelamp_extract::worker::{
    WorkerFailure, WorkerLimits, extract_in_worker, extract_in_worker_with,
};

fn worker() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_pagelamp"))
}

fn write(dir: &tempfile::TempDir, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

/// A PDF whose pages each hold `content` (already-encoded content-stream bytes), compressed.
fn pdf(pages: usize, content: &[u8]) -> Vec<u8> {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.to_vec()));
    let resources = doc.add_object(dictionary! { "Font" => dictionary! { "F1" => font_id } });
    let media_box: Vec<Object> = vec![0.into(), 0.into(), 612.into(), 792.into()];
    let kids: Vec<Object> = (0..pages)
        .map(|_| {
            doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => content_id,
                "Resources" => resources,
                "MediaBox" => media_box.clone(),
            })
            .into()
        })
        .collect();
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => kids, "Count" => pages as i64,
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    doc.compress();
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

fn text_page(text: &str) -> Vec<u8> {
    Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 24.into()]),
            Operation::new("Td", vec![72.into(), 700.into()]),
            Operation::new("Tj", vec![Object::string_literal(text)]),
            Operation::new("ET", vec![]),
        ],
    }
    .encode()
    .unwrap()
}

fn limits(memory_mb: u64, cpu_seconds: u64) -> WorkerLimits {
    WorkerLimits {
        memory_bytes: memory_mb * 1024 * 1024,
        cpu_seconds,
    }
}

#[test]
fn a_readable_file_comes_back_as_segments() {
    let dir = tempfile::tempdir().unwrap();
    let notes = write(&dir, "notes.md", b"# Week 1\nphotosynthesis demo notes\n");
    let segments = extract_in_worker(&worker(), &notes, None, WorkerLimits::default())
        .unwrap()
        .unwrap();
    assert!(segments[0].text.contains("photosynthesis"), "{segments:?}");

    let slides = write(&dir, "slides.pdf", &pdf(2, &text_page("Stomata page")));
    let segments = extract_in_worker(&worker(), &slides, None, WorkerLimits::default())
        .unwrap()
        .unwrap();
    assert_eq!(segments.len(), 2);
    assert_eq!(segments[0].locator.as_deref(), Some("p. 1"));
    assert!(segments[0].text.contains("Stomata page"));

    // The extractor's own answers pass through unchanged.
    let video = write(&dir, "lecture.mp4", b"\0\0\0 ftypmp42");
    assert!(matches!(
        extract_in_worker(&worker(), &video, None, WorkerLimits::default()),
        Ok(Err(ExtractError::Unsupported(_)))
    ));
}

#[test]
fn a_token_dense_page_hits_the_memory_cap() {
    // ~20 MB of drawing operators: small once compressed and within the stream caps, but each
    // six-byte operator becomes an allocated operation when the page is parsed.
    let content = b"0 0 m ".repeat(3_500_000);
    let dir = tempfile::tempdir().unwrap();
    let bomb = write(&dir, "dense.pdf", &pdf(1, &content));
    assert!(std::fs::metadata(&bomb).unwrap().len() < 2 * 1024 * 1024);
    assert_eq!(
        extract_in_worker(&worker(), &bomb, None, limits(64, 120)).unwrap_err(),
        WorkerFailure::MemoryLimit
    );
}

#[test]
fn a_huge_page_tree_hits_the_cpu_budget() {
    // Reading a page walks the whole page tree, so the work grows with the square of the page
    // count; 4,500 pages stay under the page limit.
    let dir = tempfile::tempdir().unwrap();
    let bomb = write(&dir, "pages.pdf", &pdf(4_500, &text_page("x")));
    assert_eq!(
        extract_in_worker(&worker(), &bomb, None, limits(512, 1)).unwrap_err(),
        WorkerFailure::CpuLimit
    );
}

/// Budget for the median cost of one worker (plan §M0.5). Measured on the GitHub runners with
/// this unoptimised test build (2026-09-29): Windows 45 ms, macOS 46 ms, Linux 20 ms.
const SPAWN_BUDGET: Duration = Duration::from_millis(150);

#[test]
fn spawn_cost_is_within_budget() {
    // One worker per file: the fixed cost of starting one is paid by every extracted file.
    let dir = tempfile::tempdir().unwrap();
    let file = write(&dir, "tiny.txt", b"demo");
    let mut times: Vec<Duration> = (0..15)
        .map(|_| {
            let started = Instant::now();
            extract_in_worker(&worker(), &file, None, WorkerLimits::default())
                .unwrap()
                .unwrap();
            started.elapsed()
        })
        .collect();
    times.sort();
    let median = times[times.len() / 2];
    let line = format!(
        "extract-worker spawn cost on {}: median {} ms, min {} ms, max {} ms",
        std::env::consts::OS,
        median.as_millis(),
        times[0].as_millis(),
        times[times.len() - 1].as_millis()
    );
    // Written to the real stdout, past the test harness's capture, so every CI log shows it;
    // on GitHub Actions as a notice in the run summary.
    let line = if std::env::var_os("GITHUB_ACTIONS").is_some() {
        format!("::notice title=extract-worker spawn cost::{line}\n")
    } else {
        format!("{line}\n")
    };
    let _ = std::io::Write::write_all(&mut std::io::stdout(), line.as_bytes());
    assert!(median <= SPAWN_BUDGET, "median {median:?}");
}

/// Test faults exist only in debug builds of the worker.
#[cfg(debug_assertions)]
mod faults {
    use super::*;

    fn fault(name: &str, limits: WorkerLimits, wall: Duration) -> WorkerFailure {
        let dir = tempfile::tempdir().unwrap();
        let file = write(&dir, "notes.txt", b"demo");
        extract_in_worker_with(&worker(), &file, None, limits, wall, Some(name)).unwrap_err()
    }

    fn classified(name: &str, limits: WorkerLimits, wall: Duration, expected: WorkerFailure) {
        let started = Instant::now();
        assert_eq!(fault(name, limits, wall), expected, "{name}");
        assert!(
            started.elapsed() < wall + Duration::from_secs(10),
            "{name} took {:?}",
            started.elapsed()
        );
    }

    /// What the worker's `console` fault says when started through `extract_in_worker_with`.
    fn console_via_parent() -> String {
        let dir = tempfile::tempdir().unwrap();
        let file = write(&dir, "notes.txt", b"demo");
        let wall = Duration::from_secs(60);
        match extract_in_worker_with(
            &worker(),
            &file,
            None,
            WorkerLimits::default(),
            wall,
            Some("console"),
        ) {
            Ok(Err(ExtractError::Failed(answer))) => answer,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_worker_has_no_console_window() {
        // On Windows it is spawned with CREATE_NO_WINDOW, so the GUI app never flashes a console.
        assert_eq!(console_via_parent(), "no window");
    }

    /// The same worker started as a plain child of this test process (inheriting its console).
    #[cfg(windows)]
    fn console_as_plain_child() -> String {
        use std::io::{Read, Write};
        use std::process::{Command, Stdio};
        let request = serde_json::json!({
            "protocol": pagelamp_extract::worker::PROTOCOL,
            "path": "notes.txt",
            "mime": null,
            "memory_bytes": WorkerLimits::default().memory_bytes,
            "cpu_seconds": WorkerLimits::default().cpu_seconds,
            "debug_fault": "console",
        });
        let mut child = Command::new(worker())
            .args([
                pagelamp_extract::worker::SUBCOMMAND,
                "--protocol",
                &pagelamp_extract::worker::PROTOCOL.to_string(),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(request.to_string().as_bytes())
            .unwrap();
        let mut output = String::new();
        child
            .stdout
            .take()
            .unwrap()
            .read_to_string(&mut output)
            .unwrap();
        child.wait().unwrap();
        let response: serde_json::Value = serde_json::from_str(&output).unwrap();
        response["outcome"]["message"].as_str().unwrap().to_string()
    }

    #[cfg(windows)]
    #[test]
    fn without_the_flag_the_worker_would_have_a_console_window() {
        // Shows the check above can fail: without CREATE_NO_WINDOW the worker shares this test
        // process's console. (A test process without any console can't show the difference.)
        let answer = console_as_plain_child();
        assert!(answer == "window" || answer == "no window", "{answer}");
        // Past the test capture, so the CI log shows whether the comparison meant anything.
        let line = if answer == "window" {
            "a plain child has a console window, so CREATE_NO_WINDOW is what removes it\n"
        } else {
            "this test process has no console window; nothing to compare with\n"
        };
        let _ = std::io::Write::write_all(&mut std::io::stdout(), line.as_bytes());
    }

    #[test]
    fn the_worker_gets_no_environment() {
        // No PAGELAMP_SECRET_*, HOME, PATH or anything else reaches the worker (Windows keeps
        // SystemRoot, which it needs to start a process).
        let dir = tempfile::tempdir().unwrap();
        let file = write(&dir, "notes.txt", b"demo");
        let names = match extract_in_worker_with(
            &worker(),
            &file,
            None,
            WorkerLimits::default(),
            Duration::from_secs(60),
            Some("env"),
        ) {
            Ok(Err(ExtractError::Failed(names))) => names,
            other => panic!("{other:?}"),
        };
        // macOS itself sets __CF_USER_TEXT_ENCODING in every new process.
        let names: Vec<&str> = names
            .split(',')
            .filter(|name| !name.is_empty() && !name.starts_with("__CF_"))
            .collect();
        let expected: &[&str] = if cfg!(windows) { &["SystemRoot"] } else { &[] };
        assert_eq!(names, expected);
    }

    #[test]
    fn a_hanging_worker_is_timed_out() {
        classified(
            "hang",
            WorkerLimits::default(),
            Duration::from_secs(2),
            WorkerFailure::TimedOut,
        );
    }

    #[test]
    fn a_spinning_worker_is_stopped_at_its_cpu_budget() {
        classified(
            "spin",
            limits(512, 1),
            Duration::from_secs(60),
            WorkerFailure::CpuLimit,
        );
    }

    #[test]
    fn a_growing_worker_is_stopped_at_its_memory_cap() {
        classified(
            "allocate",
            limits(64, 120),
            Duration::from_secs(60),
            WorkerFailure::MemoryLimit,
        );
    }

    #[test]
    fn a_dying_worker_is_crashed_and_a_babbling_one_bad_output() {
        let wall = Duration::from_secs(60);
        classified(
            "abort",
            WorkerLimits::default(),
            wall,
            WorkerFailure::Crashed,
        );
        classified(
            "garbage",
            WorkerLimits::default(),
            wall,
            WorkerFailure::BadOutput,
        );
    }
}

/// A store with one course and one material for `index_file_using`.
fn store_with_material(id: &str) -> pagelamp_core::Store {
    use pagelamp_core::model::*;
    let store = pagelamp_core::Store::open_in_memory().unwrap();
    store
        .upsert_source(&SourceRecord {
            id: "folder:demo".into(),
            kind: SourceKind::Folder,
            label: "Demo courses".into(),
            config: serde_json::json!({ "path": "/demo/courses" }),
            last_synced_at: None,
            last_error: None,
            last_error_kind: None,
        })
        .unwrap();
    store
        .upsert_course(&CourseUpsert {
            id: "folder:demo/course/DEMO101".into(),
            source_id: "folder:demo".into(),
            external_id: "DEMO101".into(),
            code: Some("DEMO101".into()),
            name: "Intro to Demo Studies".into(),
            term_start: None,
            term_end: None,
            url: None,
            syllabus_text: None,
            lms: Default::default(),
        })
        .unwrap();
    store
        .upsert_material(&MaterialUpsert {
            id: id.into(),
            course_id: "folder:demo/course/DEMO101".into(),
            module_id: None,
            kind: MaterialKind::File,
            title: "dense.pdf".into(),
            url: None,
            local_path: None,
            mime: None,
            published_at: None,
            week_hint: None,
        })
        .unwrap();
    store
}

#[test]
fn a_hard_failure_is_not_extracted_again_on_the_next_sync() {
    use pagelamp_core::ingest::{self, Extractor, IndexOutcome};
    use pagelamp_core::model::{TextErrorKind, TextStatus};
    let id = "folder:demo/course/DEMO101/material/dense.pdf";
    let store = store_with_material(id);
    let dir = tempfile::tempdir().unwrap();
    let bomb = write(&dir, "dense.pdf", &pdf(1, &b"0 0 m ".repeat(3_500_000)));

    let capped = Extractor::worker_with_limits(worker(), limits(64, 120));
    let first = ingest::index_file_using(&store, id, &bomb, None, &capped).unwrap();
    assert!(matches!(first, IndexOutcome::Failed(_)), "{first:?}");
    let material = store.get_material(id).unwrap().unwrap();
    assert_eq!(material.text_status, TextStatus::Error);
    assert_eq!(material.text_error_kind, Some(TextErrorKind::MemoryLimit));
    assert_eq!(
        material.text_error_fingerprint,
        Some(ingest::failure_fingerprint())
    );

    // The next sync doesn't start a worker for it at all: one that can't even start would
    // have made this a `Deferred(SpawnFailed)`.
    let next = Extractor::worker(dir.path().join("no-such-pagelamp"));
    assert_eq!(
        ingest::index_file_using(&store, id, &bomb, None, &next).unwrap(),
        IndexOutcome::Skipped(TextErrorKind::MemoryLimit)
    );
    assert!(next.take_warning().is_none());
}

#[test]
fn doctor_checks_a_real_worker() {
    let took = pagelamp_extract::worker::check(&worker()).unwrap();
    assert!(took < Duration::from_secs(15), "{took:?}");
    let missing = std::env::temp_dir().join("no-such-pagelamp-worker");
    assert_eq!(
        pagelamp_extract::worker::check(&missing).unwrap_err(),
        WorkerFailure::SpawnFailed
    );
}

#[test]
fn a_stale_worker_is_a_protocol_mismatch() {
    // `pagelamp extract-worker --protocol 2` answers with its own version.
    let output = std::process::Command::new(worker())
        .args(["extract-worker", "--protocol", "2"])
        .env_clear()
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains(r#""kind":"protocol_mismatch""#), "{text}");
}

#[test]
fn a_cancelled_sync_stops_the_worker_mid_file() {
    // The page-tree bomb keeps a worker busy for many seconds at the default limits.
    let dir = tempfile::tempdir().unwrap();
    let bomb = write(&dir, "pages.pdf", &pdf(4_500, &text_page("x")));
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let setter = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        setter.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    let started = Instant::now();
    let result = pagelamp_extract::worker::extract_in_worker_cancellable(
        &worker(),
        &bomb,
        None,
        WorkerLimits::default(),
        &cancel,
    );
    assert_eq!(result.unwrap_err(), WorkerFailure::Cancelled);
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
}
