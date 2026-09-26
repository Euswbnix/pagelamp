//! Owns the one `studentos_app::App` instance and runs facade calls off the main thread.

use std::future::Future;
use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::Mutex;

use studentos_app::{App, AppError, AppErrorKind};

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
    /// UI thread. Panics become `internal` errors.
    pub async fn blocking<T, F>(&self, f: F) -> Result<T, AppError>
    where
        T: Send + 'static,
        F: FnOnce(&App) -> Result<T, AppError> + Send + 'static,
    {
        let app = self.app()?;
        tauri::async_runtime::spawn_blocking(move || f(&app))
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
        tauri::async_runtime::spawn(AssertUnwindSafe(fut))
            .await
            .map_err(|err| internal(format!("background task failed: {err}")))?
    }
}

/// Open the facade; a panic inside the core must not kill the window, so it becomes an error.
fn open_guarded(open: fn() -> Result<App, AppError>) -> Result<App, AppError> {
    panic::catch_unwind(open).unwrap_or_else(|payload| {
        Err(internal(format!(
            "StudentOS core failed to start: {}",
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

/// The `studentos` binary that AI apps launch for MCP: the file next to this app's executable,
/// without a target-triple suffix. Release bundles ship it there as a Tauri sidecar
/// (`scripts/build-sidecar.mjs`, macOS: `StudentOS.app/Contents/MacOS/studentos`); in development
/// it is the workspace build in `target/debug` (`cargo build -p studentos-cli`).
pub fn studentos_binary() -> PathBuf {
    let name = format!("studentos{}", std::env::consts::EXE_SUFFIX);
    std::env::current_exe()
        .map(|exe| exe.with_file_name(&name))
        .unwrap_or_else(|_| PathBuf::from(name))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use studentos_app::{App, AppError, AppErrorKind};

    use super::{Backend, studentos_binary};

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
                    let dir = std::env::temp_dir().join("studentos-backend-retry-test");
                    App::open_at(dir)
                }
            }
        }
        let backend = Backend::open_with(flaky_open);
        assert_eq!(
            backend.app().unwrap_err().message,
            "StudentOS core failed to start: core crashed"
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
    fn the_mcp_binary_sits_next_to_the_app() {
        let exe = std::env::current_exe().expect("current exe");
        let bin = studentos_binary();
        assert_eq!(bin.parent(), exe.parent());
        let name = bin
            .file_name()
            .expect("file name")
            .to_string_lossy()
            .into_owned();
        assert_eq!(name, format!("studentos{}", std::env::consts::EXE_SUFFIX));
    }
}
