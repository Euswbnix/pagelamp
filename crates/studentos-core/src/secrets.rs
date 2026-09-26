//! OS keychain storage for source secrets (Canvas access token, calendar-feed URL).
//!
//! Keychain service name: `dev.studentos`; account = source id (e.g. `canvas:lms.example.edu`).
//! For CI/tests an environment override is honoured: `STUDENTOS_SECRET_<SANITISED_ID>`
//! where the id is upper-cased and every non-alphanumeric char becomes `_`
//! (e.g. `STUDENTOS_SECRET_CANVAS_LMS_EXAMPLE_EDU`).
//!
//! Secrets must never be written to the database, logs, or MCP output. Error messages from
//! this module therefore never contain a secret (see `describe_keyring_error`).

use crate::{Error, Result};

pub const KEYCHAIN_SERVICE: &str = "dev.studentos";

const ENV_OVERRIDE_PREFIX: &str = "STUDENTOS_SECRET_";

/// Name of the env var that overrides the keychain for `source_id`: the prefix plus the id
/// upper-cased, with every character that is not an ASCII letter/digit replaced by `_`.
pub fn env_override_name(source_id: &str) -> String {
    let sanitised: String = source_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    format!("{ENV_OVERRIDE_PREFIX}{sanitised}")
}

/// Store `secret` in the OS keychain (replacing any previous value).
/// Errors: `Invalid` for an empty id/secret; `Secret` when the keychain fails.
pub fn set_secret(source_id: &str, secret: &str) -> Result<()> {
    if source_id.is_empty() {
        return Err(Error::Invalid("source id must not be empty".to_string()));
    }
    if secret.is_empty() {
        return Err(Error::Invalid("secret must not be empty".to_string()));
    }
    keychain_entry(source_id)?
        .set_password(secret)
        .map_err(|err| store_error(source_id, secret, &err))
}

/// Error for a failed `set_secret`. Platform messages should never echo the value being
/// stored, but it is redacted anyway (belt and braces).
fn store_error(source_id: &str, secret: &str, err: &keyring::Error) -> Error {
    let message = keychain_error_message("store", source_id, err);
    Error::Secret(message.replace(secret, "[redacted]"))
}

/// Env override first, then keychain. `Ok(None)` when absent.
pub fn get_secret(source_id: &str) -> Result<Option<String>> {
    get_secret_with(source_id, |name| std::env::var(name).ok())
}

/// `get_secret` with the environment lookup passed in (`env_lookup(var_name)` returns the
/// variable's value, if set). Lets this module's tests exercise the override without touching
/// the real environment. Private on purpose: when the override is absent it falls through to
/// the real OS keychain, so it is no "fake secrets" hook for other crates' tests.
fn get_secret_with(
    source_id: &str,
    env_lookup: impl FnOnce(&str) -> Option<String>,
) -> Result<Option<String>> {
    if let Some(secret) = env_override(source_id, env_lookup) {
        return Ok(Some(secret));
    }
    read_result(source_id, keychain_entry(source_id)?.get_password())
}

/// Remove from keychain; absent is not an error.
pub fn delete_secret(source_id: &str) -> Result<()> {
    delete_result(source_id, keychain_entry(source_id)?.delete_credential())
}

