//! One line in the log for every sync the window starts by itself: what triggered it and, for
//! an attended one, the kind of thing that noted the student and how long before. It is there
//! so that one can tell afterwards why a sync ran. Kinds only: never a key, a place in the
//! window or anything the student typed.
//!
//! Only the log: the facade is told the trigger with the sync itself (`SyncRequest`), and
//! nothing here decides anything.

use pagelamp_app::AutoSyncTrigger;
use serde::Deserialize;

/// What the student did that the page took for them being here (`StudentAction` in
/// `src/api/client.ts`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotedBy {
    /// They opened PageLamp, in a window they saw.
    Launch,
    /// A press in the window (mouse, pen or touch) after it gained focus.
    Press,
    /// A click with no press before it (assistive technology).
    Click,
    /// A key going down in the window after it gained focus.
    Key,
    /// The press that brought the window to the front, heard a moment before its focus.
    PressBeforeFocus,
    /// "Got it" on What's new.
    WhatsNew,
    /// The automatic sync setting, changed.
    Setting,
    /// "Try again" on a start that failed.
    TryAgain,
}

impl NotedBy {
    fn name(self) -> &'static str {
        match self {
            NotedBy::Launch => "launch",
            NotedBy::Press => "press",
            NotedBy::Click => "click",
            NotedBy::Key => "key",
            NotedBy::PressBeforeFocus => "press_before_focus",
            NotedBy::WhatsNew => "whats_new",
            NotedBy::Setting => "setting",
            NotedBy::TryAgain => "try_again",
        }
    }
}

/// The line for one start. An unattended one has nobody to name.
fn line(trigger: AutoSyncTrigger, noted_by: Option<NotedBy>, noted_ms_ago: Option<u64>) -> String {
    let noted = match (trigger, noted_by, noted_ms_ago) {
        (AutoSyncTrigger::Unattended, _, _) => return "automatic sync: unattended".to_string(),
        (AutoSyncTrigger::Attended, Some(by), Some(ms)) => {
            format!("{} {ms} ms before", by.name())
        }
        (AutoSyncTrigger::Attended, Some(by), None) => by.name().to_string(),
        (AutoSyncTrigger::Attended, None, _) => "nothing the page named".to_string(),
    };
    format!("automatic sync: attended, the student noted by {noted}")
}

/// The window is about to start a sync by itself (`useStartSync`, right before `sync_all`).
#[tauri::command]
pub fn log_auto_sync_start(
    trigger: AutoSyncTrigger,
    noted_by: Option<NotedBy>,
    noted_ms_ago: Option<u64>,
) {
    tracing::info!(target: "pagelamp::sync", "{}", line(trigger, noted_by, noted_ms_ago));
}

#[cfg(test)]
mod tests {
    use super::*;

    const KINDS: [(&str, NotedBy); 8] = [
        ("launch", NotedBy::Launch),
        ("press", NotedBy::Press),
        ("click", NotedBy::Click),
        ("key", NotedBy::Key),
        ("press_before_focus", NotedBy::PressBeforeFocus),
        ("whats_new", NotedBy::WhatsNew),
        ("setting", NotedBy::Setting),
        ("try_again", NotedBy::TryAgain),
    ];

    #[test]
    fn the_line_names_the_trigger_and_what_noted_the_student() {
        assert_eq!(
            line(AutoSyncTrigger::Attended, Some(NotedBy::Press), Some(412)),
            "automatic sync: attended, the student noted by press 412 ms before"
        );
        assert_eq!(
            line(
                AutoSyncTrigger::Attended,
                Some(NotedBy::PressBeforeFocus),
                Some(0)
            ),
            "automatic sync: attended, the student noted by press_before_focus 0 ms before"
        );
        // An unattended start names nobody, whatever came with it.
        assert_eq!(
            line(AutoSyncTrigger::Unattended, None, None),
            "automatic sync: unattended"
        );
        assert_eq!(
            line(AutoSyncTrigger::Unattended, Some(NotedBy::Key), Some(5)),
            "automatic sync: unattended"
        );
        // An attended start the page couldn't account for still says so.
        assert_eq!(
            line(AutoSyncTrigger::Attended, None, None),
            "automatic sync: attended, the student noted by nothing the page named"
        );
        assert_eq!(
            line(AutoSyncTrigger::Attended, Some(NotedBy::Launch), None),
            "automatic sync: attended, the student noted by launch"
        );
    }

    #[test]
    fn every_kind_is_read_from_the_page_and_written_as_it_was_sent() {
        for (sent, kind) in KINDS {
            let read: NotedBy = serde_json::from_value(serde_json::json!(sent)).expect(sent);
            assert_eq!(read, kind);
            assert_eq!(kind.name(), sent);
        }
        // Nothing else is a kind: a key's name or a place could not come through as one.
        for other in ["Enter", "keydown", "button#sync", ""] {
            assert!(serde_json::from_value::<NotedBy>(serde_json::json!(other)).is_err());
        }
    }

    #[test]
    fn the_log_keeps_the_line_whole() {
        // The filter every log line goes through takes `key=…` and the like for secrets.
        for (_, kind) in KINDS {
            let written = line(AutoSyncTrigger::Attended, Some(kind), Some(29_999));
            assert_eq!(pagelamp_core::diagnostics::redact(&written), written);
        }
        let written = line(AutoSyncTrigger::Unattended, None, None);
        assert_eq!(pagelamp_core::diagnostics::redact(&written), written);
    }
}
