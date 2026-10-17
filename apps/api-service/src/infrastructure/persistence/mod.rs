use crate::application::ports::{AuditEntry, AuditLogger};
use crate::domain::entities::{
    ModelCatalog, ModelMetadata, ModelPricing, ModelProvider, ProviderPolicy,
};
use crate::domain::errors::CatalogError;
use crate::domain::repositories::{
    ModelCatalogRepository, ModelMetadataRepository, ModelPricingRepository,
    ModelProviderRepository, ModelQueryFilter, ProviderPolicyRepository,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use common::KeysetPagination;
use sqlx::postgres::PgPool;
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

// ── Error mapping ─────────────────────────────────────────────────────

fn map_sqlx_error(context: &str, error: sqlx::Error) -> CatalogError {
    match &error {
        sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed | sqlx::Error::Io(_) => {
            CatalogError::DependencyUnavailable(format!("{context}: database unavailable"))
        }
        sqlx::Error::Database(db) => match db.code().as_deref() {
            Some("23505") => CatalogError::Validation(format!(
                "{context}: unique constraint violation ({})",
                db.message()
            )),
            Some("23503") => CatalogError::Validation(format!(
                "{context}: foreign key violation ({})",
                db.message()
            )),
            Some("23514") => CatalogError::Validation(format!(
                "{context}: check constraint violation ({})",
                db.message()
            )),
            Some("42P01") => CatalogError::Configuration(format!(
                "{context}: required relation does not exist ({})",
                db.message()
            )),
            _ => CatalogError::Internal(format!("{context}: database error ({})", db.message())),
        },
        _ => CatalogError::Internal(format!("{context}: {error}")),
    }
}

// ── Audit logger ──────────────────────────────────────────────────────

pub struct PgAuditLogger {
    pub pool: PgPool,
    pub signer: common::AuditSigner,
}

#[async_trait]
impl AuditLogger for PgAuditLogger {
    async fn record(&self, entry: AuditEntry) -> Result<(), CatalogError> {
        let metadata_str = serde_json::to_string(&entry.metadata)
            .map_err(|e| CatalogError::Internal(format!("metadata serialization: {e}")))?;
        let sig = self
            .signer
            .sign(
                entry.actor_type.as_str(),
                entry.actor_id,
                entry.action.as_str(),
                entry.resource_type.as_str(),
                entry.resource_id,
                Some(&metadata_str),
            )
            .map_err(CatalogError::Internal)?;
        sqlx::query(
            r#"
            INSERT INTO audit_logs
                (id, actor_type, actor_id, action, resource_type, resource_id, metadata, ip_address, signature, created_at)
            VALUES
                ($1, $2, $3, $4, $5, $6, $7, CAST($8 AS inet), $9, now())
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(entry.actor_type.as_str())
        .bind(entry.actor_id)
        .bind(entry.action.as_str())
        .bind(entry.resource_type.as_str())
        .bind(entry.resource_id)
        .bind(entry.metadata)
        .bind(entry.ip_address.map(|ip| ip.to_string()))
        .bind(sig)
        .execute(&self.pool)
        .await
        .map_err(|e| map_sqlx_error("audit.record", e))?;
        Ok(())
    }
}

// ── ModelProvider ─────────────────────────────────────────────────────

pub struct PgModelProviderRepository {
    pub pool: PgPool,
}

#[async_trait]
impl ModelProviderRepository for PgModelProviderRepository {
    async fn save(&self, provider: &ModelProvider) -> Result<(), CatalogError> {
        sqlx::query(
            r#"
            INSERT INTO model_providers (id, name, contact_email, is_active)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(provider.id)
        .bind(&provider.name)
        .bind(&provider.contact_email)
        .bind(provider.is_active)
        .execute(&self.pool)
        .await
        .map_err(|e| map_sqlx_error("model_provider.save", e))?;
        Ok(())
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<ModelProvider>, CatalogError> {
        sqlx::query_as::<_, ModelProvider>(
            r#"
            SELECT id, name, contact_email, is_active
            FROM model_providers
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| map_sqlx_error("model_provider.find_by_id", e))
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<ModelProvider>, CatalogError> {
        sqlx::query_as::<_, ModelProvider>(
            r#"
            SELECT id, name, contact_email, is_active
            FROM model_providers
            WHERE name = $1
            "#,
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| map_sqlx_error("model_provider.find_by_name", e))
    }

    async fn find_all_active(&self) -> Result<Vec<ModelProvider>, CatalogError> {
        sqlx::query_as::<_, ModelProvider>(
            r#"
            SELECT id, name, contact_email, is_active
            FROM model_providers
            WHERE is_active = true
            ORDER BY name ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| map_sqlx_error("model_provider.find_all_active", e))
    }
}

// ── ModelCatalog ───────────────────────────────────────────────────────

pub struct PgModelCatalogRepository {
    pub pool: PgPool,
}

#[async_trait]
impl ModelCatalogRepository for PgModelCatalogRepository {
    async fn save(&self, model: &ModelCatalog) -> Result<(), CatalogError> {
        sqlx::query!(
            r#"
            INSERT INTO model_catalog
                (id, provider_id, public_model_name, provider_model_id, family, context_window,
                 max_output_tokens, supports_streaming, supports_tools, supports_vision,
                 is_active, deprecated_at, created_at)
            VALUES
                ($1, $2, $3, $4, $5, $6,
                 $7, $8, $9, $10,
                 $11, $12, $13)
            "#,
            model.id,
            model.provider_id,
            &model.public_model_name,
            &model.provider_model_id,
            model.family.as_deref(),
            model.context_window,
            model.max_output_tokens,
            model.supports_streaming,
            model.supports_tools,
            model.supports_vision,
            model.is_active,
            model.deprecated_at,
            model.created_at,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| map_sqlx_error("model_catalog.save", e))?;
        Ok(())
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<ModelCatalog>, CatalogError> {
        sqlx::query_as::<_, ModelCatalog>(
            r#"
            SELECT id, provider_id, public_model_name, provider_model_id, family, context_window,
                   max_output_tokens, supports_streaming, supports_tools, supports_vision,
                   is_active, deprecated_at, created_at
            FROM model_catalog
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| map_sqlx_error("model_catalog.find_by_id", e))
    }

    async fn find_by_public_name(&self, name: &str) -> Result<Option<ModelCatalog>, CatalogError> {
        sqlx::query_as::<_, ModelCatalog>(
            r#"
            SELECT id, provider_id, public_model_name, provider_model_id, family, context_window,
                   max_output_tokens, supports_streaming, supports_tools, supports_vision,
                   is_active, deprecated_at, created_at
            FROM model_catalog
            WHERE public_model_name = $1
            "#,
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| map_sqlx_error("model_catalog.find_by_public_name", e))
    }

    async fn has_plan_access(&self, plan_id: Uuid, model_id: Uuid) -> Result<bool, CatalogError> {
        sqlx::query_scalar!(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM plan_model_access pma
                WHERE pma.plan_id = $1
                  AND pma.model_catalog_id = $2
            ) AS "exists!"
            "#,
            plan_id,
            model_id,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| map_sqlx_error("model_catalog.has_plan_access", e))
    }

    async fn find_filtered(
        &self,
        filter: &ModelQueryFilter,
    ) -> Result<(Vec<ModelCatalog>, Option<String>), CatalogError> {
        let limit = filter.limit.unwrap_or(20);
        if limit == 0 || limit > 500 {
            return Err(CatalogError::Validation(
                "limit must be between 1 and 500".to_string(),
            ));
        }

        let mut qb = QueryBuilder::<Postgres>::new(
            r#"
            SELECT id, provider_id, public_model_name, provider_model_id, family, context_window,
                   max_output_tokens, supports_streaming, supports_tools, supports_vision,
                   is_active, deprecated_at, created_at
            FROM model_catalog
            WHERE 1=1
            "#,
        );

        if let Some(plan_id) = filter.plan_id {
            qb.push(
                " AND EXISTS (
                    SELECT 1
                    FROM plan_model_access pma
                    WHERE pma.model_catalog_id = model_catalog.id
                      AND pma.plan_id = ",
            )
            .push_bind(plan_id)
            .push(")");
        }
        if let Some(provider_id) = filter.provider_id {
            qb.push(" AND provider_id = ").push_bind(provider_id);
        }
        if let Some(family) = &filter.family {
            qb.push(" AND family = ").push_bind(family);
        }
        if let Some(active) = filter.active {
            qb.push(" AND is_active = ").push_bind(active);
        }

        // Keyset pagination: when after_cursor is provided, use WHERE (created_at, id) < cursor
        if let Some(ref cursor) = filter.after_cursor {
            if let Some((cursor_time, cursor_id)) = KeysetPagination::decode_cursor(cursor) {
                qb.push(" AND (created_at, id) < (")
                    .push_bind(cursor_time)
                    .push(", ")
                    .push_bind(cursor_id)
                    .push(")");
            }
        }

        qb.push(" ORDER BY created_at DESC, id DESC ");
        qb.push(" LIMIT ").push_bind(i64::from(limit + 1));

        let results = qb
            .build_query_as::<ModelCatalog>()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| map_sqlx_error("model_catalog.find_filtered", e))?;

        let has_more = results.len() > limit as usize;
        let models = if has_more {
            &results[..limit as usize]
        } else {
            &results
        };

        let next_cursor = models
            .last()
            .map(|m| KeysetPagination::encode_cursor(m.created_at, m.id));

        Ok((models.to_vec(), if has_more { next_cursor } else { None }))
    }

    async fn find_all_active(&self) -> Result<Vec<ModelCatalog>, CatalogError> {
        self.find_filtered(&ModelQueryFilter {
            plan_id: None,
            provider_id: None,
            family: None,
            active: Some(true),
            limit: Some(500),
            after_cursor: None,
        })
        .await
        .map(|(models, _)| models)
    }

    async fn update(&self, model: &ModelCatalog) -> Result<(), CatalogError> {
        sqlx::query!(
            r#"
            UPDATE model_catalog
            SET supports_streaming = $2,
                supports_tools = $3,
                supports_vision = $4,
                is_active = $5,
                deprecated_at = $6
            WHERE id = $1
            "#,
            model.id,
            model.supports_streaming,
            model.supports_tools,
            model.supports_vision,
            model.is_active,
            model.deprecated_at,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| map_sqlx_error("model_catalog.update", e))?;
        Ok(())
    }
}

// ── ModelPricing ───────────────────────────────────────────────────────

pub struct PgModelPricingRepository {
    pub pool: PgPool,
}

#[async_trait]
impl ModelPricingRepository for PgModelPricingRepository {
    async fn save(&self, pricing: &ModelPricing) -> Result<(), CatalogError> {
        sqlx::query!(
            r#"
            INSERT INTO model_pricing
                (id, model_catalog_id, aws_cost_per_1k_input, aws_cost_per_1k_output,
                 price_per_1k_input, price_per_1k_output, effective_from, effective_to, created_by_admin_id)
            VALUES
                ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
            pricing.id,
            pricing.model_catalog_id,
            pricing.aws_cost_per_1k_input,
            pricing.aws_cost_per_1k_output,
            pricing.price_per_1k_input,
            pricing.price_per_1k_output,
            pricing.effective_from,
            pricing.effective_to,
            pricing.created_by_admin_id,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| map_sqlx_error("model_pricing.save", e))?;
        Ok(())
    }

    async fn replace_active_pricing(&self, pricing: &ModelPricing) -> Result<(), CatalogError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| map_sqlx_error("model_pricing.begin_tx", e))?;

        sqlx::query!(
            r#"
            UPDATE model_pricing
            SET effective_to = now()
            WHERE model_catalog_id = $1
              AND effective_to IS NULL
            "#,
            pricing.model_catalog_id,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error("model_pricing.close_active", e))?;

        sqlx::query!(
            r#"
            INSERT INTO model_pricing
                (id, model_catalog_id, aws_cost_per_1k_input, aws_cost_per_1k_output,
                 price_per_1k_input, price_per_1k_output, effective_from, effective_to, created_by_admin_id)
            VALUES
                ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
            pricing.id,
            pricing.model_catalog_id,
            pricing.aws_cost_per_1k_input,
            pricing.aws_cost_per_1k_output,
            pricing.price_per_1k_input,
            pricing.price_per_1k_output,
            pricing.effective_from,
            pricing.effective_to,
            pricing.created_by_admin_id,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| map_sqlx_error("model_pricing.insert_new", e))?;

        tx.commit()
            .await
            .map_err(|e| map_sqlx_error("model_pricing.commit_tx", e))?;

        Ok(())
    }

    async fn close_active_pricing(&self, model_catalog_id: Uuid) -> Result<(), CatalogError> {
        sqlx::query!(
            r#"
            UPDATE model_pricing
            SET effective_to = now()
            WHERE model_catalog_id = $1
              AND effective_to IS NULL
            "#,
            model_catalog_id,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| map_sqlx_error("model_pricing.close_active_pricing", e))?;
        Ok(())
    }

    async fn find_active_by_model(
        &self,
        model_catalog_id: Uuid,
    ) -> Result<Option<ModelPricing>, CatalogError> {
        sqlx::query_as::<_, ModelPricing>(
            r#"
            SELECT id, model_catalog_id, aws_cost_per_1k_input, aws_cost_per_1k_output,
                   price_per_1k_input, price_per_1k_output, effective_from, effective_to, created_by_admin_id
            FROM model_pricing
            WHERE model_catalog_id = $1
              AND effective_to IS NULL
            "#,
        )
        .bind(model_catalog_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| map_sqlx_error("model_pricing.find_active_by_model", e))
    }

    async fn find_at(
        &self,
        model_catalog_id: Uuid,
        timestamp: DateTime<Utc>,
    ) -> Result<Option<ModelPricing>, CatalogError> {
        sqlx::query_as::<_, ModelPricing>(
            r#"
            SELECT id, model_catalog_id, aws_cost_per_1k_input, aws_cost_per_1k_output,
                   price_per_1k_input, price_per_1k_output, effective_from, effective_to, created_by_admin_id
            FROM model_pricing
            WHERE model_catalog_id = $1
              AND effective_from <= $2
              AND (effective_to IS NULL OR effective_to > $2)
            ORDER BY effective_from DESC
            LIMIT 1
            "#,
        )
        .bind(model_catalog_id)
        .bind(timestamp)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| map_sqlx_error("model_pricing.find_at", e))
    }

    async fn find_history(
        &self,
        model_catalog_id: Uuid,
    ) -> Result<Vec<ModelPricing>, CatalogError> {
        sqlx::query_as::<_, ModelPricing>(
            r#"
            SELECT id, model_catalog_id, aws_cost_per_1k_input, aws_cost_per_1k_output,
                   price_per_1k_input, price_per_1k_output, effective_from, effective_to, created_by_admin_id
            FROM model_pricing
            WHERE model_catalog_id = $1
            ORDER BY effective_from DESC
            "#,
        )
        .bind(model_catalog_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| map_sqlx_error("model_pricing.find_history", e))
    }
}

pub struct PgModelMetadataRepository {
    pub pool: PgPool,
}

#[async_trait]
impl ModelMetadataRepository for PgModelMetadataRepository {
    async fn save_or_replace(&self, metadata: &ModelMetadata) -> Result<(), CatalogError> {
        sqlx::query(
            r#"
            INSERT INTO model_metadata
                (id, model_catalog_id, serving_provider_id, description, architecture, parameter_count,
                 precision, quantization, metadata, source_url, source_name,
                 source_updated_at, metadata_version, created_at, updated_at)
            VALUES
                ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
            ON CONFLICT (model_catalog_id) DO UPDATE SET
                description = EXCLUDED.description,
                serving_provider_id = EXCLUDED.serving_provider_id,
                architecture = EXCLUDED.architecture,
                parameter_count = EXCLUDED.parameter_count,
                precision = EXCLUDED.precision,
                quantization = EXCLUDED.quantization,
                metadata = EXCLUDED.metadata,
                source_url = EXCLUDED.source_url,
                source_name = EXCLUDED.source_name,
                source_updated_at = EXCLUDED.source_updated_at,
                metadata_version = EXCLUDED.metadata_version,
                updated_at = now()
            "#,
        )
        .bind(metadata.id)
        .bind(metadata.model_catalog_id)
        .bind(metadata.serving_provider_id)
        .bind(&metadata.description)
        .bind(&metadata.architecture)
        .bind(metadata.parameter_count)
        .bind(&metadata.precision)
        .bind(&metadata.quantization)
        .bind(&metadata.metadata)
        .bind(&metadata.source_url)
        .bind(&metadata.source_name)
        .bind(metadata.source_updated_at)
        .bind(metadata.metadata_version)
        .bind(metadata.created_at)
        .bind(metadata.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| map_sqlx_error("model_metadata.save_or_replace", e))?;
        Ok(())
    }

    async fn find_by_model(
        &self,
        model_catalog_id: Uuid,
    ) -> Result<Option<ModelMetadata>, CatalogError> {
        sqlx::query_as::<_, ModelMetadata>(
            r#"
            SELECT id, model_catalog_id, serving_provider_id, description, architecture, parameter_count,
                   precision, quantization, metadata, source_url, source_name,
                   source_updated_at, metadata_version, created_at, updated_at
            FROM model_metadata
            WHERE model_catalog_id = $1
            "#,
        )
        .bind(model_catalog_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| map_sqlx_error("model_metadata.find_by_model", e))
    }
}

pub struct PgProviderPolicyRepository {
    pub pool: PgPool,
}

#[async_trait]
impl ProviderPolicyRepository for PgProviderPolicyRepository {
    async fn replace_current(&self, policy: &ProviderPolicy) -> Result<(), CatalogError> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|e| map_sqlx_error("provider_policy.begin_tx", e))?;

        sqlx::query(
            r#"
            UPDATE provider_policies
            SET is_current = false, effective_to = $3
            WHERE provider_id = $1
              AND model_catalog_id IS NOT DISTINCT FROM $2
              AND is_current = true
            "#,
        )
        .bind(policy.provider_id)
        .bind(policy.model_catalog_id)
        .bind(policy.effective_from)
        .execute(&mut *transaction)
        .await
        .map_err(|e| map_sqlx_error("provider_policy.close_current", e))?;

        sqlx::query(
            r#"
            INSERT INTO provider_policies
                (id, provider_id, model_catalog_id, policy_scope, retention_mode,
                 retention_days, training_usage, provider_access,
                 supported_retention_modes, data_residency, security_controls,
                 compliance, limitations, privacy_policy_url, terms_url,
                 documentation_url, source_url, policy_version, effective_from,
                 effective_to, is_current, created_by_admin_id, created_at)
            VALUES
                ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
                 $14, $15, $16, $17, $18, $19, $20, $21, $22, $23)
            "#,
        )
        .bind(policy.id)
        .bind(policy.provider_id)
        .bind(policy.model_catalog_id)
        .bind(&policy.policy_scope)
        .bind(&policy.retention_mode)
        .bind(policy.retention_days)
        .bind(&policy.training_usage)
        .bind(&policy.provider_access)
        .bind(&policy.supported_retention_modes)
        .bind(&policy.data_residency)
        .bind(&policy.security_controls)
        .bind(&policy.compliance)
        .bind(&policy.limitations)
        .bind(&policy.privacy_policy_url)
        .bind(&policy.terms_url)
        .bind(&policy.documentation_url)
        .bind(&policy.source_url)
        .bind(&policy.policy_version)
        .bind(policy.effective_from)
        .bind(policy.effective_to)
        .bind(policy.is_current)
        .bind(policy.created_by_admin_id)
        .bind(policy.created_at)
        .execute(&mut *transaction)
        .await
        .map_err(|e| map_sqlx_error("provider_policy.insert", e))?;

        transaction
            .commit()
            .await
            .map_err(|e| map_sqlx_error("provider_policy.commit_tx", e))?;
        Ok(())
    }

    async fn find_current_for_provider(
        &self,
        provider_id: Uuid,
    ) -> Result<Option<ProviderPolicy>, CatalogError> {
        self.find_current(provider_id, None, "provider_policy.find_current_provider")
            .await
    }

    async fn find_current_for_model(
        &self,
        provider_id: Uuid,
        model_catalog_id: Uuid,
    ) -> Result<Option<ProviderPolicy>, CatalogError> {
        self.find_current(
            provider_id,
            Some(model_catalog_id),
            "provider_policy.find_current_model",
        )
        .await
    }
}

impl PgProviderPolicyRepository {
    async fn find_current(
        &self,
        provider_id: Uuid,
        model_catalog_id: Option<Uuid>,
        context: &str,
    ) -> Result<Option<ProviderPolicy>, CatalogError> {
        sqlx::query_as::<_, ProviderPolicy>(
            r#"
            SELECT id, provider_id, model_catalog_id, policy_scope, retention_mode,
                   retention_days, training_usage, provider_access,
                   supported_retention_modes, data_residency, security_controls,
                   compliance, limitations, privacy_policy_url, terms_url,
                   documentation_url, source_url, policy_version, effective_from,
                   effective_to, is_current, created_by_admin_id, created_at
            FROM provider_policies
            WHERE provider_id = $1
              AND model_catalog_id IS NOT DISTINCT FROM $2
              AND is_current = true
            ORDER BY effective_from DESC
            LIMIT 1
            "#,
        )
        .bind(provider_id)
        .bind(model_catalog_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| map_sqlx_error(context, e))
    }
}
