//! Folder source: `<root>/<COURSE>/**` → courses, modules, materials (see the crate docs).
//!
//! Sync order per course: one short transaction writes the course, its modules and every
//! material's metadata and prunes what disappeared; then each supported file is indexed with
//! `ingest::index_file` OUTSIDE any transaction (text extraction is slow and must not hold
//! the write lock). Unchanged files are skipped cheaply by ingest.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

use chrono::{DateTime, NaiveDate, Utc};
use pagelamp_core::Store;
use pagelamp_core::ingest::{self, IndexOutcome};
use pagelamp_core::model::{CourseUpsert, MaterialKind, MaterialUpsert, Module, TextStatus};
use pagelamp_core::source::{ProgressFn, SourceError, SyncProgress};
use pagelamp_core::timeline::parse_week_hint;
use regex::Regex;
use walkdir::WalkDir;

use crate::FolderSyncReport;

/// Leading course-code token of a directory name, e.g. "CSC413H1 Neural Nets" → "CSC413H1",
/// "mat 137 notes" → "MAT137". Case-insensitive; the result is upper-cased without spaces.
static COURSE_CODE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^([A-Z]{2,4})\s?(\d{3}[A-Z0-9]*)(?:$|[\s_\-–—.,:(])").expect("valid regex")
});

/// Per-course metadata files (skipped as materials).
const COURSE_FILES: [&str; 2] = ["course.toml", "course.json"];

pub(crate) fn sync_folder(
    store: &Store,
    source_id: &str,
    root: &Path,
    default_term_start: Option<NaiveDate>,
    progress: ProgressFn<'_>,
) -> Result<FolderSyncReport, SourceError> {
    let not_found = || {
        SourceError::not_found(format!(
            "The course folder {} does not exist or cannot be read.",
            root.display()
        ))
    };
    if !root.is_dir() {
        return Err(not_found());
    }
    let entries = std::fs::read_dir(root).map_err(|_| not_found())?;
    let mut course_dirs: Vec<(String, PathBuf)> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|_| not_found())?;
        let name = entry.file_name().to_string_lossy().to_string();
        // `file_type` does not follow symlinks, so symlinked course dirs are skipped.
        if !is_hidden(&name) && entry.file_type().is_ok_and(|t| t.is_dir()) {
            course_dirs.push((name, entry.path()));
        }
    }
    course_dirs.sort();

    let mut report = FolderSyncReport::default();
    let mut keep_courses = Vec::new();
    for (dir_name, dir) in &course_dirs {
        let course_id = format!("{source_id}/course/{dir_name}");
        keep_courses.push(course_id.clone());
        let course = CourseDir {
            source_id,
            dir,
            dir_name,
            course_id: &course_id,
            default_term_start,
        };
        course.sync(store, progress, &mut report)?;
    }
    store.prune_courses(source_id, &keep_courses)?;
    report.courses = course_dirs.len();
    Ok(report)
}

/// One course directory being synced.
struct CourseDir<'a> {
    source_id: &'a str,
    dir: &'a Path,
    dir_name: &'a str,
    course_id: &'a str,
    default_term_start: Option<NaiveDate>,
}

/// A file found under a course directory.
struct FoundFile {
    path: PathBuf,
    material: MaterialUpsert,
    supported: bool,
}

