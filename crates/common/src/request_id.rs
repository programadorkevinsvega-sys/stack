use uuid::Uuid;

/// Identificador único de request propagado end-to-end desde api-gateway
/// a través de gRPC metadata, logs, y eventos NATS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct RequestId(pub Uuid);

impl RequestId {
    /// Crea un nuevo RequestId con UUID v7 (ordenable por tiempo).
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for RequestId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<Uuid> for RequestId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl From<RequestId> for Uuid {
    fn from(rid: RequestId) -> Self {
        rid.0
    }
}
