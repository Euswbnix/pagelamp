//! How Canvas is reached. `CanvasTransport` is the seam for other transports (OAuth, a
//! browser extension); only `TokenTransport` (personal access token over HTTPS) exists now.
//!
//! Security properties of `TokenTransport`:
//! - Only GET. There is no method that could send anything else.
//! - API calls go only to the Canvas origin (scheme + host + port); anything else is refused
//!   before a request is made.
//! - Automatic redirects are disabled. File downloads start only at a Canvas file link
//!   (`/files/<id>/download`), follow redirects by hand, attach the `Authorization` header
//!   only while the chain has never left the Canvas origin, and refuse to be redirected back to
//!   Canvas afterwards — so the token never reaches file storage hosts (S3 / inst-fs signed
//!   URLs don't need it) and a redirect can't turn a download into an arbitrary API call.
//!   Plain http is refused except on the Canvas origin itself (http://localhost in tests).
//! - The token is never logged and never appears in errors (`Debug` is not derived; the header
//!   value is marked sensitive).
//! - At most 2 requests at a time; exponential backoff on throttling (429, or 403 "Rate Limit
//!   Exceeded"): 1s, 2s, 4s, 8s → `RateLimited` after 5 tries; slowing down when
//!   `X-Rate-Limit-Remaining` drops below 100.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::Duration;

use reqwest::StatusCode;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue, LINK, LOCATION};
use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;
use url::Url;

use crate::endpoint::{Next, next_link, same_origin};

/// Why a request failed. Mapped to `SourceError` (or a per-course warning) by the sync.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CanvasError {
    /// The token is invalid, expired or revoked (abort the sync).
    Unauthorized,
    /// This resource isn't available to the student (hidden tab, locked file): warn, continue.
    Forbidden,
    NotFound,
    /// Still throttled after all retries.
    RateLimited,
    /// DNS, TLS, timeout, connection refused, interrupted body.
    Network(String),
    /// Any other HTTP status.
    Http(u16),
    /// Download larger than allowed.
    TooLarge,
    /// Unexpected response (not JSON, redirect loop, foreign URL…).
    BadResponse(String),
    /// Writing a downloaded file failed (per-file problem).
    Io(String),
    /// The local database failed (aborts the sync: it would fail for every course).
    Store(String),
}

impl std::fmt::Display for CanvasError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CanvasError::Unauthorized => f.write_str("Canvas rejected the access token"),
            CanvasError::Forbidden => f.write_str("not available to you in Canvas"),
            CanvasError::NotFound => f.write_str("not found in Canvas"),
            CanvasError::RateLimited => f.write_str("Canvas is throttling requests"),
            CanvasError::Network(detail) => write!(f, "could not reach Canvas ({detail})"),
            CanvasError::Http(code) => write!(f, "Canvas answered HTTP {code}"),
            CanvasError::TooLarge => f.write_str("larger than the download limit"),
            CanvasError::BadResponse(detail) => {
                write!(f, "unexpected response from Canvas ({detail})")
            }
            CanvasError::Io(detail) => write!(f, "could not save the file ({detail})"),
            CanvasError::Store(detail) => write!(f, "could not save Canvas data ({detail})"),
        }
    }
}

/// One JSON response.
#[derive(Debug)]
pub(crate) struct JsonPage {
    pub body: serde_json::Value,
    /// What the `Link` header(s) say about the next page (checked by the caller).
    pub next: Next,
    /// Size of the response body (for per-listing memory budgets).
    pub bytes: usize,
}

/// Reaching Canvas. Implementations must only ever issue GET requests.
pub(crate) trait CanvasTransport: Send + Sync {
    /// GET one JSON document from the Canvas API (Canvas origin only).
    fn get_json(&self, url: Url) -> impl Future<Output = Result<JsonPage, CanvasError>> + Send;

    /// GET a file into `dest` (following redirects safely), at most `max_bytes`.
    /// Returns the number of bytes written. `dest` is replaced atomically.
    fn download(
        &self,
        url: Url,
        dest: PathBuf,
        max_bytes: u64,
    ) -> impl Future<Output = Result<u64, CanvasError>> + Send;
}

