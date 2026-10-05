//! Reminders while PageLamp runs (v0.3 M3; design §5.3). A ticker asks the page to deliver due
//! reminders every 15 minutes and right after the computer wakes. The page fetches them
//! (`due_reminders`), words them in the student's language (course code and titles only, never
//! material text) and hands them to `show_reminders`, which shows them and marks them shown.
//! What is due, the catch-up window and the dedupe are the facade's; the page's own check at
//! launch is the catch-up.

use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};
use pagelamp_app::{App, AppError, Reminder, ReminderSettings};
use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;

use crate::backend::Backend;
use crate::background::{self, Background, BackgroundStatus};

type CmdResult<T> = Result<T, AppError>;

/// The event the page answers with a delivery.
pub const CHECK_EVENT: &str = "reminders:check";
/// The event at which the page may ask again what is due by itself (`startup_tasks`: an
/// automatic sync among it). The page asks on its own hourly timer; a window that is out of
/// sight may not run that timer on time. The event carries nothing and starts nothing. An event
/// from the shell can make the page ask the facade; what a run is started as is the page's to
/// decide, and only the student's input makes a run attended.
pub const STARTUP_CHECK_EVENT: &str = "startup:check";
/// How often the ticker looks at the clock.
const TICK: Duration = Duration::from_secs(60);
/// How often it asks for a delivery.
const CHECK_EVERY: TimeDelta = TimeDelta::minutes(15);
/// A tick this much later than expected means the computer slept (or the clock was changed).
const WOKE_AFTER: TimeDelta = TimeDelta::minutes(2);
/// How long after waking the page is first asked to look at what is due: the network is often
/// not back at once, and a sync tried without it fails and counts as its try.
const STARTUP_AFTER_WAKING: TimeDelta = TimeDelta::minutes(2);

/// What a tick asks the page for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Asks {
    /// A delivery of the reminders that are due.
    pub reminders: bool,
    /// Another look at what is due by itself: every 15 minutes too, on a clock of its own. Not
    /// at the tick that follows a sleep, nor in the two minutes after it (`STARTUP_AFTER_WAKING`);
    /// what came due meanwhile is asked for then, not dropped.
    pub startup: bool,
}

/// When to ask for a delivery, from the wall clock alone: the monotonic clock stops while a Mac
/// or a Linux laptop sleeps, and a reminder that came due meanwhile should show on waking.
#[derive(Debug)]
pub struct Ticker {
    last_check: DateTime<Utc>,
    last_tick: DateTime<Utc>,
    /// The last time a look at what is due was asked for. A waking doesn't move it (it moves
    /// `last_check`): a computer that never stays awake for a quarter of an hour would
    /// otherwise never be asked.
    last_startup: DateTime<Utc>,
    /// No look at what is due before this: two minutes after the last waking.
    startup_not_before: DateTime<Utc>,
}

impl Ticker {
    /// Started at launch, whose delivery the page does itself.
    pub fn new(now: DateTime<Utc>) -> Self {
        Ticker {
            last_check: now,
            last_tick: now,
            last_startup: now,
            startup_not_before: now,
        }
    }

    /// What to ask for now: 15 minutes after the last time, at once after a sleep, and when the
    /// clock went back (a check "in the future" would otherwise wait for it).
    pub fn tick(&mut self, now: DateTime<Utc>) -> Asks {
        let gap = now - self.last_tick;
        let woke = gap > WOKE_AFTER || gap < TimeDelta::zero();
        self.last_tick = now;
        let due = now - self.last_check >= CHECK_EVERY || now < self.last_check;
        if due || woke {
            self.last_check = now;
        }
        if woke {
            self.startup_not_before = now + STARTUP_AFTER_WAKING;
        }
        let startup = !woke
            && now >= self.startup_not_before
            && (now - self.last_startup >= CHECK_EVERY || now < self.last_startup);
        if startup {
            self.last_startup = now;
        }
        Asks {
            reminders: due || woke,
            startup,
        }
    }
}