impl CourseDir<'_> {
    fn sync(
        &self,
        store: &Store,
        progress: ProgressFn<'_>,
        report: &mut FolderSyncReport,
    ) -> Result<(), SourceError> {
        let warn = |report: &mut FolderSyncReport, message: String| {
            progress(SyncProgress::Warning(message.clone()));
            report.warnings.push(message);
        };
        let meta = match read_course_meta(self.dir) {
            Ok(meta) => meta,
            Err(message) => {
                warn(report, format!("{}: {message}", self.dir_name));
                CourseMeta::default()
            }
        };
        for message in &meta.warnings {
            warn(report, format!("{}: {message}", self.dir_name));
        }
        let code = meta.code.clone().or_else(|| course_code(self.dir_name));
        let label = code.clone().unwrap_or_else(|| self.dir_name.to_string());
        progress(SyncProgress::Step {
            message: format!("{label}: scanning files"),
            current: None,
            total: None,
        });

        let (modules, files, walk_complete) = self.scan(|message| warn(report, message));
        let upsert = CourseUpsert {
            id: self.course_id.to_string(),
            source_id: self.source_id.to_string(),
            external_id: self.dir_name.to_string(),
            code,
            name: meta.name.unwrap_or_else(|| self.dir_name.to_string()),
            term_start: meta.term_start.or(self.default_term_start),
            term_end: meta.term_end,
            url: url::Url::from_directory_path(self.dir)
                .ok()
                .map(String::from),
            syllabus_text: None,
        };
        store.in_transaction(|store| {
            store.upsert_course(&upsert)?;
            store.replace_modules(self.course_id, &modules)?;
            for file in &files {
                store.upsert_material(&file.material)?;
                if !file.supported {
                    // Listed so students still see it; never read or hashed.
                    store.set_text_state(&file.material.id, TextStatus::Unsupported, None, None)?;
                }
            }
            if walk_complete {
                let keep: Vec<String> = files.iter().map(|f| f.material.id.clone()).collect();
                store.prune_materials(self.course_id, &keep)?;
            }
            Ok(())
        })?;
        if !walk_complete {
            warn(
                report,
                format!(
                    "{label}: some folders could not be read; nothing was removed from this course"
                ),
            );
        }
        report.modules += modules.len();
        report.materials += files.len();

        let supported: Vec<&FoundFile> = files.iter().filter(|f| f.supported).collect();
        for (index, file) in supported.iter().enumerate() {
            progress(SyncProgress::Step {
                message: format!("{label}: indexing files"),
                current: Some(to_u32(index + 1)),
                total: Some(to_u32(supported.len())),
            });
            let mime = file.material.mime.as_deref();
            match ingest::index_file(store, &file.material.id, &file.path, mime) {
                Ok(IndexOutcome::Unchanged) => report.files_unchanged += 1,
                Ok(IndexOutcome::Indexed { .. }) => report.files_indexed += 1,
                Ok(IndexOutcome::Empty) => {
                    report.files_indexed += 1;
                    warn(
                        report,
                        format!("{}: no extractable text (scanned?)", file.material.title),
                    );
                }
                Ok(IndexOutcome::Unsupported) => {}
                Ok(IndexOutcome::Failed(message)) => {
                    warn(report, format!("{}: {message}", file.material.title));
                }
                // The file vanished or became unreadable during the sync: not fatal.
                Err(pagelamp_core::Error::Io(err)) => {
                    warn(
                        report,
                        format!("{}: could not read the file ({err})", file.material.title),
                    );
                }
                Err(other) => return Err(other.into()),
            }
        }
        Ok(())
    }

    /// Walk the course directory: modules (first-level sub-directories) and files.
    /// `walk_complete` is false when any part could not be read (then nothing is pruned).
    fn scan(&self, mut warn: impl FnMut(String)) -> (Vec<Module>, Vec<FoundFile>, bool) {
        let mut modules: BTreeMap<String, Module> = BTreeMap::new();
        let mut files = Vec::new();
        let mut walk_complete = true;
        let walker = WalkDir::new(self.dir)
            .follow_links(false)
            .min_depth(1)
            .sort_by_file_name()
            .into_iter()
            .filter_entry(|entry| !is_hidden(&entry.file_name().to_string_lossy()));
        for entry in walker {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    walk_complete = false;
                    let place = err
                        .path()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default();
                    warn(format!("could not read {place}"));
                    continue;
                }
            };
            let file_type = entry.file_type();
            if file_type.is_symlink() {
                continue;
            }
            let Ok(relative) = entry.path().strip_prefix(self.dir) else {
                continue;
            };
            let parts: Vec<String> = relative
                .components()
                .filter_map(|c| match c {
                    Component::Normal(part) => Some(part.to_string_lossy().to_string()),
                    _ => None,
                })
                .collect();
            if file_type.is_dir() {
                if let [module_dir] = parts.as_slice() {
                    let id = self.module_id(module_dir);
                    modules.insert(
                        id.clone(),
                        Module {
                            id,
                            course_id: self.course_id.to_string(),
                            name: module_dir.clone(),
                            position: None,
                            unlock_at: None,
                            week_hint: parse_week_hint(module_dir),
                        },
                    );
                }
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            if let [only] = parts.as_slice()
                && COURSE_FILES.contains(&only.to_ascii_lowercase().as_str())
            {
                continue;
            }
            files.push(self.found_file(entry.path(), &parts, entry.metadata().ok()));
        }
        let mut modules: Vec<Module> = modules.into_values().collect();
        for (position, module) in modules.iter_mut().enumerate() {
            module.position = Some(position as i64);
        }
        (modules, files, walk_complete)
    }

    fn found_file(
        &self,
        path: &Path,
        parts: &[String],
        metadata: Option<std::fs::Metadata>,
    ) -> FoundFile {
        let file_name = parts.last().cloned().unwrap_or_default();
        let stem = Path::new(&file_name)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        // Week: the file's own name, else the nearest enclosing folder that names a week.
        let week_hint = parse_week_hint(&stem).or_else(|| {
            parts[..parts.len().saturating_sub(1)]
                .iter()
                .rev()
                .find_map(|dir| parse_week_hint(dir))
        });
        let module_id = (parts.len() > 1).then(|| self.module_id(&parts[0]));
        let mime = mime_guess::from_path(path).first_raw().map(str::to_string);
        let supported = pagelamp_extract::is_supported(path, mime.as_deref());
        let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        let published_at = metadata
            .and_then(|m| m.modified().ok())
            .map(DateTime::<Utc>::from);
        FoundFile {
            path: path.to_path_buf(),
            supported,
            material: MaterialUpsert {
                id: format!(
                    "{}/file/{}/{}",
                    self.source_id,
                    self.dir_name,
                    parts.join("/")
                ),
                course_id: self.course_id.to_string(),
                module_id,
                kind: MaterialKind::File,
                title: file_name,
                url: url::Url::from_file_path(&absolute).ok().map(String::from),
                local_path: Some(absolute.to_string_lossy().to_string()),
                mime,
                published_at,
                week_hint,
            },
        }
    }

    fn module_id(&self, module_dir: &str) -> String {
        format!("{}/module/{}/{module_dir}", self.source_id, self.dir_name)
    }
}

