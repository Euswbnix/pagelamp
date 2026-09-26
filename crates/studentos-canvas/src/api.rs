//! Typed access to the allow-listed endpoints over any `CanvasTransport`, with the pagination
//! rules: `Link: rel="next"` is followed only on the Canvas origin with the first request's
//! path, never twice (loop protection), at most `MAX_PAGES` times and within
//! `MAX_LISTING_BYTES`. Anything that cuts a listing short marks it `truncated`, and a
//! truncated or partly unreadable listing must never be used to delete data.

use std::collections::HashSet;

use serde::de::DeserializeOwned;
use url::Url;

use crate::endpoint::{Endpoint, Next, acceptable_next_link};
use crate::transport::{CanvasError, CanvasTransport};

/// Safety net against endless pagination (100 items per page → 50,000 items).
const MAX_PAGES: usize = 500;
/// Memory budget for one listing (all pages together).
const MAX_LISTING_BYTES: usize = 64 * 1024 * 1024;

/// A fetched list plus what had to be skipped (malformed items, pagination cut short).
#[derive(Debug)]
pub(crate) struct Listing<T> {
    pub items: Vec<T>,
    /// Items that could not be read (unexpected JSON) and were skipped.
    pub skipped: usize,
    /// Pagination stopped early (foreign/looping next link or page limit): the listing may
    /// be incomplete, so nothing must be pruned based on it.
    pub truncated: bool,
}

impl<T> Listing<T> {
    /// Every page was read and every item understood: safe to prune what's missing.
    pub(crate) fn complete(&self) -> bool {
        !self.truncated && self.skipped == 0
    }
}

pub(crate) struct Api<T> {
    pub(crate) transport: T,
    pub(crate) base: Url,
}

impl<T: CanvasTransport> Api<T> {
    pub(crate) fn new(transport: T, base: Url) -> Self {
        Api { transport, base }
    }

    /// One object (e.g. `/users/self`, a file, a page).
    pub(crate) async fn get_one<D: DeserializeOwned>(
        &self,
        endpoint: Endpoint<'_>,
    ) -> Result<D, CanvasError> {
        let page = self.transport.get_json(endpoint.url(&self.base)).await?;
        serde_json::from_value(page.body)
            .map_err(|_| CanvasError::BadResponse("unexpected JSON shape".into()))
    }

    /// Every item of a paginated list.
    pub(crate) async fn get_all<D: DeserializeOwned>(
        &self,
        endpoint: Endpoint<'_>,
    ) -> Result<Listing<D>, CanvasError> {
        let mut listing = Listing {
            items: Vec::new(),
            skipped: 0,
            truncated: false,
        };
        let first = endpoint.url(&self.base);
        let mut url = first.clone();
        let mut seen = HashSet::new();
        let mut bytes = 0usize;
        for _ in 0..MAX_PAGES {
            seen.insert(url.to_string());
            let page = self.transport.get_json(url).await?;
            bytes += page.bytes;
            let serde_json::Value::Array(values) = page.body else {
                return Err(CanvasError::BadResponse("expected a list".into()));
            };
            for value in values {
                match serde_json::from_value(value) {
                    Ok(item) => listing.items.push(item),
                    Err(_) => listing.skipped += 1,
                }
            }
            match page.next {
                Next::End => return Ok(listing),
                Next::Link(next)
                    if acceptable_next_link(&first, &next)
                        && !seen.contains(next.as_str())
                        && bytes < MAX_LISTING_BYTES =>
                {
                    url = next;
                }
                Next::Link(_) | Next::Unusable => {
                    listing.truncated = true;
                    return Ok(listing);
                }
            }
        }
        listing.truncated = true;
        Ok(listing)
    }
}