/// A notification as the page words it (course code and titles only).
#[derive(Clone, Debug, Deserialize)]
pub struct NotificationText {
    /// The reminder's id, marked shown once the notification is out.
    pub id: String,
    pub title: String,
    pub body: String,
}

#[tauri::command]
pub async fn reminder_settings<R: Runtime>(
    app: AppHandle<R>,
    backend: State<'_, Backend>,
) -> CmdResult<ReminderSettings> {
    let settings = backend
        .blocking(|facade| facade.reminder_settings())
        .await?;
    // A launch that couldn't read it (the core opened later): the tray follows it now.
    background::sync_stored(&app, settings.run_in_background);
    Ok(settings)
}

/// Saves the settings, then follows `run_in_background` (tray, login item, close button), from
/// what was stored before: the login item changes only when that setting does.
#[tauri::command]
pub async fn set_reminder_settings<R: Runtime>(
    app: AppHandle<R>,
    backend: State<'_, Backend>,
    settings: ReminderSettings,
) -> CmdResult<BackgroundStatus> {
    let on = settings.run_in_background;
    // One save at a time, so quick toggles end with the tray and the login item as last saved.
    let background = app.state::<Background>();
    let _one_at_a_time = background.settings_saves.lock().await;
    let was = backend
        .blocking(move |facade| save_settings(facade, &settings))
        .await?;
    Ok(background::apply(&app, was, on))
}

/// Saves the settings; returns `run_in_background` as it was stored before.
fn save_settings(facade: &App, settings: &ReminderSettings) -> Result<bool, AppError> {
    let was = facade.reminder_settings()?.run_in_background;
    facade.set_reminder_settings(settings)?;
    Ok(was)
}

#[tauri::command]
pub async fn due_reminders(backend: State<'_, Backend>) -> CmdResult<Vec<Reminder>> {
    backend.blocking(|app| app.due_reminders(Utc::now())).await
}

/// Shows the notifications, then marks their reminders shown. Nothing unless the student said
/// "Remind me", as stored (checked here too: an opt-out can land between the page's check and
/// this call).
/// On desktop the plugin hands each notification to the system without waiting and reports no
/// failure, so every one handed over counts as shown; the Err branch covers only a failure to
/// build it (Windows: finding the executable).
#[tauri::command]
pub async fn show_reminders<R: Runtime>(
    app: AppHandle<R>,
    backend: State<'_, Backend>,
    notifications: Vec<NotificationText>,
) -> CmdResult<()> {
    let on = backend
        .blocking(|facade| facade.reminder_settings().map(|s| s.run_in_background))
        .await?;
    background::sync_stored(&app, on);
    if !on {
        return Ok(());
    }
    let mut shown = Vec::new();
    for notification in notifications {
        match notify(&app, &notification.title, &notification.body) {
            Ok(()) => shown.push(notification.id),
            Err(error) => {
                tracing::warn!(target: "pagelamp::reminders", %error, "notification not shown")
            }
        }
    }
    if shown.is_empty() {
        return Ok(());
    }
    backend
        .blocking(move |facade| facade.mark_reminders_shown(&shown))
        .await
}

/// Seen in the app (the catch-up card while reminders are off): they don't come back.
#[tauri::command]
pub async fn mark_reminders_shown(backend: State<'_, Backend>, ids: Vec<String>) -> CmdResult<()> {
    backend
        .blocking(move |app| app.mark_reminders_shown(&ids))
        .await
}

/// The one notification when the student turns reminders on: where the system asks whether
/// PageLamp may notify (desktop systems have no other way to ask). Marks nothing.
#[tauri::command]
pub fn show_reminders_on_notice<R: Runtime>(app: AppHandle<R>, title: String, body: String) {
    if !app.state::<Background>().is_on() {
        return;
    }
    if let Err(error) = notify(&app, &title, &body) {
        tracing::warn!(target: "pagelamp::reminders", %error, "notice not shown");
    }
}