/// Values from `course.toml` / `course.json`.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct CourseMeta {
    pub code: Option<String>,
    pub name: Option<String>,
    pub term_start: Option<NaiveDate>,
    pub term_end: Option<NaiveDate>,
    /// One message per setting we don't know (a typo would otherwise be silently ignored).
    pub warnings: Vec<String>,
}

/// The settings `course.toml` / `course.json` may contain.
const COURSE_KEYS: [&str; 4] = ["code", "name", "term_start", "term_end"];

/// Warnings for the keys of a course file that aren't `COURSE_KEYS`.
fn unknown_keys<'a>(file: &str, keys: impl Iterator<Item = &'a String>) -> Vec<String> {
    keys.filter(|key| !COURSE_KEYS.contains(&key.as_str()))
        .map(|key| {
            let key: String = key.chars().take(40).collect();
            format!(
                "{file}: unknown setting {key:?} ignored (use {})",
                COURSE_KEYS.join(", ")
            )
        })
        .collect()
}

/// Read `course.toml` (preferred) or `course.json` in `dir`. Missing files → defaults; a
/// malformed file → `Err(message)` (the caller warns and ignores it).
pub(crate) fn read_course_meta(dir: &Path) -> Result<CourseMeta, String> {
    let toml_path = dir.join("course.toml");
    if toml_path.is_file() {
        let text = std::fs::read_to_string(&toml_path)
            .map_err(|err| format!("course.toml could not be read ({err})"))?;
        let table: toml::Table = toml::from_str(&text)
            .map_err(|err| format!("course.toml is not valid TOML ({err})"))?;
        let mut meta = meta_from(|key| match table.get(key)? {
            toml::Value::String(s) => Some(s.clone()),
            // Bare TOML dates: term_start = 2026-09-08
            toml::Value::Datetime(dt) => dt.date.map(|d| d.to_string()),
            _ => None,
        })
        .map_err(|err| format!("course.toml: {err}"))?;
        meta.warnings = unknown_keys("course.toml", table.keys());
        return Ok(meta);
    }
    let json_path = dir.join("course.json");
    if json_path.is_file() {
        let text = std::fs::read_to_string(&json_path)
            .map_err(|err| format!("course.json could not be read ({err})"))?;
        let value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|err| format!("course.json is not valid JSON ({err})"))?;
        let mut meta = meta_from(|key| value.get(key)?.as_str().map(str::to_string))
            .map_err(|err| format!("course.json: {err}"))?;
        if let Some(object) = value.as_object() {
            meta.warnings = unknown_keys("course.json", object.keys());
        }
        return Ok(meta);
    }
    Ok(CourseMeta::default())
}

