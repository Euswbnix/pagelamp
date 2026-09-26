//! Owns the one `pagelamp_app::App` instance and runs facade calls off the main thread.

use std::future::Future;
use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::Mutex;

use pagelamp_app::diagnostics::{expect_panics, expect_panics_in};
use pagelamp_app::{App, AppError, AppErrorKind};
use pagelamp_core::brand;

/// Managed Tauri state. If the core fails to open (e.g. the data folder isn't writable yet),
/// the window still starts and every command returns that error, so the UI can explain it. A
/// failed open is retried on the next call, so the start screen's "Try again" can recover
/// without restarting the app; once opened, the facade is kept.
pub struct Backend {
    state: Mutex<Result<App, AppError>>,
    open: fn() -> Result<App, AppError>,
}

impl Backend {
    pub fn open() -> Self {
        Backend::open_with(App::open)
    }

    fn open_with(open: fn() -> Result<App, AppError>) -> Self {
        Backend {
            state: Mutex::new(open_guarded(open)),
            open,
        }
    }

    /// Wrap an already opened facade (tests use a temp data dir and in-memory secrets).
    pub fn from_app(app: App) -> Self {
        Backend {
            state: Mutex::new(Ok(app)),
            open: App::open,
        }
    }

    /// A handle to the facade (cheap: `App` only holds the data-dir path). Retries a failed open.
    pub fn app(&self) -> Result<App, AppError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.is_err() {
            *state = open_guarded(self.open);
        }
        state.clone()
    }

    /// Run a synchronous facade call (they open SQLite) on the blocking pool, never on the
    /// UI thread. Panics become `internal` errors (logged, but not recorded as a crash: the app
    /// keeps running).
    pub async fn blocking<T, F>(&self, f: F) -> Result<T, AppError>
    where
        T: Send + 'static,
        F: FnOnce(&App) -> Result<T, AppError> + Send + 'static,
    {
        let app = self.app()?;
        tauri::async_runtime::spawn_blocking(move || expect_panics(|| f(&app)))
            .await
            .map_err(|err| internal(format!("background task failed: {err}")))?
    }

    /// Diagnostics must work even when the facade can't open: a locked or damaged database is
    /// exactly when a tester needs a report. With an open facade they use its data dir,
    /// otherwise the default one (`pagelamp_app::diagnostics`, resolved like `App::open`).
    pub async fn diagnostics<T, F, G>(&self, with_app: F, without_app: G) -> Result<T, AppError>
    where
        T: Send + 'static,
        F: FnOnce(&App) -> Result<T, AppError> + Send + 'static,
        G: FnOnce() -> Result<T, AppError> + Send + 'static,
    {
        let app = self.app().ok();
        tauri::async_runtime::spawn_blocking(move || {
            expect_panics(|| match &app {
                Some(app) => with_app(app),
                None => without_app(),
            })
        })
        .await
        .map_err(|err| internal(format!("background task failed: {err}")))?
    }

    /// Run an async facade call (network: token validation, sync) as its own task. Panics
    /// become `internal` errors instead of a promise that never settles.
    pub async fn spawn<T, F, Fut>(&self, f: F) -> Result<T, AppError>
    where
        T: Send + 'static,
        F: FnOnce(App) -> Fut,
        Fut: Future<Output = Result<T, AppError>> + Send + 'static,
    {
        let fut = f(self.app()?);
        tauri::async_runtime::spawn(expect_panics_in(AssertUnwindSafe(fut)))
            .await
            .map_err(|err| internal(format!("background task failed: {err}")))?
    }
}

/// Open the facade; a panic inside the core must not kill the window, so it becomes an error.
fn open_guarded(open: fn() -> Result<App, AppError>) -> Result<App, AppError> {
    expect_panics(|| panic::catch_unwind(open)).unwrap_or_else(|payload| {
        Err(internal(format!(
            "{} core failed to start: {}",
            brand::PRODUCT_NAME,
            panic_message(&*payload)
        )))
    })
}

pub fn internal(message: impl Into<String>) -> AppError {
    AppError::new(AppErrorKind::Internal, message)
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_string())
}

/// The `pagelamp` binary that AI apps launch for MCP: the file next to this app's executable,
/// without a target-triple suffix. Release bundles ship it there as a Tauri sidecar
/// (`scripts/build-sidecar.mjs`, macOS: `PageLamp.app/Contents/MacOS/pagelamp`); in development
/// it is the workspace build in `target/debug` (`cargo build -p pagelamp-cli`).
pub fn pagelamp_binary() -> PathBuf {
    let name = format!("{}{}", brand::CLI_NAME, std::env::consts::EXE_SUFFIX);
    std::env::current_exe()
        .map(|exe| exe.with_file_name(&name))
        .unwrap_or_else(|_| PathBuf::from(name))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use pagelamp_app::{App, AppError, AppErrorKind};

    use super::{Backend, brand, pagelamp_binary};

    #[test]
    fn a_failed_open_is_retried_and_success_is_kept() {
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        fn flaky_open() -> Result<App, AppError> {
            match CALLS.fetch_add(1, Ordering::SeqCst) {
                0 => Err(AppError::new(
                    AppErrorKind::Internal,
                    "data folder not ready",
                )),
                1 => panic!("core crashed"),
                _ => {
                    let dir = std::env::temp_dir().join("pagelamp-backend-retry-test");
                    App::open_at(dir)
                }
            }
        }
        let backend = Backend::open_with(flaky_open);
        assert_eq!(
            backend.app().unwrap_err().message,
            format!("{} core failed to start: core crashed", brand::PRODUCT_NAME)
        );
        assert!(backend.app().is_ok(), "third attempt opens");
        assert!(backend.app().is_ok());
        assert_eq!(
            CALLS.load(Ordering::SeqCst),
            3,
            "an opened facade is kept, not reopened"
        );
    }

    #[test]
    fn diagnostics_use_the_default_data_dir_only_when_the_core_cannot_open() {
        fn locked_open() -> Result<App, AppError> {
            Err(AppError::new(AppErrorKind::Internal, "database is locked"))
        }
        let backend = Backend::open_with(locked_open);
        let used = tauri::async_runtime::block_on(
            backend.diagnostics(|_| Ok("facade"), || Ok("default data dir")),
        );
        assert_eq!(used.unwrap(), "default data dir");

        // With an open facade, its own data dir: tests never touch the real default one.
        let dir = tempfile::tempdir().expect("temp dir");
        let app = App::open_at(dir.path().to_path_buf()).expect("open App in a temp dir");
        let backend = Backend::from_app(app);
        let used = tauri::async_runtime::block_on(backend.diagnostics(
            |app| Ok(app.data_dir().to_path_buf()),
            || panic!("the open facade's data dir must be used"),
        ));
        assert_eq!(used.unwrap(), dir.path());
    }

    #[test]
    fn the_mcp_binary_sits_next_to_the_app() {
        let exe = std::env::current_exe().expect("current exe");
        let bin = pagelamp_binary();
        assert_eq!(bin.parent(), exe.parent());
        let name = bin
            .file_name()
            .expect("file name")
            .to_string_lossy()
            .into_owned();
        assert_eq!(
            name,
            format!("{}{}", brand::CLI_NAME, std::env::consts::EXE_SUFFIX)
        );
    }
}
