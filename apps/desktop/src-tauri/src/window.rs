//! The main window. `tauri.conf.json` describes it with `"create": false`, and it is built here
//! so its backdrop can depend on the Windows build (docs/design/macos-shell.md §8): Windows 11
//! 22H2 (build 22621) and later get a transparent window with Mica; Windows 10, Windows 11 21H2,
//! Linux and macOS get an opaque one, as before. Tauri ignores whether an effect took, so the
//! gate is ours. The page learns the result through an initialization script
//! (`window.__PAGELAMP_WINDOW__`, read by `src/lib/appearance.ts`), and paints its own paper
//! everywhere Mica isn't shown.
//!
//! The window starts hidden (`"visible": false`) and is shown once the page has loaded: by then
//! main.tsx has applied the student's theme, transparency and contrast, so the first frame is
//! never the system theme or a see-through Mica the student turned off.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use tauri::webview::PageLoadEvent;
use tauri::{App, Manager, Runtime, Url, WebviewWindowBuilder};

use crate::background::{self, Background};

/// How often this process has loaded the page. The first load is the launch. A later one is a
/// reload, and a reload needs nobody at the app: when the system ends the webview's content
/// process (memory pressure, a long sleep, a crash), Tauri loads the page again by itself. The
/// page asks (`first_page_load`) so that it never takes a reload for the student opening
/// PageLamp, which would let an automatic sync count as attended.
///
/// The loads are counted where they happen (`on_page_load`), not when the page asks: a first
/// page that never got to ask (it stopped at an error screen) must not leave the answer "first"
/// to a later reload. And "first" is said once per process at most, to the first ask: the page
/// asks once and keeps the answer (src/api/tauri.ts), so whoever asks next is another page.
#[derive(Debug, Default)]
pub struct PageLoads {
    started: AtomicU32,
    asked: AtomicBool,
    shown: AtomicBool,
}

impl PageLoads {
    /// Takes one page-load event: counts a load that starts, and says (true) when the window
    /// is to be shown. That is once, when its first page has loaded. Showing it again after a
    /// reload would bring it forward by itself (on macOS it takes the focus, out of the Dock if
    /// it was minimised), and the page takes a window gaining focus for the student coming back.
    pub fn event(&self, event: PageLoadEvent, url: &Url) -> bool {
        match event {
            // Not the empty document a webview may start from.
            PageLoadEvent::Started if url.scheme() == "about" => false,
            PageLoadEvent::Started => {
                self.started.fetch_add(1, Ordering::SeqCst);
                false
            }
            PageLoadEvent::Finished => !self.shown.swap(true, Ordering::SeqCst),
        }
    }

    /// Whether the page asking is the launch. True once per process at most: to the first ask,
    /// and only while no load after the first has started.
    pub fn first(&self) -> bool {
        // The ask is spent whatever the answer.
        let asked_before = self.asked.swap(true, Ordering::SeqCst);
        !asked_before && self.started.load(Ordering::SeqCst) <= 1
    }
}

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

/// Runs in the page before any of its scripts. `hidden`: the window was started without being
/// shown to the student (a login start), so the page must not take its launch for the student
/// opening PageLamp (an automatic sync at such a start is never "attended").
pub fn init_script(backdrop: Backdrop, hidden: bool) -> String {
    format!(
        "window.__PAGELAMP_WINDOW__ = Object.freeze({{ backdrop: \"{}\", hidden: {hidden} }});",
        backdrop.name()
    )
}

/// Whether the main window starts without being shown: a login launch (`--hidden`) waiting in
/// the tray.
fn starts_hidden<R: Runtime>(app: &App<R>) -> bool {
    app.try_state::<Background>()
        .is_some_and(|state| !state.may_show())
}

