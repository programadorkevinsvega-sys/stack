use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AwsAccountModel {
    pub account_id: Uuid,
    pub model_catalog_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MantleConnection {
    pub account_id: Uuid,
    pub model_catalog_id: Uuid,
    pub api_key_encrypted: Option<Vec<u8>>,
    pub api_key_kms_key_id: Option<String>,
    pub base_url: Option<String>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}