/// Outcome of a keychain read: a missing entry is `Ok(None)`, not an error.
/// (Separate from `get_secret_with` so it can be tested without a keychain.)
fn read_result(source_id: &str, result: keyring::Result<String>) -> Result<Option<String>> {
    match result {
        Ok(secret) => Ok(Some(secret)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(Error::Secret(keychain_error_message(
            "read", source_id, &err,
        ))),
    }
}

/// Outcome of a keychain delete: deleting a missing entry succeeds (e.g. a folder source,
/// which never had a secret). (Separate from `delete_secret` so it can be tested without a
/// keychain.)
fn delete_result(source_id: &str, result: keyring::Result<()>) -> Result<()> {
    match result {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(Error::Secret(keychain_error_message(
            "delete", source_id, &err,
        ))),
    }
}

/// The env override value for `source_id`, if set and non-empty.
fn env_override(
    source_id: &str,
    env_lookup: impl FnOnce(&str) -> Option<String>,
) -> Option<String> {
    env_lookup(&env_override_name(source_id)).filter(|value| !value.is_empty())
}

fn keychain_entry(source_id: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(KEYCHAIN_SERVICE, source_id)
        .map_err(|err| Error::Secret(keychain_error_message("open", source_id, &err)))
}

/// "could not <action> the keychain entry for '<source_id>': <reason>".
fn keychain_error_message(action: &str, source_id: &str, err: &keyring::Error) -> String {
    let reason = describe_keyring_error(err);
    format!("could not {action} the keychain entry for '{source_id}': {reason}")
}

/// A secret-free description of a keyring error. Deliberately does NOT use the error's
/// `Display`: some variants carry the stored bytes (i.e. the secret itself).
fn describe_keyring_error(err: &keyring::Error) -> String {
    use keyring::Error as E;
    match err {
        // Platform errors are OS status codes/messages, not stored data.
        E::PlatformFailure(platform) => format!("keychain failure ({platform})"),
        E::NoStorageAccess(platform) => format!("keychain is not accessible ({platform})"),
        E::NoEntry => "no entry".to_string(),
        E::BadEncoding(_) => "stored value is not valid UTF-8".to_string(),
        E::BadDataFormat(..) => "stored value is malformed".to_string(),
        E::BadStoreFormat(_) => "keychain data is malformed".to_string(),
        E::TooLong(attribute, limit) => {
            format!("'{attribute}' is longer than the platform limit of {limit}")
        }
        E::Invalid(attribute, _) => format!("invalid '{attribute}'"),
        E::Ambiguous(entries) => format!("{} keychain entries match", entries.len()),
        E::NoDefaultStore => "no keychain is available on this system".to_string(),
        E::NotSupportedByStore(_) => "operation not supported by this keychain".to_string(),
        // `keyring::Error` is non-exhaustive.
        _ => "unexpected keychain error".to_string(),
    }
}

#[cfg(test)]
mod tests {
    //! These tests never touch the real OS keychain or the process environment.

    use super::*;

    const DEMO_ID: &str = "canvas:lms.example.edu";
    const DEMO_SECRET: &str = "demo-not-a-real-token-123";

    #[test]
    fn env_override_name_sanitises_id() {
        assert_eq!(
            env_override_name(DEMO_ID),
            "STUDENTOS_SECRET_CANVAS_LMS_EXAMPLE_EDU"
        );
        assert_eq!(
            env_override_name("ical:9b1c-2f"),
            "STUDENTOS_SECRET_ICAL_9B1C_2F"
        );
        // Non-ASCII characters become '_' too (one per character).
        assert_eq!(
            env_override_name("folder:é漢"),
            "STUDENTOS_SECRET_FOLDER___"
        );
        assert_eq!(env_override_name(""), "STUDENTOS_SECRET_");
    }

    #[test]
    fn env_override_is_looked_up_by_sanitised_name() {
        let found = env_override(DEMO_ID, |name| {
            assert_eq!(name, "STUDENTOS_SECRET_CANVAS_LMS_EXAMPLE_EDU");
            Some(DEMO_SECRET.to_string())
        });
        assert_eq!(found.as_deref(), Some(DEMO_SECRET));
    }

    #[test]
    fn empty_or_missing_env_override_is_ignored() {
        assert_eq!(env_override(DEMO_ID, |_| Some(String::new())), None);
        assert_eq!(env_override(DEMO_ID, |_| None), None);
    }

    #[test]
    fn get_secret_with_prefers_env_override() {
        // Returns before the keychain would be consulted.
        let secret = get_secret_with(DEMO_ID, |_| Some(DEMO_SECRET.to_string())).unwrap();
        assert_eq!(secret.as_deref(), Some(DEMO_SECRET));
    }

    #[test]
    fn missing_keychain_entry_reads_as_none() {
        let read = |result| read_result(DEMO_ID, result);
        assert_eq!(read(Err(keyring::Error::NoEntry)).unwrap(), None);
        assert_eq!(
            read(Ok(DEMO_SECRET.to_string())).unwrap().as_deref(),
            Some(DEMO_SECRET)
        );
        let failure = keyring::Error::PlatformFailure(Box::new(std::io::Error::other("locked")));
        match read(Err(failure)) {
            Err(Error::Secret(message)) => assert!(message.contains("could not read"), "{message}"),
            other => panic!("expected Error::Secret, got {other:?}"),
        }
    }

    #[test]
    fn deleting_a_missing_keychain_entry_succeeds() {
        assert!(delete_result(DEMO_ID, Err(keyring::Error::NoEntry)).is_ok());
        assert!(delete_result(DEMO_ID, Ok(())).is_ok());
        let failure = keyring::Error::NoStorageAccess(Box::new(std::io::Error::other("locked")));
        match delete_result(DEMO_ID, Err(failure)) {
            Err(Error::Secret(message)) => {
                assert!(message.contains("could not delete"), "{message}")
            }
            other => panic!("expected Error::Secret, got {other:?}"),
        }
    }

    #[test]
    fn set_secret_rejects_empty_values_before_keychain() {
        assert!(matches!(set_secret(DEMO_ID, ""), Err(Error::Invalid(_))));
        assert!(matches!(
            set_secret("", DEMO_SECRET),
            Err(Error::Invalid(_))
        ));
    }

    #[test]
    fn error_messages_never_contain_stored_bytes() {
        let errors = [
            keyring::Error::BadEncoding(DEMO_SECRET.as_bytes().to_vec()),
            keyring::Error::BadDataFormat(
                DEMO_SECRET.as_bytes().to_vec(),
                Box::new(std::io::Error::other(format!("bad blob {DEMO_SECRET}"))),
            ),
            keyring::Error::Invalid("password".to_string(), DEMO_SECRET.to_string()),
            keyring::Error::BadStoreFormat(DEMO_SECRET.to_string()),
            keyring::Error::NotSupportedByStore(DEMO_SECRET.to_string()),
        ];
        for err in &errors {
            let message = keychain_error_message("read", DEMO_ID, err);
            assert!(!message.contains(DEMO_SECRET), "leaked in: {message}");
            assert!(message.contains(DEMO_ID));
        }
    }

    #[test]
    fn store_error_redacts_the_secret() {
        let err = keyring::Error::PlatformFailure(Box::new(std::io::Error::other(format!(
            "refused value {DEMO_SECRET}"
        ))));
        let Error::Secret(message) = store_error(DEMO_ID, DEMO_SECRET, &err) else {
            panic!("expected Error::Secret");
        };
        assert!(!message.contains(DEMO_SECRET), "leaked in: {message}");
        assert!(message.contains("[redacted]"));
    }

    #[test]
    fn error_message_describes_platform_failures() {
        let err = keyring::Error::NoStorageAccess(Box::new(std::io::Error::other("locked")));
        let message = keychain_error_message("store", DEMO_ID, &err);
        assert_eq!(
            message,
            "could not store the keychain entry for 'canvas:lms.example.edu': \
             keychain is not accessible (locked)"
        );
    }
}