/// Takes one page-load event and says whether to show the window now. Every event is counted
/// (`PageLoads::event`), and the one show is spent at the first page also when the window may
/// not be shown (a login launch waiting in the tray): a later reload must not bring the window
/// up by itself once the student has opened it. A window that started hidden is shown by the
/// student only.
fn shows_now(loads: &PageLoads, background: &Background, event: PageLoadEvent, url: &Url) -> bool {
    let show = loads.event(event, url);
    show && background.may_show()
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
    // Normally there already, with the commands (lib.rs `with_commands`); a window built without
    // them counts its loads all the same.
    app.manage(PageLoads::default());
    let builder = WebviewWindowBuilder::from_config(app.handle(), &config)?;
    let (builder, backdrop, os_build) = with_backdrop(builder);
    // Read before the window exists: the page's script carries it.
    let hidden = starts_hidden(app);
    builder
        .initialization_script(init_script(backdrop, hidden))
        // Shown once the first page has loaded. Windows and Linux report a failed load as
        // finished too, so the window doesn't stay hidden there (macOS reports no failed load).
        // A reload finds the window as the student left it. After a login launch (`--hidden`)
        // no page load shows it: the student opens it from the tray (`shows_now`).
        .on_page_load(|window, payload| {
            if shows_now(
                &window.state::<PageLoads>(),
                &window.state::<Background>(),
                payload.event(),
                payload.url(),
            ) && let Err(error) = window.show()
            {
                tracing::warn!(target: "pagelamp::window", %error, "show main window");
            }
        })
        .build()?;
    if let Some(window) = app.get_webview_window("main") {
        background::watch_close(&window);
    }
    // The Windows build tells a tester's "no Mica" apart: gated (Windows 10, 21H2) or DWM's own
    // solid fallback (Battery Saver, transparency off, an inactive window).
    tracing::info!(
        target: "pagelamp::window",
        backdrop = backdrop.name(),
        os_build = ?os_build,
        hidden,
        "main window"
    );
    Ok(())
}

/// The builder with its backdrop, which backdrop that is, and the Windows build it depends on.
type WithBackdrop<'a, R, M> = (WebviewWindowBuilder<'a, R, M>, Backdrop, Option<u32>);

#[cfg(windows)]
fn with_backdrop<'a, R: Runtime, M: Manager<R>>(
    builder: WebviewWindowBuilder<'a, R, M>,
) -> WithBackdrop<'a, R, M> {
    use tauri::window::{Effect, EffectsBuilder};
    let build = windows_version::OsVersion::current().build;
    // Mica needs Windows 11 22H2 (build 22621); older builds would get a see-through window.
    if build >= 22_621 {
        let effects = EffectsBuilder::new().effect(Effect::Mica).build();
        (
            builder.transparent(true).effects(effects),
            Backdrop::Mica,
            Some(build),
        )
    } else {
        (builder, Backdrop::None, Some(build))
    }
}

