use crate::domain::entities::{AccountLoad, CircuitStatus, SelectedAccount};
use crate::domain::errors::ModelRouterError;
use crate::infrastructure::clients::{CatalogResolver, CredentialResolver};
use crate::infrastructure::events::RouterEventPublisher;
use crate::infrastructure::redis_state::RedisRouterState;
use events::{AccountMarkedUnhealthy, AccountRecovered};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct ModelRouterConfig {
    pub circuit_failure_threshold: i64,
    pub circuit_cooldown_seconds: i64,
}

#[derive(Clone)]
pub struct ModelRouterApplication {
    pub credentials: Arc<dyn CredentialResolver>,
    pub catalog: Arc<dyn CatalogResolver>,
    pub state: Arc<RedisRouterState>,
    pub events: Arc<RouterEventPublisher>,
    pub config: ModelRouterConfig,
}

impl ModelRouterApplication {
    pub async fn select_account(
        &self,
        model_catalog_id: Uuid,
        request_id: Uuid,
        estimated_tokens: i64,
    ) -> Result<Option<SelectedAccount>, ModelRouterError> {
        let provider_model_id = self.catalog.provider_model_id(model_catalog_id).await?;
        let candidates = self
            .credentials
            .list_healthy_accounts_for_model(model_catalog_id)
            .await?;
        if candidates.is_empty() {
            return Ok(None);
        }

        let mut best = None;
        let mut best_score = f64::MAX;
        for candidate in candidates {
            if self.state.is_circuit_open(candidate.account_id).await? {
                continue;
            }
            let load = self.state.get_load(candidate.account_id).await?;
            let weight = candidate.priority_weight.max(1) as f64;
            let active_score = load.active_connections as f64 / weight;
            let rpm_score = load.current_rpm as f64 / candidate.max_rpm.max(1) as f64;
            let tpm_score = load.current_tpm as f64 / candidate.max_tpm.max(1) as f64;
            let score = active_score + rpm_score + tpm_score;
            if score < best_score {
                best_score = score;
                best = Some(candidate);
            }
        }

        let Some(candidate) = best else {
            return Err(ModelRouterError::AllAccountsCircuitOpen);
        };

        self.state
            .record_selection(candidate.account_id, request_id, estimated_tokens.max(0))
            .await?;
        Ok(Some(SelectedAccount {
            account_id: candidate.account_id,
            aws_account_id: candidate.aws_account_id,
            region: candidate.region,
            provider_model_id,
        }))
    }

    pub async fn report_outcome(
        &self,
        account_id: Uuid,
        request_id: Uuid,
        success: bool,
        error_code: Option<String>,
        input_tokens: i64,
        output_tokens: i64,
    ) -> Result<(), ModelRouterError> {
        self.state
            .record_outcome(
                account_id,
                request_id,
                input_tokens.max(0) + output_tokens.max(0),
            )
            .await?;
        if success {
            let was_open = self.state.force_close_circuit(account_id).await?;
            if was_open {
                self.events
                    .publish_account_recovered(&AccountRecovered {
                        account_id,
                        aws_account_id: account_id.to_string(),
                    })
                    .await?;
            }
            return Ok(());
        }

        let failures = self.state.record_failure(account_id).await?;
        if failures >= self.config.circuit_failure_threshold {
            let opened = self
                .state
                .open_circuit(account_id, self.config.circuit_cooldown_seconds)
                .await?;
            if opened {
                self.events
                    .publish_account_marked_unhealthy(&AccountMarkedUnhealthy {
                        account_id,
                        aws_account_id: account_id.to_string(),
                        error_code: error_code.unwrap_or_else(|| "upstream_error".into()),
                        error_message: "model-router circuit opened".into(),
                    })
                    .await?;
            }
        }
        Ok(())
    }

    pub async fn reset_circuit(&self, account_id: Uuid) -> Result<bool, ModelRouterError> {
        self.state.force_close_circuit(account_id).await
    }

    pub async fn circuit_status(
        &self,
        account_id: Uuid,
    ) -> Result<CircuitStatus, ModelRouterError> {
        self.state.circuit_status(account_id).await
    }

    pub async fn get_load(&self, account_id: Uuid) -> Result<AccountLoad, ModelRouterError> {
        self.state.get_load(account_id).await
    }

    pub async fn list_loads(
        &self,
        model_catalog_id: Uuid,
    ) -> Result<Vec<AccountLoad>, ModelRouterError> {
        let candidates = self
            .credentials
            .list_healthy_accounts_for_model(model_catalog_id)
            .await?;
        let mut loads = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            loads.push(self.state.get_load(candidate.account_id).await?);
        }
        Ok(loads)
    }
}
