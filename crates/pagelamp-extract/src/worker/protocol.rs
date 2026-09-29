//! The worker's wire format: one JSON request on stdin, one JSON response on stdout.

use serde::{Deserialize, Serialize};

use crate::Segment;

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Request {
    pub protocol: u32,
    pub path: String,
    pub mime: Option<String>,
    pub memory_bytes: u64,
    pub cpu_seconds: u64,
    /// Test builds only: make the worker misbehave on purpose (`child::run_fault`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debug_fault: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Response {
    pub protocol: u32,
    pub outcome: Outcome,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub(crate) enum Outcome {
    Ok {
        segments: Vec<WireSegment>,
    },
    Unsupported {
        message: String,
    },
    Failed {
        message: String,
    },
    Io {
        message: String,
    },
    /// The request's protocol is not this worker's.
    ProtocolMismatch,
    /// The request could not be read.
    BadRequest,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct WireSegment {
    pub locator: Option<String>,
    pub text: String,
}

impl From<Segment> for WireSegment {
    fn from(segment: Segment) -> Self {
        WireSegment {
            locator: segment.locator,
            text: segment.text,
        }
    }
}

impl From<WireSegment> for Segment {
    fn from(segment: WireSegment) -> Self {
        Segment {
            locator: segment.locator,
            text: segment.text,
        }
    }
}
