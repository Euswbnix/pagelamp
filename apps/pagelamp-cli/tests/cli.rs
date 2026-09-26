//! The `pagelamp` binary end to end, with a temporary PAGELAMP_HOME and synthetic course
//! folders. Commands that would store secrets in the real OS keychain are only exercised up to
//! their input validation.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Output, Stdio};

use serde_json::{Value, json};

fn pagelamp(home: &Path, args: &[&str]) -> Output {
    pagelamp_with_stdin(home, args, "")
}

fn pagelamp_with_stdin(home: &Path, args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_pagelamp"))
        .args(args)
        .env("PAGELAMP_HOME", home)
        .env_remove("RUST_LOG")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn ok(output: &Output) -> String {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn json_out(output: &Output) -> Value {
    serde_json::from_str(&ok(output)).unwrap()
}

fn write(root: &Path, relative: &str, content: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

fn demo_courses(root: &Path) {
    write(
        root,
        "DEMO101 Intro to Demo Studies/Week 1/notes.md",
        "# Basics\nphotosynthesis converts light",
    );
    write(
        root,
        "DEMO101 Intro to Demo Studies/Week 2/cycle.txt",
        "the calvin cycle fixes carbon",
    );
    write(
        root,
        "DEMO202 Advanced Demo Studies/kinetics.md",
        "enzyme kinetics",
    );
}

#[test]
fn folder_add_sync_and_read_commands() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let courses = temp.path().join("Courses");
    demo_courses(&courses);

    let empty = pagelamp(&home, &["sync"]);
    assert!(ok(&empty).contains("No sources yet"));

    let added = pagelamp(
        &home,
        &[
            "folder",
            "add",
            courses.to_str().unwrap(),
            "--term-start",
            "2026-09-07",
        ],
    );
    let stdout = ok(&added);
    assert!(stdout.contains("Added Courses (folder:"), "{stdout}");
    let stderr = String::from_utf8_lossy(&added.stderr);
    assert!(
        stderr.contains("stores nothing remotely"),
        "first add shows the disclosure: {stderr}"
    );
    // …but only once.
    let again = pagelamp(&home, &["folder", "add", courses.to_str().unwrap()]);
    ok(&again);
    assert!(!String::from_utf8_lossy(&again.stderr).contains("stores nothing remotely"));

    let synced = pagelamp(&home, &["sync"]);
    assert!(ok(&synced).contains("ok — 2 courses, 3 materials"));

    let list = json_out(&pagelamp(&home, &["--json", "courses"]));
    let codes: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["course"]["code"].as_str().unwrap())
        .collect();
    assert_eq!(codes, ["DEMO101", "DEMO202"]);
    assert!(ok(&pagelamp(&home, &["courses"])).contains("DEMO101 Intro to Demo Studies"));

    let hits = json_out(&pagelamp(&home, &["--json", "search", "calvin"]));
    assert_eq!(hits.as_array().unwrap().len(), 1);
    assert!(
        ok(&pagelamp(
            &home,
            &["search", "calvin", "--course", "DEMO101"]
        ))
        .contains("cycle.txt")
    );

    let status = json_out(&pagelamp(&home, &["--json", "status"]));
    assert_eq!(status["counts"]["courses"], 2);
    assert!(status["last_synced_at"].is_string());
    assert!(ok(&pagelamp(&home, &["status"])).contains("Courses: 2 (0 hidden)"));
    let sources = json_out(&pagelamp(&home, &["--json", "sources"]));
    assert_eq!(sources.as_array().unwrap().len(), 1);
}

#[test]
fn course_settings_commands() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let courses = temp.path().join("Courses");
    demo_courses(&courses);
    ok(&pagelamp(
        &home,
        &["folder", "add", courses.to_str().unwrap()],
    ));
    ok(&pagelamp(&home, &["sync"]));

    ok(&pagelamp(&home, &["course", "ai-access", "DEMO101", "off"]));
    ok(&pagelamp(
        &home,
        &[
            "course",
            "policy",
            "demo202",
            "prohibited",
            "--note",
            "syllabus §2",
        ],
    ));
    ok(&pagelamp(
        &home,
        &["course", "term", "DEMO101", "--start", "2026-09-08"],
    ));
    let list = json_out(&pagelamp(&home, &["--json", "courses"]));
    assert_eq!(list[0]["ai_materials"], "turned_off");
    assert_eq!(list[0]["course"]["term_source"], "user");
    assert_eq!(list[1]["ai_materials"], "withheld_by_policy");
    assert_eq!(list[1]["course"]["ai_policy_note"], "syllabus §2");

    ok(&pagelamp(&home, &["course", "term", "DEMO101", "--clear"]));
    ok(&pagelamp(&home, &["course", "hide", "DEMO202"]));
    let list = json_out(&pagelamp(&home, &["--json", "courses"]));
    // No course.toml and no --term-start: nothing synced to fall back to.
    assert_eq!(list[0]["course"]["term_source"], "none");
    assert_eq!(list[1]["course"]["hidden"], true);
    ok(&pagelamp(&home, &["course", "show", "DEMO202"]));

    let unknown = pagelamp(&home, &["course", "hide", "NOPE999"]);
    assert!(!unknown.status.success());
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("error:"));
    let no_dates = pagelamp(&home, &["course", "term", "DEMO101"]);
    assert!(!no_dates.status.success());
}