/// Retry/backoff knobs (tests use millisecond delays).
#[derive(Clone, Copy, Debug)]
pub(crate) struct RetryPolicy {
    pub base_delay: Duration,
    pub max_tries: u32,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        RetryPolicy {
            base_delay: Duration::from_secs(1),
            max_tries: 5,
        }
    }
}

const MAX_REDIRECTS: usize = 5;
/// Deadline for one API call (JSON pages are small).
const API_TIMEOUT: Duration = Duration::from_secs(120);
/// How much of an error body is read to classify it.
const MAX_ERROR_BODY: usize = 64 * 1024;
/// Largest JSON document accepted (a Canvas page holds ≤ 100 items).
const MAX_JSON_BYTES: usize = 32 * 1024 * 1024;

/// Personal-access-token transport.
pub(crate) struct TokenTransport {
    client: reqwest::Client,
    base: Url,
    auth: HeaderValue,
    permits: Semaphore,
    retry: RetryPolicy,
}

/// A response whose (API-sized) body has been read.
struct Fetched {
    status: StatusCode,
    headers: HeaderMap,
    body: Vec<u8>,
}

impl TokenTransport {
    pub(crate) fn new(base: Url, token: &str, retry: RetryPolicy) -> Result<Self, CanvasError> {
        let mut auth =
            HeaderValue::from_str(&format!("Bearer {}", token.trim())).map_err(|_| {
                CanvasError::BadResponse("the token contains invalid characters".into())
            })?;
        auth.set_sensitive(true);
        // No total deadline (a large file on a slow link would never finish); instead a
        // connect timeout and a read timeout between chunks. API calls add a per-request
        // deadline in `fetch_api`.
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(30))
            .read_timeout(Duration::from_secs(60))
            .user_agent(concat!(
                "StudentOS/",
                env!("CARGO_PKG_VERSION"),
                " (read-only)"
            ))
            .build()
            .map_err(network_error)?;
        Ok(TokenTransport {
            client,
            base,
            auth,
            permits: Semaphore::new(2),
            retry,
        })
    }

    fn request(&self, url: &Url) -> reqwest::RequestBuilder {
        let request = self.client.get(url.clone());
        if same_origin(url, &self.base) {
            request.header(AUTHORIZATION, self.auth.clone())
        } else {
            request
        }
    }

    /// GET an API URL with throttling/backoff, reading the whole (capped) body.
    async fn fetch_api(&self, url: &Url) -> Result<Fetched, CanvasError> {
        if !same_origin(url, &self.base) {
            return Err(CanvasError::BadResponse(
                "refused to call a host other than Canvas".into(),
            ));
        }
        let mut delay = self.retry.base_delay;
        for attempt in 1..=self.retry.max_tries {
            let fetched = {
                let _permit = self
                    .permits
                    .acquire()
                    .await
                    .expect("semaphore is never closed");
                let mut response = self
                    .request(url)
                    .timeout(API_TIMEOUT)
                    .send()
                    .await
                    .map_err(network_error)?;
                let mut body = Vec::new();
                while let Some(chunk) = response.chunk().await.map_err(network_error)? {
                    if body.len() + chunk.len() > MAX_JSON_BYTES {
                        return Err(CanvasError::BadResponse("response too large".into()));
                    }
                    body.extend_from_slice(&chunk);
                }
                Fetched {
                    status: response.status(),
                    headers: response.headers().clone(),
                    body,
                }
            };
            self.pace(&fetched.headers).await;
            if !is_throttled(&fetched) {
                return Ok(fetched);
            }
            if attempt < self.retry.max_tries {
                tokio::time::sleep(delay).await;
                delay *= 2;
            }
        }
        Err(CanvasError::RateLimited)
    }

    /// Slow down when the per-user quota runs low: the lower, the slower.
    async fn pace(&self, headers: &HeaderMap) {
        let remaining = headers
            .get("X-Rate-Limit-Remaining")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<f64>().ok());
        if let Some(remaining) = remaining
            && remaining < 100.0
        {
            let factor = ((100.0 - remaining.max(0.0)) / 100.0).clamp(0.0, 1.0);
            tokio::time::sleep(self.retry.base_delay.mul_f64(factor)).await;
        }
    }
}

