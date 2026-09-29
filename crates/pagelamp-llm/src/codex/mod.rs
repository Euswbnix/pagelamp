//! Mode A: the student's ChatGPT plan through the official, unmodified Codex CLI (design §2.3,
//! plan M2). PageLamp never touches ChatGPT credentials: Codex signs in and runs on its own, in a
//! dedicated `CODEX_HOME`, with every tool off.
//!
//! - `pin`: the pinned version, its models and the verified asset per target.
//! - `runtime`: download, verify and install that asset (at most two versions kept).

pub mod pin;
pub mod runtime;

pub use pin::{Pin, PinAsset, Version, pin, running_target};
pub use runtime::{InstallError, InstallEvent, Installed, Runtime};
