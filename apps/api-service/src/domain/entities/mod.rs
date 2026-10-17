use crate::domain::errors::CatalogError;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Copy)]
pub struct ModelCapabilities {
    pub supports_streaming: bool,
    pub supports_tools: bool,
    pub supports_vision: bool,
}

impl ModelCapabilities {
    pub fn new(supports_streaming: bool, supports_tools: bool, supports_vision: bool) -> Self {
        Self {
            supports_streaming,
            supports_tools,
            supports_vision,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TokenPrice {
    pub aws_cost_per_1k_input: Decimal,
    pub aws_cost_per_1k_output: Decimal,
    pub price_per_1k_input: Decimal,
    pub price_per_1k_output: Decimal,
}

impl TokenPrice {
    pub fn new(
        aws_cost_per_1k_input: Decimal,
        aws_cost_per_1k_output: Decimal,
        price_per_1k_input: Decimal,
        price_per_1k_output: Decimal,
    ) -> Result<Self, CatalogError> {
        let values = [
            ("aws_cost_per_1k_input", aws_cost_per_1k_input),
            ("aws_cost_per_1k_output", aws_cost_per_1k_output),
            ("price_per_1k_input", price_per_1k_input),
            ("price_per_1k_output", price_per_1k_output),
        ];
        for (field, value) in values {
            if value < Decimal::ZERO {
                return Err(CatalogError::Validation(format!(
                    "{field} must be greater or equal to zero"
                )));
            }
        }
        Ok(Self {
            aws_cost_per_1k_input,
            aws_cost_per_1k_output,
            price_per_1k_input,
            price_per_1k_output,
        })
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct ModelProvider {
    pub id: Uuid,
    pub name: String,
    pub contact_email: Option<String>,
    pub is_active: bool,
}

impl ModelProvider {
    pub fn new(name: String) -> Result<Self, CatalogError> {
        let normalized_name = normalize_provider_name(&name)?;
        Ok(Self {
            id: Uuid::now_v7(),
            name: normalized_name,
            contact_email: None,
            is_active: true,
        })
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct ModelCatalog {
    pub id: Uuid,
    pub provider_id: Uuid,
    pub public_model_name: String,
    pub provider_model_id: String,
    pub family: Option<String>,
    pub context_window: i32,
    pub max_output_tokens: i32,
    pub supports_streaming: bool,
    pub supports_tools: bool,
    pub supports_vision: bool,
    pub is_active: bool,
    pub deprecated_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl ModelCatalog {
    pub fn new(
        provider_id: Uuid,
        public_model_name: String,
        provider_model_id: String,
        family: Option<String>,
        context_window: i32,
        max_output_tokens: i32,
        capabilities: ModelCapabilities,
    ) -> Result<Self, CatalogError> {
        if context_window <= 0 {
            return Err(CatalogError::Validation(
                "context_window must be positive".to_string(),
            ));
        }
        if max_output_tokens <= 0 {
            return Err(CatalogError::Validation(
                "max_output_tokens must be positive".to_string(),
            ));
        }

        let public_model_name = normalize_public_model_name(&public_model_name)?;
        let provider_model_id = normalize_provider_model_id(&provider_model_id)?;
        let family = normalize_family(family)?;

        Ok(Self {
            id: Uuid::now_v7(),
            provider_id,
            public_model_name,
            provider_model_id,
            family,
            context_window,
            max_output_tokens,
            supports_streaming: capabilities.supports_streaming,
            supports_tools: capabilities.supports_tools,
            supports_vision: capabilities.supports_vision,
            is_active: true,
            deprecated_at: None,
            created_at: Utc::now(),
        })
    }

    pub fn set_capabilities(&mut self, capabilities: ModelCapabilities) {
        self.supports_streaming = capabilities.supports_streaming;
        self.supports_tools = capabilities.supports_tools;
        self.supports_vision = capabilities.supports_vision;
    }

    pub fn set_provider_model_id(&mut self, provider_model_id: String) -> Result<(), CatalogError> {
        self.provider_model_id = normalize_provider_model_id(&provider_model_id)?;
        Ok(())
    }

    pub fn deprecate(&mut self, now: DateTime<Utc>) {
        self.is_active = false;
        self.deprecated_at = Some(now);
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct ModelPricing {
    pub id: Uuid,
    pub model_catalog_id: Uuid,
    pub aws_cost_per_1k_input: Decimal,
    pub aws_cost_per_1k_output: Decimal,
    pub price_per_1k_input: Decimal,
    pub price_per_1k_output: Decimal,
    pub effective_from: DateTime<Utc>,
    pub effective_to: Option<DateTime<Utc>>,
    pub created_by_admin_id: Option<Uuid>,
}

#[derive(Debug, Clone, FromRow)]
pub struct ModelMetadata {
    pub id: Uuid,
    pub model_catalog_id: Uuid,
    pub serving_provider_id: Option<Uuid>,
    pub description: Option<String>,
    pub architecture: Option<String>,
    pub parameter_count: Option<i64>,
    pub precision: Option<String>,
    pub quantization: Option<String>,
    pub metadata: Value,
    pub source_url: Option<String>,
    pub source_name: Option<String>,
    pub source_updated_at: Option<DateTime<Utc>>,
    pub metadata_version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ModelMetadataParams {
    pub model_catalog_id: Uuid,
    pub serving_provider_id: Option<Uuid>,
    pub description: Option<String>,
    pub architecture: Option<String>,
    pub parameter_count: Option<i64>,
    pub precision: Option<String>,
    pub quantization: Option<String>,
    pub metadata: Value,
    pub source_url: Option<String>,
    pub source_name: Option<String>,
    pub source_updated_at: Option<DateTime<Utc>>,
    pub metadata_version: i32,
}

impl ModelMetadata {
    pub fn new(params: ModelMetadataParams) -> Result<Self, CatalogError> {
        if !params.metadata.is_object() {
            return Err(CatalogError::Validation(
                "model metadata must be a JSON object".to_string(),
            ));
        }
        if params.parameter_count.is_some_and(|value| value <= 0) {
            return Err(CatalogError::Validation(
                "parameter_count must be positive".to_string(),
            ));
        }
        if params.metadata_version <= 0 {
            return Err(CatalogError::Validation(
                "metadata_version must be positive".to_string(),
            ));
        }
        Ok(Self {
            id: Uuid::now_v7(),
            model_catalog_id: params.model_catalog_id,
            serving_provider_id: params.serving_provider_id,
            description: normalize_optional_text(params.description, "description", 20_000)?,
            architecture: normalize_optional_text(params.architecture, "architecture", 100)?,
            parameter_count: params.parameter_count,
            precision: normalize_optional_text(params.precision, "precision", 30)?,
            quantization: normalize_optional_text(params.quantization, "quantization", 50)?,
            metadata: params.metadata,
            source_url: normalize_optional_text(params.source_url, "source_url", 2_048)?,
            source_name: normalize_optional_text(params.source_name, "source_name", 100)?,
            source_updated_at: params.source_updated_at,
            metadata_version: params.metadata_version,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct ProviderPolicy {
    pub id: Uuid,
    pub provider_id: Uuid,
    pub model_catalog_id: Option<Uuid>,
    pub policy_scope: String,
    pub retention_mode: String,
    pub retention_days: Option<i32>,
    pub training_usage: String,
    pub provider_access: String,
    pub supported_retention_modes: Value,
    pub data_residency: Value,
    pub security_controls: Value,
    pub compliance: Value,
    pub limitations: Value,
    pub privacy_policy_url: Option<String>,
    pub terms_url: Option<String>,
    pub documentation_url: Option<String>,
    pub source_url: Option<String>,
    pub policy_version: String,
    pub effective_from: DateTime<Utc>,
    pub effective_to: Option<DateTime<Utc>>,
    pub is_current: bool,
    pub created_by_admin_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

impl ProviderPolicy {
    pub fn new(
        input: ProviderPolicyInput,
        created_by_admin_id: Option<Uuid>,
    ) -> Result<Self, CatalogError> {
        let ProviderPolicyInput {
            provider_id,
            model_catalog_id,
            retention_mode,
            retention_days,
            training_usage,
            provider_access,
            supported_retention_modes,
            data_residency,
            security_controls,
            compliance,
            limitations,
            privacy_policy_url,
            terms_url,
            documentation_url,
            source_url,
            policy_version,
        } = input;
        let policy_scope = if model_catalog_id.is_some() {
            "model".to_string()
        } else {
            "provider".to_string()
        };
        validate_json_array(&supported_retention_modes, "supported_retention_modes")?;
        validate_json_object(&data_residency, "data_residency")?;
        validate_json_object(&security_controls, "security_controls")?;
        validate_json_array(&compliance, "compliance")?;
        validate_json_array(&limitations, "limitations")?;
        if retention_days.is_some_and(|value| value < 0) {
            return Err(CatalogError::Validation(
                "retention_days cannot be negative".to_string(),
            ));
        }
        let retention_mode = normalize_required_text(retention_mode, "retention_mode", 50)?;
        let training_usage = normalize_required_text(training_usage, "training_usage", 50)?;
        let provider_access = normalize_required_text(provider_access, "provider_access", 50)?;
        let policy_version = normalize_required_text(policy_version, "policy_version", 50)?;
        Ok(Self {
            id: Uuid::now_v7(),
            provider_id,
            model_catalog_id,
            policy_scope,
            retention_mode,
            retention_days,
            training_usage,
            provider_access,
            supported_retention_modes,
            data_residency,
            security_controls,
            compliance,
            limitations,
            privacy_policy_url: normalize_optional_text(
                privacy_policy_url,
                "privacy_policy_url",
                2_048,
            )?,
            terms_url: normalize_optional_text(terms_url, "terms_url", 2_048)?,
            documentation_url: normalize_optional_text(
                documentation_url,
                "documentation_url",
                2_048,
            )?,
            source_url: normalize_optional_text(source_url, "source_url", 2_048)?,
            policy_version,
            effective_from: Utc::now(),
            effective_to: None,
            is_current: true,
            created_by_admin_id,
            created_at: Utc::now(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct ProviderPolicyInput {
    pub provider_id: Uuid,
    pub model_catalog_id: Option<Uuid>,
    pub retention_mode: String,
    pub retention_days: Option<i32>,
    pub training_usage: String,
    pub provider_access: String,
    pub supported_retention_modes: Value,
    pub data_residency: Value,
    pub security_controls: Value,
    pub compliance: Value,
    pub limitations: Value,
    pub privacy_policy_url: Option<String>,
    pub terms_url: Option<String>,
    pub documentation_url: Option<String>,
    pub source_url: Option<String>,
    pub policy_version: String,
}

fn normalize_optional_text(
    value: Option<String>,
    field: &str,
    max_length: usize,
) -> Result<Option<String>, CatalogError> {
    match value {
        None => Ok(None),
        Some(raw) => {
            let normalized = raw.trim().to_string();
            if normalized.is_empty() {
                return Ok(None);
            }
            if normalized.len() > max_length {
                return Err(CatalogError::Validation(format!(
                    "{field} exceeds maximum length of {max_length}"
                )));
            }
            Ok(Some(normalized))
        }
    }
}

fn normalize_required_text(
    value: String,
    field: &str,
    max_length: usize,
) -> Result<String, CatalogError> {
    let normalized = value.trim().to_string();
    if normalized.is_empty() || normalized.len() > max_length {
        return Err(CatalogError::Validation(format!(
            "{field} must contain between 1 and {max_length} characters"
        )));
    }
    Ok(normalized)
}

fn validate_json_array(value: &Value, field: &str) -> Result<(), CatalogError> {
    if !value.is_array() {
        return Err(CatalogError::Validation(format!(
            "{field} must be a JSON array"
        )));
    }
    Ok(())
}

fn validate_json_object(value: &Value, field: &str) -> Result<(), CatalogError> {
    if !value.is_object() {
        return Err(CatalogError::Validation(format!(
            "{field} must be a JSON object"
        )));
    }
    Ok(())
}

impl ModelPricing {
    pub fn new(
        model_catalog_id: Uuid,
        price: TokenPrice,
        created_by_admin_id: Option<Uuid>,
    ) -> Result<Self, CatalogError> {
        Ok(Self {
            id: Uuid::now_v7(),
            model_catalog_id,
            aws_cost_per_1k_input: price.aws_cost_per_1k_input,
            aws_cost_per_1k_output: price.aws_cost_per_1k_output,
            price_per_1k_input: price.price_per_1k_input,
            price_per_1k_output: price.price_per_1k_output,
            effective_from: Utc::now(),
            effective_to: None,
            created_by_admin_id,
        })
    }
}

fn normalize_provider_name(raw: &str) -> Result<String, CatalogError> {
    let normalized = raw.trim().to_lowercase();
    if normalized.len() < 3 || normalized.len() > 50 {
        return Err(CatalogError::Validation(
            "provider name length must be between 3 and 50".to_string(),
        ));
    }
    if !normalized
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return Err(CatalogError::Validation(
            "provider name must contain only [a-z0-9_-]".to_string(),
        ));
    }
    Ok(normalized)
}

fn normalize_public_model_name(raw: &str) -> Result<String, CatalogError> {
    let normalized = raw.trim().to_string();
    if normalized.len() < 3 || normalized.len() > 100 {
        return Err(CatalogError::Validation(
            "public_model_name length must be between 3 and 100".to_string(),
        ));
    }
    if !normalized
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' || c == ':')
    {
        return Err(CatalogError::Validation(
            "public_model_name contains invalid characters".to_string(),
        ));
    }
    Ok(normalized)
}

fn normalize_provider_model_id(raw: &str) -> Result<String, CatalogError> {
    let normalized = raw.trim().to_string();
    if normalized.len() < 3 || normalized.len() > 150 {
        return Err(CatalogError::Validation(
            "provider_model_id length must be between 3 and 150".to_string(),
        ));
    }
    if !normalized
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(CatalogError::Validation(
            "provider_model_id contains invalid characters".to_string(),
        ));
    }
    Ok(normalized)
}

fn normalize_family(raw: Option<String>) -> Result<Option<String>, CatalogError> {
    match raw {
        Some(value) => {
            let normalized = value.trim().to_lowercase();
            if normalized.is_empty() {
                return Ok(None);
            }
            if normalized.len() > 50 {
                return Err(CatalogError::Validation(
                    "family max length is 50".to_string(),
                ));
            }
            if !normalized
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                return Err(CatalogError::Validation(
                    "family contains invalid characters".to_string(),
                ));
            }
            Ok(Some(normalized))
        }
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_name_is_normalized_to_lowercase() {
        let provider =
            ModelProvider::new(" Aws-Bedrock ".to_string()).expect("provider name should be valid");
        assert_eq!(provider.name, "aws-bedrock");
    }

    #[test]
    fn public_model_name_rejects_invalid_chars() {
        let result = ModelCatalog::new(
            Uuid::now_v7(),
            "bad model name with spaces".to_string(),
            "provider-model".to_string(),
            Some("claude".to_string()),
            200_000,
            8_192,
            ModelCapabilities::new(true, true, false),
        );
        assert!(matches!(result, Err(CatalogError::Validation(_))));
    }

    #[test]
    fn token_price_rejects_negative_values() {
        let result = TokenPrice::new(
            Decimal::new(-1, 0),
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
        );
        assert!(matches!(result, Err(CatalogError::Validation(_))));
    }
}