impl CanvasTransport for TokenTransport {
    async fn get_json(&self, url: Url) -> Result<JsonPage, CanvasError> {
        let fetched = self.fetch_api(&url).await?;
        let status = fetched.status;
        if status.is_success() {
            let text = String::from_utf8_lossy(&fetched.body);
            // Old Canvas versions prefixed JSON with `while(1);` (removed 2021); strip anyway.
            let json = text.trim_start().trim_start_matches("while(1);");
            let body = serde_json::from_str(json)
                .map_err(|_| CanvasError::BadResponse("not JSON".into()))?;
            let next = next_link(
                fetched.headers.get_all(LINK).iter().map(|v| v.as_bytes()),
                &url,
            );
            return Ok(JsonPage {
                body,
                next,
                bytes: fetched.body.len(),
            });
        }
        Err(classify_failure(fetched.status, &fetched.body))
    }

    async fn download(&self, url: Url, dest: PathBuf, max_bytes: u64) -> Result<u64, CanvasError> {
        if !same_origin(&url, &self.base) || !is_file_link(&url) {
            return Err(CanvasError::BadResponse("not a Canvas file link".into()));
        }
        let mut current = url;
        let mut left_canvas = false;
        for _hop in 0..=MAX_REDIRECTS {
            let on_canvas = same_origin(&current, &self.base);
            if on_canvas && (left_canvas || !is_file_link(&current)) {
                return Err(CanvasError::BadResponse(
                    "unexpected redirect back to Canvas".into(),
                ));
            }
            if !allowed_download_url(&current, &self.base) {
                return Err(CanvasError::BadResponse(
                    "refused an insecure download link".into(),
                ));
            }
            let response = self.send_download(&current, on_canvas).await?;
            let status = response.status();
            if status.is_redirection() {
                let location = response
                    .headers()
                    .get(LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .ok_or_else(|| {
                        CanvasError::BadResponse("redirect without a location".into())
                    })?;
                current = current
                    .join(location)
                    .map_err(|_| CanvasError::BadResponse("invalid redirect".into()))?;
                left_canvas |= !same_origin(&current, &self.base);
                continue;
            }
            if !status.is_success() {
                return Err(match status {
                    // On the Canvas origin a 401 may mean an expired token: look at the body.
                    StatusCode::UNAUTHORIZED if on_canvas => {
                        classify_failure(status, &read_capped(response, MAX_ERROR_BODY).await)
                    }
                    StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => CanvasError::Forbidden,
                    StatusCode::NOT_FOUND => CanvasError::NotFound,
                    StatusCode::TOO_MANY_REQUESTS => CanvasError::RateLimited,
                    other => CanvasError::Http(other.as_u16()),
                });
            }
            return save_body(response, &dest, max_bytes).await;
        }
        Err(CanvasError::BadResponse("too many redirects".into()))
    }
}

impl TokenTransport {
    /// One download hop, retried with backoff on 429/503. The token is attached only on the
    /// Canvas origin before the chain ever left it (`with_token`).
    async fn send_download(
        &self,
        url: &Url,
        with_token: bool,
    ) -> Result<reqwest::Response, CanvasError> {
        let mut delay = self.retry.base_delay;
        for attempt in 1..=self.retry.max_tries {
            let response = {
                let _permit = self
                    .permits
                    .acquire()
                    .await
                    .expect("semaphore is never closed");
                let request = self.client.get(url.clone());
                let request = if with_token {
                    request.header(AUTHORIZATION, self.auth.clone())
                } else {
                    request
                };
                request.send().await.map_err(network_error)?
            };
            let transient = matches!(
                response.status(),
                StatusCode::TOO_MANY_REQUESTS | StatusCode::SERVICE_UNAVAILABLE
            );
            if !transient || attempt == self.retry.max_tries {
                return Ok(response);
            }
            tokio::time::sleep(delay).await;
            delay *= 2;
        }
        unreachable!("the last attempt always returns")
    }
}

/// Canvas file links: `/files/<id>/download` or `/courses/<id>/files/<id>[/download]`.
fn is_file_link(url: &Url) -> bool {
    static FILE_LINK: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"^/(courses/\d+/)?files/\d+(/download)?/?$").expect("valid regex")
    });
    FILE_LINK.is_match(url.path())
}

