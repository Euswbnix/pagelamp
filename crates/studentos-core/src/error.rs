use thiserror::Error;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The database was created by a newer StudentOS than this binary understands.
    #[error(
        "database schema version {found} is newer than supported version {supported}; please update StudentOS"
    )]
    SchemaTooNew { found: i64, supported: i64 },

    /// The database has not been initialised yet (read-only open of a missing/empty DB).
    #[error("StudentOS has no data yet at {0}; run `studentos sync` first")]
    NotInitialised(String),

    #[error("not found: {0}")]
    NotFound(String),

    /// A course query matched more than one course.
    #[error("'{query}' matches several courses: {candidates:?}")]
    Ambiguous {
        query: String,
        candidates: Vec<String>,
    },

    #[error("invalid input: {0}")]
    Invalid(String),

    #[error("keychain error: {0}")]
    Secret(String),
}