/// "Not seeing reminders?" in Settings: opens the system's notification settings. false where
/// there is no standard place to open (Linux desktops differ).
#[tauri::command]
pub fn open_notification_settings<R: Runtime>(app: AppHandle<R>) -> bool {
    #[cfg(target_os = "macos")]
    let url = Some("x-apple.systempreferences:com.apple.Notifications-Settings.extension");
    #[cfg(windows)]
    let url = Some("ms-settings:notifications");
    #[cfg(not(any(target_os = "macos", windows)))]
    let url: Option<&str> = None;
    let Some(url) = url else {
        return false;
    };
    // Rust-side: the webview may still open only http(s) links (capabilities/default.json).
    match app.opener().open_url(url, None::<&str>) {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(target: "pagelamp::reminders", %error, "open notification settings");
            false
        }
    }
}

/// Every notification PageLamp shows goes through here. On macOS the plugin delivers through
/// NSUserNotificationCenter (deprecated): if a signed build shows no banner, the switch to
/// UNUserNotificationCenter happens in this one place.
fn notify<R: Runtime>(
    app: &AppHandle<R>,
    title: &str,
    body: &str,
) -> tauri_plugin_notification::Result<()> {
    app.notification().builder().title(title).body(body).show()
}

/// Starts the ticker for the app's lifetime.
pub fn start<R: Runtime>(app: AppHandle<R>) {
    let spawned = std::thread::Builder::new()
        .name("reminders".into())
        .spawn(move || {
            let mut ticker = Ticker::new(Utc::now());
            loop {
                std::thread::sleep(TICK);
                let asks = ticker.tick(Utc::now());
                if asks.reminders
                    && let Err(error) = app.emit_to("main", CHECK_EVENT, ())
                {
                    tracing::warn!(target: "pagelamp::reminders", %error, "ask for a delivery");
                }
                if asks.startup
                    && let Err(error) = app.emit_to("main", STARTUP_CHECK_EVENT, ())
                {
                    tracing::warn!(target: "pagelamp::reminders", %error, "ask for a look at what is due");
                }
            }
        });
    if let Err(error) = spawned {
        tracing::warn!(target: "pagelamp::reminders", %error, "reminder ticker not started");
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, TimeDelta, TimeZone, Utc};

    use super::Ticker;

    #[test]
    fn a_save_returns_what_was_stored_so_the_login_item_stays_put() {
        let dir = tempfile::tempdir().expect("temp dir");
        let facade = super::App::open_at_with_secrets(
            dir.path().to_path_buf(),
            std::sync::Arc::new(pagelamp_core::secrets::MemorySecrets::new()),
        )
        .expect("open App in a temp dir");
        let on = super::ReminderSettings {
            run_in_background: true,
            ..super::ReminderSettings::default()
        };
        // Turning it on: nothing was stored before.
        assert!(!super::save_settings(&facade, &on).expect("save"));
        // Another setting saved while on (the digest time): stored on before, so apply() gets
        // true → true and leaves the login item alone, whatever `Background.on` says.
        let digest = super::ReminderSettings {
            digest_time: "10:00".into(),
            ..on.clone()
        };
        assert!(super::save_settings(&facade, &digest).expect("save"));
    }

    fn at(minutes: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 31, 23, 0, 0).unwrap() + TimeDelta::minutes(minutes)
    }

    /// Ticks once a minute from `from` to `to` (inclusive), returning the minutes that asked
    /// for a delivery.
    fn run(ticker: &mut Ticker, from: i64, to: i64) -> Vec<i64> {
        (from..=to)
            .filter(|&m| ticker.tick(at(m)).reminders)
            .collect()
    }

    /// Ticks once a minute from `from` to `to` (inclusive), returning the minutes that asked
    /// for a look at what is due.
    fn looks(ticker: &mut Ticker, from: i64, to: i64) -> Vec<i64> {
        (from..=to)
            .filter(|&m| ticker.tick(at(m)).startup)
            .collect()
    }

    #[test]
    fn asks_for_a_look_at_what_is_due_every_15_minutes_and_not_as_the_computer_wakes() {
        let mut ticker = Ticker::new(at(0));
        assert_eq!(looks(&mut ticker, 1, 31), vec![15, 30]);
        // Asleep from minute 31 to minute 151, through several regular ticks. The tick after
        // waking asks for a delivery and for nothing else: the network may not be back.
        let woke = ticker.tick(at(151));
        assert!(woke.reminders && !woke.startup, "{woke:?}");
        // Two minutes later it asks for what came due meanwhile, then every 15 minutes again.
        assert_eq!(looks(&mut ticker, 152, 170), vec![153, 168]);
    }

    #[test]
    fn a_short_sleep_moves_no_look_that_isnt_due_yet() {
        let mut ticker = Ticker::new(at(0));
        assert_eq!(looks(&mut ticker, 1, 5), Vec::<i64>::new());
        // Asleep from minute 5 to minute 9: nothing was due, and nothing is two minutes later.
        assert!(!ticker.tick(at(9)).startup);
        assert_eq!(looks(&mut ticker, 10, 20), vec![15]);
    }

    #[test]
    fn a_computer_that_never_stays_awake_for_15_minutes_is_asked_all_the_same() {
        let mut ticker = Ticker::new(at(0));
        // Awake for 12 minutes, asleep for 8, again and again.
        let mut asked = Vec::new();
        let mut minute = 0;
        for _ in 0..4 {
            asked.extend(looks(&mut ticker, minute + 1, minute + 12));
            minute += 20;
            assert!(!ticker.tick(at(minute)).startup, "the tick after waking");
        }
        // Two minutes into the second stretch (the first look was due while it slept), and
        // from then on whenever a quarter of an hour has passed since the last one.
        assert_eq!(asked, vec![22, 42, 62]);
    }

    #[test]
    fn asks_for_a_look_two_minutes_after_the_clock_went_back() {
        let mut ticker = Ticker::new(at(0));
        assert_eq!(looks(&mut ticker, 1, 16), vec![15]);
        let back = ticker.tick(at(-60));
        assert!(back.reminders && !back.startup, "{back:?}");
        // The last look is "in the future" now: asked for as soon as the two minutes are over.
        assert_eq!(looks(&mut ticker, -59, -40), vec![-58, -43]);
    }

    #[test]
    fn asks_every_15_minutes_but_not_at_launch() {
        let mut ticker = Ticker::new(at(0));
        assert_eq!(run(&mut ticker, 1, 46), vec![15, 30, 45]);
    }

    #[test]
    fn asks_at_once_after_a_sleep() {
        let mut ticker = Ticker::new(at(0));
        assert_eq!(run(&mut ticker, 1, 5), Vec::<i64>::new());
        // Asleep from minute 5 to minute 125: the first tick after waking asks.
        assert!(ticker.tick(at(125)).reminders);
        // And the 15 minutes count from there.
        assert_eq!(run(&mut ticker, 126, 141), vec![140]);
    }

    #[test]
    fn a_short_sleep_counts_as_waking_too() {
        let mut ticker = Ticker::new(at(0));
        assert_eq!(run(&mut ticker, 1, 3), Vec::<i64>::new());
        assert!(ticker.tick(at(6)).reminders, "three minutes between ticks");
    }

    #[test]
    fn asks_when_the_clock_goes_back() {
        let mut ticker = Ticker::new(at(0));
        assert_eq!(run(&mut ticker, 1, 10), Vec::<i64>::new());
        assert!(
            ticker.tick(at(-60)).reminders,
            "the clock went back an hour"
        );
        assert_eq!(run(&mut ticker, -59, -44), vec![-45]);
    }
}