#[test]
fn mcp_config_and_schema() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let code = ok(&pagelamp(&home, &["mcp-config", "claude-code"]));
    assert!(
        code.starts_with("claude mcp add --scope user --env "),
        "{code}"
    );
    assert!(code.contains("--transport stdio pagelamp -- "), "{code}");
    assert!(code.contains(home.to_str().unwrap()));

    let desktop = ok(&pagelamp(&home, &["mcp-config", "claude-desktop"]));
    let desktop: Value = serde_json::from_str(&desktop).unwrap();
    assert_eq!(desktop["mcpServers"]["pagelamp"]["args"], json!(["mcp"]));
    assert_eq!(
        desktop["mcpServers"]["pagelamp"]["env"]["PAGELAMP_HOME"],
        json!(home.to_str().unwrap())
    );
    let all = json_out(&pagelamp(&home, &["--json", "mcp-config"]));
    assert_eq!(all.as_array().unwrap().len(), 4);

    let schema = json_out(&pagelamp(&home, &["schema"]));
    assert!(schema["$defs"]["CourseSummary"].is_object());
}

#[test]
fn ical_add_rejects_bad_urls_before_touching_anything() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let output = pagelamp_with_stdin(&home, &["ical", "add"], "not a url secret-token\n");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not a valid calendar feed URL"), "{stderr}");
    assert!(
        !stderr.contains("secret-token"),
        "the secret is never echoed: {stderr}"
    );
    assert!(
        json_out(&pagelamp(&home, &["--json", "sources"]))
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn mcp_speaks_json_rpc_on_stdout_only() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let courses = temp.path().join("Courses");
    demo_courses(&courses);
    ok(&pagelamp(
        &home,
        &["folder", "add", courses.to_str().unwrap()],
    ));
    ok(&pagelamp(&home, &["sync"]));

    let mut child = Command::new(env!("CARGO_BIN_EXE_pagelamp"))
        .arg("mcp")
        .env("PAGELAMP_HOME", &home)
        .env_remove("RUST_LOG")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut send = move |message: Value| {
        writeln!(stdin, "{message}").unwrap();
        stdin.flush().unwrap();
    };
    let mut receive = || {
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        serde_json::from_str::<Value>(&line).expect("every stdout line is JSON-RPC")
    };

    send(
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18", "capabilities": {},
        "clientInfo": {"name": "cli-test", "version": "0"}}}),
    );
    let init = receive();
    assert_eq!(init["result"]["serverInfo"]["name"], "pagelamp");
    send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    send(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": {"name": "search_materials", "arguments": {"query": "calvin"}}}));
    let result = receive();
    let text = result["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("<course_material") && text.contains("«calvin»"),
        "{text}"
    );

    drop(send); // closes the server's stdin → it exits
    let status = child.wait().unwrap();
    assert!(status.success());
    let mut stderr = String::new();
    std::io::Read::read_to_string(&mut child.stderr.take().unwrap(), &mut stderr).unwrap();
    assert!(stderr.is_empty(), "no log noise: {stderr}");
}

