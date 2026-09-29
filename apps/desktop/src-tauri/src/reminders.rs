//! Reminders while PageLamp runs (v0.3 M3; design §5.3). A ticker asks the page to deliver due
//! reminders every 15 minutes and right after the computer wakes. The page fetches them
//! (`due_reminders`), words them in the student's language (course code and titles only, never
//! material text) and hands them to `show_reminders`, which shows them and marks them shown.
//! What is due, the catch-up window and the dedupe are the facade's; the page's own check at
//! launch is the catch-up.

use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};
use tauri::{AppHandle, Emitter, Runtime};

/// The event the page answers with a delivery.
pub const CHECK_EVENT: &str = "reminders:check";
/// How often the ticker looks at the clock.
const TICK: Duration = Duration::from_secs(60);
/// How often it asks for a delivery.
const CHECK_EVERY: TimeDelta = TimeDelta::minutes(15);
/// A tick this much later than expected means the computer slept (or the clock was changed).
const WOKE_AFTER: TimeDelta = TimeDelta::minutes(2);

/// When to ask for a delivery, from the wall clock alone: the monotonic clock stops while a Mac
/// or a Linux laptop sleeps, and a reminder that came due meanwhile should show on waking.
#[derive(Debug)]
pub struct Ticker {
    last_check: DateTime<Utc>,
    last_tick: DateTime<Utc>,
}

impl Ticker {
    /// Started at launch, whose delivery the page does itself.
    pub fn new(now: DateTime<Utc>) -> Self {
        Ticker {
            last_check: now,
            last_tick: now,
        }
    }

    /// Whether to ask now: 15 minutes after the last time, at once after a sleep, and when the
    /// clock went back (a check "in the future" would otherwise wait for it).
    pub fn tick(&mut self, now: DateTime<Utc>) -> bool {
        let gap = now - self.last_tick;
        let woke = gap > WOKE_AFTER || gap < TimeDelta::zero();
        self.last_tick = now;
        let due = now - self.last_check >= CHECK_EVERY || now < self.last_check;
        if due || woke {
            self.last_check = now;
        }
        due || woke
    }
}

/// Starts the ticker for the app's lifetime.
pub fn start<R: Runtime>(app: AppHandle<R>) {
    let spawned = std::thread::Builder::new()
        .name("reminders".into())
        .spawn(move || {
            let mut ticker = Ticker::new(Utc::now());
            loop {
                std::thread::sleep(TICK);
                if ticker.tick(Utc::now())
                    && let Err(error) = app.emit_to("main", CHECK_EVENT, ())
                {
                    tracing::warn!(target: "pagelamp::reminders", %error, "ask for a delivery");
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

    fn at(minutes: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 31, 23, 0, 0).unwrap() + TimeDelta::minutes(minutes)
    }

    /// Ticks once a minute from `from` to `to` (inclusive), returning the minutes that asked.
    fn run(ticker: &mut Ticker, from: i64, to: i64) -> Vec<i64> {
        (from..=to).filter(|&m| ticker.tick(at(m))).collect()
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
        assert!(ticker.tick(at(125)));
        // And the 15 minutes count from there.
        assert_eq!(run(&mut ticker, 126, 141), vec![140]);
    }

    #[test]
    fn a_short_sleep_counts_as_waking_too() {
        let mut ticker = Ticker::new(at(0));
        assert_eq!(run(&mut ticker, 1, 3), Vec::<i64>::new());
        assert!(ticker.tick(at(6)), "three minutes between ticks");
    }

    #[test]
    fn asks_when_the_clock_goes_back() {
        let mut ticker = Ticker::new(at(0));
        assert_eq!(run(&mut ticker, 1, 10), Vec::<i64>::new());
        assert!(ticker.tick(at(-60)), "the clock went back an hour");
        assert_eq!(run(&mut ticker, -59, -44), vec![-45]);
    }
}
