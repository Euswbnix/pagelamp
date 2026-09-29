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

#[test]
fn spawn_cost_is_measured() {
    // One worker per file: the fixed cost of starting one is paid by every extracted file.
    // The median is printed for the CI log (`--nocapture`); the budget is loose enough for an
    // unoptimised test build on a shared runner and catches pathological regressions.
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
    println!(
        "extract-worker spawn cost on {}: median {} ms, min {} ms, max {} ms",
        std::env::consts::OS,
        median.as_millis(),
        times[0].as_millis(),
        times[times.len() - 1].as_millis()
    );
    assert!(median < Duration::from_millis(1500), "median {median:?}");
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