#[test]
fn canvas_add_fails_cleanly_and_stores_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let bad_url = pagelamp_with_stdin(
        &home,
        &["canvas", "add", "--base-url", "ftp://lms.example.edu"],
        "demo-not-a-real-token\n",
    );
    assert!(!bad_url.status.success());
    // A local port nobody listens on: the token check fails fast, without real network use.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let base_url = format!("http://127.0.0.1:{port}");
    let output = pagelamp_with_stdin(
        &home,
        &["canvas", "add", "--base-url", &base_url],
        "demo-not-a-real-token\n",
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{stderr}");
    assert!(!stderr.contains("panicked"), "{stderr}");
    assert!(
        stderr.contains("personal use only") && stderr.contains("Approved Integrations"),
        "{stderr}"
    );
    assert!(!stderr.contains("demo-not-a-real-token"));
    assert!(
        json_out(&pagelamp(&home, &["--json", "sources"]))
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn doctor_report_and_log_files() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let courses = temp.path().join("Courses");
    demo_courses(&courses);
    ok(&pagelamp(
        &home,
        &["folder", "add", courses.to_str().unwrap()],
    ));
    let sync = ok(&pagelamp(&home, &["sync"]));
    assert!(
        sync.contains("s\n") || sync.contains(" s"),
        "elapsed time shown: {sync}"
    );

    let doctor = ok(&pagelamp(&home, &["doctor"]));
    assert!(doctor.contains("Database: ok (schema"), "{doctor}");
    assert!(doctor.contains("Source folder: ok"), "{doctor}");
    assert!(doctor.contains("Courses: 2"), "{doctor}");
    let doctor_json = json_out(&pagelamp(&home, &["--json", "doctor"]));
    assert_eq!(doctor_json["courses"], 2);

    // The app log file exists, has the sync line, and no course names at info level.
    let logs = home.join("logs");
    let log_file = std::fs::read_dir(&logs)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.file_name().unwrap().to_string_lossy().starts_with("app-"))
        .expect("app log file");
    let log = std::fs::read_to_string(log_file).unwrap();
    assert!(log.contains("folder sync ok: 2 courses"), "{log}");
    assert!(log.contains(" pid="), "{log}");
    assert!(
        !log.contains("DEMO101") && !log.contains("photosynthesis"),
        "{log}"
    );

    // The report pseudonymises course names and goes to a file on request.
    let out = temp.path().join("report.md");
    let written = pagelamp(&home, &["report", "--out", out.to_str().unwrap()]);
    ok(&written);
    let report = std::fs::read_to_string(&out).unwrap();
    assert!(
        report.starts_with("# PageLamp diagnostic report"),
        "{report}"
    );
    assert!(report.contains("- folder: ok"), "{report}");
    assert!(
        !report.contains("DEMO101") && !report.contains("Intro to Demo Studies"),
        "{report}"
    );
    assert!(ok(&pagelamp(&home, &["report"])).contains("## Recent log"));
}

#[test]
fn canvas_urls_with_a_path_are_rejected_before_any_network_use() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    for url in [
        "https://lms.example.edu/courses/1",
        "https://lms.example.edu/?login=1",
    ] {
        let output = pagelamp_with_stdin(
            &home,
            &["canvas", "add", "--base-url", url],
            "demo-not-a-real-token\n",
        );
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("Enter just the address"), "{stderr}");
    }
}
