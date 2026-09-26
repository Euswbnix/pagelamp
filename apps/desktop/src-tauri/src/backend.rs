//! Owns the one `studentos_app::App` instance and runs facade calls off the main thread.

use std::future::Future;
use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;

use studentos_app::{App, AppError, AppErrorKind};

/// Managed Tauri state. If the core fails to open (e.g. the database was created by a newer
/// StudentOS), the window still starts and every command returns that error, so the UI can
/// explain it instead of the app vanishing.
pub struct Backend(Result<App, AppError>);

impl Backend {
    pub fn open() -> Self {
        // Defensive: a panic inside the core must not kill the window before it opens.
        let opened = panic::catch_unwind(App::open).unwrap_or_else(|payload| {
            Err(internal(format!(
                "StudentOS core failed to start: {}",
                panic_message(&payload)
            )))
        });
        Backend(opened)
    }

    /// A handle to the facade (cheap: `App` only holds the data-dir path).
    pub fn app(&self) -> Result<App, AppError> {
        self.0.clone()
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

/// The `studentos` binary that AI apps should launch for MCP.
///
/// - Debug builds (`pnpm tauri dev`): the workspace build at `<repo>/target/debug/studentos`
///   (run `cargo build -p studentos-cli` once).
/// - Release builds: next to the app executable, where a Tauri `externalBin` sidecar is placed
///   (bundling the sidecar is on the backlog).
pub fn studentos_binary() -> PathBuf {
    if cfg!(debug_assertions) {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let bin = repo.join("target/debug").join(exe_name());
        return bin.canonicalize().unwrap_or(bin);
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(exe_name())))
        .unwrap_or_else(|| PathBuf::from(exe_name()))
}

fn exe_name() -> &'static str {
    if cfg!(windows) {
        "studentos.exe"
    } else {
        "studentos"
    }
}
