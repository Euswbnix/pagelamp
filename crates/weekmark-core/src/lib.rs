//! Weekmark core: the local, read-mostly course knowledge base.
//!
//! Layering (see `docs/ARCHITECTURE.md`):
//! - `model`    — plain data types shared by every crate (also the MCP tool output types).
//! - `store`    — the SQLite store. The CLI/sync process is the only writer of synced data;
//!   MCP server processes open it read-only (plus one tiny write path for study plans).
//! - `ingest`   — sync-time extract → chunk → FTS index (heavy work never runs in MCP calls).
//! - `timeline` — pure functions that infer "which week is this course in" with evidence.
//! - `views`    — read views shared by the App facade and the MCP server.
//! - `diagnostics` — local log files, redaction, crash capture (logs never leave the device).
//! - `brand`    — product naming for user-facing text (edited by distributions).
//! - `paths`    — where data lives on disk (`WEEKMARK_HOME` overrides everything).
//! - `source`   — error type + progress callback shared by the sync sources.
//! - `secrets`  — OS keychain access for Canvas tokens / calendar-feed URLs. Never used by MCP.

pub mod brand;
pub mod diagnostics;
pub mod error;
pub mod ingest;
pub mod model;
pub mod paths;
pub mod secrets;
pub mod source;
pub mod store;
pub mod timeline;
pub mod views;

pub use error::{Error, Result};
pub use store::Store;
