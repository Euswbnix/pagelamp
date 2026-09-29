//! The HTTP rules of every model call: HTTPS unless loopback, no redirects, an honest
//! User-Agent, keys in sensitive headers only.

use std::net::IpAddr;
use std::time::Duration;

use pagelamp_core::ai::ModelErrorKind;
use pagelamp_core::brand;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use url::Url;

use crate::error::ModelError;
use crate::profile::{ApiKey, Auth};

/// Connecting (TCP + TLS) may take this long.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Why a base URL can't be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BaseUrlProblem {
    #[error("not a valid URL")]
    Invalid,
    /// Plain `http://` to a host that isn't this computer: a key would travel unencrypted.
    #[error("the address must start with https:// (http:// is only allowed for this computer)")]
    NotHttps,
    #[error("the address can't contain a user name or password")]
    HasCredentials,
    #[error("the address can't contain a query (?…) or fragment (#…)")]
    HasQuery,
}

/// Parse and check a provider base URL (`https://api.example.com/v1`). A trailing slash is
/// dropped, so paths can be appended with `join_path`.
pub fn check_base_url(raw: &str) -> Result<Url, BaseUrlProblem> {
    let url = Url::parse(raw.trim()).map_err(|_| BaseUrlProblem::Invalid)?;
    match url.scheme() {
        "https" => {}
        "http" if is_loopback(&url) => {}
        "http" => return Err(BaseUrlProblem::NotHttps),
        _ => return Err(BaseUrlProblem::Invalid),
    }
    if url.host_str().is_none_or(str::is_empty) {
        return Err(BaseUrlProblem::Invalid);
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(BaseUrlProblem::HasCredentials);
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(BaseUrlProblem::HasQuery);
    }
    let mut url = url;
    let path = url.path().trim_end_matches('/').to_string();
    url.set_path(&path);
    Ok(url)
}

/// Whether `url` points at this computer (`localhost`, 127.0.0.0/8, `::1`).
pub fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => IpAddr::V4(ip).is_loopback(),
        Some(url::Host::Ipv6(ip)) => IpAddr::V6(ip).is_loopback(),
        None => false,
    }
}

/// `base` + `/path` (`base` has no trailing slash, `path` starts with one).
pub(crate) fn join_path(base: &Url, path: &str) -> Url {
    let mut url = base.clone();
    let joined = format!("{}{}", base.path().trim_end_matches('/'), path);
    url.set_path(&joined);
    url
}

/// The shared client: no redirects (a key must never follow one), `User-Agent: PageLamp/<v>`.
pub(crate) fn build_client() -> Result<reqwest::Client, ModelError> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(format!(
            "{}/{}",
            brand::PRODUCT_NAME,
            env!("CARGO_PKG_VERSION")
        ))
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .map_err(|_| ModelError::new(ModelErrorKind::Network, "could not set up the connection"))
}

/// Headers for `auth` with `key`; the key header is marked sensitive (never printed).
pub(crate) fn auth_headers(auth: Auth, key: Option<&ApiKey>) -> Result<HeaderMap, ModelError> {
    let mut headers = HeaderMap::new();
    let Some(key) = key else {
        return Ok(headers);
    };
    let bad_key = || {
        ModelError::new(
            ModelErrorKind::AuthRejected,
            "the key contains characters that can't be sent",
        )
    };
    match auth {
        Auth::None => {}
        Auth::Bearer => {
            let mut value = HeaderValue::from_str(&format!("Bearer {}", key.expose()))
                .map_err(|_| bad_key())?;
            value.set_sensitive(true);
            headers.insert(reqwest::header::AUTHORIZATION, value);
        }
        Auth::XApiKey => {
            let mut value = HeaderValue::from_str(key.expose()).map_err(|_| bad_key())?;
            value.set_sensitive(true);
            headers.insert(HeaderName::from_static("x-api-key"), value);
        }
    }
    Ok(headers)
}