#[cfg(not(windows))]
fn with_backdrop<'a, R: Runtime, M: Manager<R>>(
    builder: WebviewWindowBuilder<'a, R, M>,
) -> WithBackdrop<'a, R, M> {
    (builder, Backdrop::None, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> Url {
        "tauri://localhost".parse().expect("url")
    }

    #[test]
    fn only_the_first_page_load_is_the_launch() {
        let loads = PageLoads::default();
        loads.event(PageLoadEvent::Started, &page());
        assert!(loads.first());
        // Said once: the page keeps the answer, and whoever asks next is another page.
        assert!(!loads.first());

        // After a reload, also when the first page never asked.
        let never_asked = PageLoads::default();
        never_asked.event(PageLoadEvent::Started, &page());
        never_asked.event(PageLoadEvent::Finished, &page());
        never_asked.event(PageLoadEvent::Started, &page());
        assert!(!never_asked.first());

        // Should a load go uncounted, the second page to ask is a reload all the same.
        let uncounted = PageLoads::default();
        assert!(uncounted.first());
        assert!(!uncounted.first());
    }

    #[test]
    fn the_empty_start_document_is_no_page_load() {
        let loads = PageLoads::default();
        let blank: Url = "about:blank".parse().expect("url");
        loads.event(PageLoadEvent::Started, &blank);
        loads.event(PageLoadEvent::Started, &page());
        assert!(loads.first());
    }

    #[test]
    fn the_window_is_shown_for_its_first_page_only() {
        let loads = PageLoads::default();
        assert!(!loads.event(PageLoadEvent::Started, &page()));
        assert!(loads.event(PageLoadEvent::Finished, &page()));
        // A reload must not bring the window forward by itself.
        assert!(!loads.event(PageLoadEvent::Started, &page()));
        assert!(!loads.event(PageLoadEvent::Finished, &page()));
        assert!(!loads.event(PageLoadEvent::Finished, &page()));
    }

    #[test]
    fn the_page_is_told_the_backdrop() {
        assert_eq!(
            init_script(Backdrop::Mica, false),
            r#"window.__PAGELAMP_WINDOW__ = Object.freeze({ backdrop: "mica", hidden: false });"#
        );
        assert!(init_script(Backdrop::None, false).contains(r#"backdrop: "none""#));
        assert!(init_script(Backdrop::None, true).contains("hidden: true"));
    }

    /// After a login launch no page load shows the window: not the first one (it waits in the
    /// tray), and not a reload later, when the student has opened it and it may be shown. The
    /// one show was spent at the first page.
    #[test]
    fn a_window_that_started_hidden_is_never_shown_by_a_page_load() {
        let loads = PageLoads::default();
        let in_tray = Background::new(true);
        assert!(!shows_now(
            &loads,
            &in_tray,
            PageLoadEvent::Started,
            &page()
        ));
        assert!(!shows_now(
            &loads,
            &in_tray,
            PageLoadEvent::Finished,
            &page()
        ));
        // The student opened it from the tray; then the page loads again.
        let opened = Background::new(false);
        assert!(!shows_now(&loads, &opened, PageLoadEvent::Started, &page()));
        assert!(!shows_now(
            &loads,
            &opened,
            PageLoadEvent::Finished,
            &page()
        ));
        // The load was counted all the same: this page is not the launch.
        assert!(!loads.first());

        // A start the student made: shown once, at the first page.
        let loads = PageLoads::default();
        assert!(!shows_now(&loads, &opened, PageLoadEvent::Started, &page()));
        assert!(shows_now(&loads, &opened, PageLoadEvent::Finished, &page()));
        assert!(!shows_now(
            &loads,
            &opened,
            PageLoadEvent::Finished,
            &page()
        ));
    }

    /// A login start (`--hidden`) is told to the page: such a launch must never count as the
    /// student opening PageLamp. A start the student made is told as one.
    #[test]
    fn the_page_is_told_a_login_start() {
        for hidden in [true, false] {
            let app = tauri::test::mock_builder()
                .manage(Background::new(hidden))
                .build(tauri::test::mock_context(tauri::test::noop_assets()))
                .expect("mock app");
            assert_eq!(starts_hidden(&app), hidden);
        }
    }

    fn shipped_config() -> serde_json::Value {
        serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json")
    }

    /// `create_main` expects exactly one "main" entry, and Tauri must not create it too (a lost
    /// `"create": false` would make two "main" windows and fail at startup). Transparency and
    /// effects are window.rs's decision: in the config they would bypass the Windows-build gate
    /// (a see-through window on Windows 10 and Linux). It starts hidden; the page shows it.
    #[test]
    fn the_shipped_config_leaves_the_main_window_to_us() {
        let config = shipped_config();
        let windows = config["app"]["windows"].as_array().expect("app.windows");
        let main: Vec<_> = windows.iter().filter(|w| w["label"] == "main").collect();
        assert_eq!(main.len(), 1, "one main window");
        assert_eq!(main[0]["create"], false, "built in window.rs, not by Tauri");
        assert_eq!(main[0]["visible"], false, "shown once the page has loaded");
        for key in ["transparent", "windowEffects"] {
            assert!(main[0].get(key).is_none(), "{key} is decided in window.rs");
        }
    }

    /// A JSON merge patch replaces arrays whole: `app.windows` in the rehearsal overlay would
    /// drop `"create": false` (and the rest) from the main window.
    #[test]
    fn the_rehearsal_overlay_keeps_the_windows() {
        let overlay: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.rehearsal.conf.json"))
                .expect("tauri.rehearsal.conf.json");
        assert!(overlay.pointer("/app/windows").is_none());
    }

    /// Runs `create_main` itself (mock runtime): the config entry, the builder and the
    /// initialization script together make exactly one "main" window.
    #[test]
    fn create_main_builds_the_one_main_window() {
        let windows: Vec<tauri::utils::config::WindowConfig> =
            serde_json::from_value(shipped_config()["app"]["windows"].clone())
                .expect("window configs");
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().app.windows = windows;
        let app = tauri::test::mock_builder()
            .build(context)
            .expect("mock app");
        create_main(&app).expect("create_main");
        assert_eq!(app.webview_windows().len(), 1);
        assert!(app.get_webview_window("main").is_some());
    }
}