fn meta_from(get: impl Fn(&str) -> Option<String>) -> Result<CourseMeta, String> {
    let text = |key: &str| {
        get(key)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    let date = |key: &str| -> Result<Option<NaiveDate>, String> {
        text(key)
            .map(|v| {
                NaiveDate::parse_from_str(&v, "%Y-%m-%d")
                    .map_err(|_| format!("{key} must be a date like 2026-09-08, not '{v}'"))
            })
            .transpose()
    };
    Ok(CourseMeta {
        code: text("code"),
        name: text("name"),
        term_start: date("term_start")?,
        term_end: date("term_end")?,
        warnings: Vec::new(),
    })
}

/// Course code from a directory name (see `COURSE_CODE`).
pub(crate) fn course_code(dir_name: &str) -> Option<String> {
    let captures = COURSE_CODE.captures(dir_name.trim())?;
    Some(format!("{}{}", &captures[1], &captures[2]).to_ascii_uppercase())
}

fn is_hidden(name: &str) -> bool {
    name.starts_with('.')
}

fn to_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn course_codes_from_directory_names() {
        assert_eq!(
            course_code("DEMO101H1 Intro to Demo Studies").as_deref(),
            Some("DEMO101H1")
        );
        assert_eq!(course_code("demo 101 notes").as_deref(), Some("DEMO101"));
        assert_eq!(course_code("ABC123"), Some("ABC123".into()));
        assert_eq!(
            course_code("MAT137Y1-lectures").as_deref(),
            Some("MAT137Y1")
        );
        assert_eq!(course_code("Intro to Demo Studies"), None);
        assert_eq!(course_code("DEMOS101"), None, "5 letters is not a code");
        assert_eq!(course_code("AB12 notes"), None);
    }

    #[test]
    fn course_meta_from_toml_and_json() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_course_meta(dir.path()).unwrap(), CourseMeta::default());

        std::fs::write(
            dir.path().join("course.json"),
            r#"{"code": "DEMO202", "name": "Advanced Demo Studies", "term_start": "2026-09-08"}"#,
        )
        .unwrap();
        let meta = read_course_meta(dir.path()).unwrap();
        assert_eq!(meta.code.as_deref(), Some("DEMO202"));
        assert_eq!(meta.term_start, NaiveDate::from_ymd_opt(2026, 9, 8));

        // course.toml wins; bare TOML dates and strings both work.
        std::fs::write(
            dir.path().join("course.toml"),
            "code = \"DEMO303\"\nterm_start = 2026-09-10\nterm_end = \"2026-12-20\"\n",
        )
        .unwrap();
        let meta = read_course_meta(dir.path()).unwrap();
        assert_eq!(meta.code.as_deref(), Some("DEMO303"));
        assert_eq!(meta.name, None);
        assert!(meta.warnings.is_empty(), "{:?}", meta.warnings);
        assert_eq!(meta.term_start, NaiveDate::from_ymd_opt(2026, 9, 10));
        assert_eq!(meta.term_end, NaiveDate::from_ymd_opt(2026, 12, 20));

        // A misspelt setting is reported, not silently ignored.
        std::fs::write(
            dir.path().join("course.toml"),
            "code = \"DEMO303\"\nterm_sart = 2026-09-10\n",
        )
        .unwrap();
        let meta = read_course_meta(dir.path()).unwrap();
        assert_eq!(meta.code.as_deref(), Some("DEMO303"));
        assert_eq!(meta.warnings.len(), 1);
        assert!(
            meta.warnings[0].contains("unknown setting \"term_sart\""),
            "{:?}",
            meta.warnings
        );

        std::fs::write(
            dir.path().join("course.toml"),
            "term_start = \"next monday\"\n",
        )
        .unwrap();
        assert!(
            read_course_meta(dir.path())
                .unwrap_err()
                .contains("term_start")
        );
        std::fs::write(dir.path().join("course.toml"), "code = [unclosed").unwrap();
        assert!(
            read_course_meta(dir.path())
                .unwrap_err()
                .contains("not valid TOML")
        );
    }
}