/// `retry-after` in seconds (the HTTP-date form is ignored: providers send seconds).
pub(crate) fn retry_after(headers: &HeaderMap) -> Option<Duration> {
    let value = headers.get(reqwest::header::RETRY_AFTER)?.to_str().ok()?;
    let seconds: f64 = value.trim().parse().ok()?;
    (seconds.is_finite() && seconds >= 0.0).then(|| Duration::from_secs_f64(seconds))
}

/// The provider's request id (`x-request-id` / `request-id`).
pub(crate) fn request_id(headers: &HeaderMap) -> Option<String> {
    ["x-request-id", "request-id"]
        .iter()
        .find_map(|name| headers.get(*name)?.to_str().ok())
        .map(|id| id.chars().take(100).collect())
}

/// A transport failure as a model error (no URL: it can carry a user's custom host path).
pub(crate) fn transport_error(error: &reqwest::Error) -> ModelError {
    if error.is_timeout() {
        ModelError::new(
            ModelErrorKind::Timeout,
            "the service took too long to answer",
        )
    } else {
        ModelError::new(ModelErrorKind::Network, "could not reach the service")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_base_urls_need_https_and_loopback_may_use_http() {
        assert_eq!(
            check_base_url("https://api.openai.com/v1/")
                .unwrap()
                .as_str(),
            "https://api.openai.com/v1"
        );
        assert!(check_base_url("http://localhost:11434").is_ok());
        assert!(check_base_url("http://127.0.0.1:1234/v1").is_ok());
        assert!(check_base_url("http://[::1]:1234/v1").is_ok());
        assert_eq!(
            check_base_url("http://api.example.com/v1"),
            Err(BaseUrlProblem::NotHttps)
        );
        // Looks local, isn't.
        assert_eq!(
            check_base_url("http://localhost.example.com/v1"),
            Err(BaseUrlProblem::NotHttps)
        );
        assert_eq!(
            check_base_url("http://127.0.0.1.nip.io/v1"),
            Err(BaseUrlProblem::NotHttps)
        );
        assert_eq!(
            check_base_url("https://user:pw@api.example.com"),
            Err(BaseUrlProblem::HasCredentials)
        );
        assert_eq!(
            check_base_url("https://api.example.com/v1?key=x"),
            Err(BaseUrlProblem::HasQuery)
        );
        assert_eq!(
            check_base_url("ftp://api.example.com"),
            Err(BaseUrlProblem::Invalid)
        );
        assert_eq!(check_base_url("not a url"), Err(BaseUrlProblem::Invalid));
    }

    #[test]
    fn paths_are_appended_to_the_base_path() {
        let base = check_base_url("https://openrouter.ai/api/v1").unwrap();
        assert_eq!(
            join_path(&base, "/chat/completions").as_str(),
            "https://openrouter.ai/api/v1/chat/completions"
        );
        let root = check_base_url("http://localhost:11434").unwrap();
        assert_eq!(
            join_path(&root, "/api/chat").as_str(),
            "http://localhost:11434/api/chat"
        );
    }

    #[test]
    fn key_headers_are_sensitive() {
        let key = ApiKey::new("sk-demo-not-a-real-key");
        let headers = auth_headers(Auth::Bearer, Some(&key)).unwrap();
        let value = &headers[reqwest::header::AUTHORIZATION];
        assert!(value.is_sensitive());
        assert!(!format!("{headers:?}").contains("sk-demo"));
        let headers = auth_headers(Auth::XApiKey, Some(&key)).unwrap();
        assert!(headers["x-api-key"].is_sensitive());
        assert!(auth_headers(Auth::None, Some(&key)).unwrap().is_empty());
    }

    #[test]
    fn retry_after_is_read_in_seconds() {
        let mut headers = HeaderMap::new();
        headers.insert(reqwest::header::RETRY_AFTER, HeaderValue::from_static("7"));
        assert_eq!(retry_after(&headers), Some(Duration::from_secs(7)));
        headers.insert(
            reqwest::header::RETRY_AFTER,
            HeaderValue::from_static("Wed, 21 Oct 2026 07:28:00 GMT"),
        );
        assert_eq!(retry_after(&headers), None);
    }
}