/// Read at most `max` bytes of a body (for classifying errors).
async fn read_capped(mut response: reqwest::Response, max: usize) -> Vec<u8> {
    let mut body = Vec::new();
    while body.len() < max {
        match response.chunk().await {
            Ok(Some(chunk)) => body.extend_from_slice(&chunk[..chunk.len().min(max - body.len())]),
            _ => break,
        }
    }
    body
}

/// Removes an unfinished download on every exit path, including a cancelled sync.
struct PartialFile {
    path: PathBuf,
    done: bool,
}

impl Drop for PartialFile {
    fn drop(&mut self) {
        if !self.done {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Stream a download to `<dest>.part` (appended, so `x.part` can't collide with itself),
/// enforcing `max_bytes`, then rename over `dest`.
async fn save_body(
    mut response: reqwest::Response,
    dest: &Path,
    max_bytes: u64,
) -> Result<u64, CanvasError> {
    if response.content_length().is_some_and(|len| len > max_bytes) {
        return Err(CanvasError::TooLarge);
    }
    let io = |err: std::io::Error| CanvasError::Io(err.to_string());
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(io)?;
    }
    let mut partial_path = dest.as_os_str().to_owned();
    partial_path.push(".part");
    let mut partial = PartialFile {
        path: PathBuf::from(partial_path),
        done: false,
    };
    let mut file = tokio::fs::File::create(&partial.path).await.map_err(io)?;
    let mut written: u64 = 0;
    while let Some(chunk) = response.chunk().await.map_err(network_error)? {
        written += chunk.len() as u64;
        if written > max_bytes {
            return Err(CanvasError::TooLarge);
        }
        file.write_all(&chunk).await.map_err(io)?;
    }
    file.flush().await.map_err(io)?;
    drop(file);
    tokio::fs::rename(&partial.path, dest).await.map_err(io)?;
    partial.done = true;
    Ok(written)
}

/// 429, or 403 whose body says "Rate Limit Exceeded" (Canvas's throttling response).
fn is_throttled(fetched: &Fetched) -> bool {
    fetched.status == StatusCode::TOO_MANY_REQUESTS
        || (fetched.status == StatusCode::FORBIDDEN
            && String::from_utf8_lossy(&fetched.body).contains("Rate Limit Exceeded"))
}

/// Map a failed Canvas API response. Canvas answers 401 both for a bad token ("Invalid access
/// token", "Expired access token", `{"status":"unauthenticated"}`) and, in some versions, for
/// resources the student may not see (`{"status":"unauthorized"}`, "user not authorized to
/// perform that action"). Only the former aborts a sync; the body decides, because the
/// `WWW-Authenticate` header may be present on both.
fn classify_failure(status: StatusCode, body: &[u8]) -> CanvasError {
    let body = String::from_utf8_lossy(body).to_ascii_lowercase();
    match status {
        StatusCode::UNAUTHORIZED => {
            let token_problem = body.contains("access token") || body.contains("unauthenticated");
            let resource_denied =
                body.contains("\"unauthorized\"") || body.contains("not authorized");
            if resource_denied && !token_problem {
                CanvasError::Forbidden
            } else {
                CanvasError::Unauthorized
            }
        }
        StatusCode::FORBIDDEN => CanvasError::Forbidden,
        StatusCode::NOT_FOUND => CanvasError::NotFound,
        status if status.is_redirection() => {
            CanvasError::BadResponse("unexpected redirect from the API".into())
        }
        status => CanvasError::Http(status.as_u16()),
    }
}

/// https anywhere (without user info). Plain http only on the Canvas origin itself, or on
/// loopback when Canvas itself is a loopback http server (tests) — `normalize_base_url`
/// refuses http for any real Canvas, so production downloads are https-only.
fn allowed_download_url(url: &Url, base: &Url) -> bool {
    let loopback = |u: &Url| matches!(u.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    match url.scheme() {
        "https" => url.username().is_empty() && url.password().is_none(),
        "http" => {
            same_origin(url, base) || (base.scheme() == "http" && loopback(base) && loopback(url))
        }
        _ => false,
    }
}

/// reqwest errors without the URL (URLs can carry signed download parameters).
fn network_error(err: reqwest::Error) -> CanvasError {
    CanvasError::Network(err.without_url().to_string())
}
