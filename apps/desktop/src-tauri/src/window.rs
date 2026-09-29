//! The main window. `tauri.conf.json` describes it with `"create": false`, and it is built here
//! so its backdrop can depend on the Windows build (docs/design/macos-shell.md §8): Windows 11
//! 22H2 (build 22621) and later get a transparent window with Mica; Windows 10, Windows 11 21H2,
//! Linux and macOS get an opaque one, as before. Tauri ignores whether an effect took, so the
//! gate is ours. The page learns the result through an initialization script
//! (`window.__PAGELAMP_WINDOW__`, read by `src/lib/appearance.ts`), and paints its own paper
//! everywhere Mica isn't shown.

use tauri::{App, Manager, Runtime, WebviewWindowBuilder};

/// What the page is told the window was built with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backdrop {
    None,
    Mica,
}

impl Backdrop {
    fn name(self) -> &'static str {
        match self {
            Backdrop::None => "none",
            Backdrop::Mica => "mica",
        }
    }
}

/// Runs in the page before any of its scripts.
pub fn init_script(backdrop: Backdrop) -> String {
    format!(
        "window.__PAGELAMP_WINDOW__ = Object.freeze({{ backdrop: \"{}\" }});",
        backdrop.name()
    )
}

/// Builds the "main" window from its `tauri.conf.json` entry.
pub fn create_main<R: Runtime>(app: &App<R>) -> tauri::Result<()> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == "main")
        .cloned()
        .expect("tauri.conf.json describes the main window");
    let builder = WebviewWindowBuilder::from_config(app.handle(), &config)?;
    let (builder, backdrop) = with_backdrop(builder);
    tracing::info!(target: "pagelamp::window", backdrop = backdrop.name(), "main window");
    builder
        .initialization_script(init_script(backdrop))
        .build()?;
    Ok(())
}

#[cfg(windows)]
fn with_backdrop<'a, R: Runtime, M: Manager<R>>(
    builder: WebviewWindowBuilder<'a, R, M>,
) -> (WebviewWindowBuilder<'a, R, M>, Backdrop) {
    use tauri::window::{Effect, EffectsBuilder};
    // Mica needs Windows 11 22H2 (build 22621); older builds would get a see-through window.
    if windows_version::OsVersion::current().build >= 22_621 {
        let effects = EffectsBuilder::new().effect(Effect::Mica).build();
        (builder.transparent(true).effects(effects), Backdrop::Mica)
    } else {
        (builder, Backdrop::None)
    }
}

#[cfg(not(windows))]
fn with_backdrop<'a, R: Runtime, M: Manager<R>>(
    builder: WebviewWindowBuilder<'a, R, M>,
) -> (WebviewWindowBuilder<'a, R, M>, Backdrop) {
    (builder, Backdrop::None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_is_told_the_backdrop() {
        assert_eq!(
            init_script(Backdrop::Mica),
            r#"window.__PAGELAMP_WINDOW__ = Object.freeze({ backdrop: "mica" });"#
        );
        assert!(init_script(Backdrop::None).contains(r#"backdrop: "none""#));
    }

    /// `create_main` expects exactly one "main" entry, and Tauri must not create it too (a lost
    /// `"create": false` would make two "main" windows and fail at startup).
    #[test]
    fn the_shipped_config_leaves_the_main_window_to_us() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json");
        let windows = config["app"]["windows"].as_array().expect("app.windows");
        let main: Vec<_> = windows.iter().filter(|w| w["label"] == "main").collect();
        assert_eq!(main.len(), 1, "one main window");
        assert_eq!(main[0]["create"], false, "built in window.rs, not by Tauri");
    }
}
