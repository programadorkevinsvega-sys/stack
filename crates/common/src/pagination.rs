use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Paginación estándar para listados. Usada por todos los repositorios.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Pagination {
    pub limit: u32,
    pub offset: u32,
}

impl Pagination {
    pub fn new(limit: u32, offset: u32) -> Self {
        Self {
            limit: limit.min(Self::MAX_LIMIT),
            offset: offset.min(Self::MAX_OFFSET),
        }
    }

    pub const MAX_LIMIT: u32 = 100;
    pub const MAX_OFFSET: u32 = 10_000;

    pub const DEFAULT_LIMIT: u32 = 20;
}

impl Default for Pagination {
    fn default() -> Self {
        Self {
            limit: Self::DEFAULT_LIMIT,
            offset: 0,
        }
    }
}

/// Paginación por cursor (keyset/seek) para tablas grandes.
/// Más eficiente que offset para datasets que crecen.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KeysetPagination {
    pub limit: u32,
    pub after: Option<String>,
}

impl KeysetPagination {
    pub fn new(limit: u32, after: Option<String>) -> Self {
        Self {
            limit: limit.min(Pagination::MAX_LIMIT),
            after,
        }
    }

    pub fn default_limit() -> u32 {
        Pagination::DEFAULT_LIMIT
    }

    /// Encode a (created_at, id) pair into an opaque cursor string.
    /// The cursor is a JSON object: `{"t":"<ISO8601>","i":"<uuid>"}`
    pub fn encode_cursor(created_at: DateTime<Utc>, id: Uuid) -> String {
        #[derive(serde::Serialize)]
        struct Cursor {
            t: String,
            i: String,
        }
        let c = Cursor {
            t: created_at.to_rfc3339(),
            i: id.to_string(),
        };
        serde_json::to_string(&c).expect("cursor serialization should not fail")
    }

    /// Decode an opaque cursor string into a (created_at, id) pair.
    /// Returns `None` for invalid cursor format.
    pub fn decode_cursor(cursor: &str) -> Option<(DateTime<Utc>, Uuid)> {
        #[derive(serde::Deserialize)]
        struct Cursor {
            t: String,
            i: String,
        }
        let c: Cursor = serde_json::from_str(cursor).ok()?;
        let created_at = DateTime::parse_from_rfc3339(&c.t).ok()?.with_timezone(&Utc);
        let id = Uuid::parse_str(&c.i).ok()?;
        Some((created_at, id))
    }
}

impl Default for KeysetPagination {
    fn default() -> Self {
        Self {
            limit: Self::default_limit(),
            after: None,
        }
    }
}
