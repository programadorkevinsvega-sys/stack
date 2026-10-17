use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccountCandidate {
    pub account_id: Uuid,
    pub aws_account_id: String,
    pub region: String,
    pub max_tpm: i64,
    pub max_rpm: i64,
    pub priority_weight: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SelectedAccount {
    pub account_id: Uuid,
    pub aws_account_id: String,
    pub region: String,
    pub provider_model_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccountLoad {
    pub account_id: Uuid,
    pub active_connections: i64,
    pub current_tpm: i64,
    pub current_rpm: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CircuitStatus {
    pub account_id: Uuid,
    pub state: String,
    pub open_until_epoch_seconds: i64,
    pub consecutive_failures: i64,
}
