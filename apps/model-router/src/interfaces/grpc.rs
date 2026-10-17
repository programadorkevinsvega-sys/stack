use crate::application::service::ModelRouterApplication;
use crate::domain::entities::{AccountLoad as DomainAccountLoad, CircuitStatus};
use crate::domain::errors::ModelRouterError;
use common::ServiceJwtClaims;
use proto::account_selection_v1::account_selection_service_server::AccountSelectionService;
use proto::account_selection_v1::{
    AccountLoad, GetAccountLoadRequest, GetAccountLoadResponse, GetCircuitStatusRequest,
    GetCircuitStatusResponse, ListAccountLoadsRequest, ListAccountLoadsResponse,
    ReportOutcomeRequest, ReportOutcomeResponse, ResetCircuitBreakerRequest,
    ResetCircuitBreakerResponse, SelectAccountRequest, SelectAccountResponse,
};
use std::collections::HashSet;
use std::sync::Arc;
use tonic::{Request, Response, Status};
use uuid::Uuid;

#[derive(Clone)]
pub struct ModelRouterGrpcService {
    pub app: Arc<ModelRouterApplication>,
    pub admin_callers: HashSet<String>,
}

fn uuid(raw: &str, field: &str) -> Result<Uuid, Status> {
    Uuid::parse_str(raw).map_err(|e| Status::invalid_argument(format!("invalid {field}: {e}")))
}

fn require_admin<T>(request: &Request<T>, admin_callers: &HashSet<String>) -> Result<(), Status> {
    let claims = request
        .extensions()
        .get::<ServiceJwtClaims>()
        .ok_or_else(|| Status::unauthenticated("missing service claims"))?;
    if admin_callers.contains(&claims.sub) {
        Ok(())
    } else {
        Err(Status::permission_denied(
            "caller is not allowed for model-router admin operations",
        ))
    }
}

fn map_error(error: ModelRouterError) -> Status {
    match error {
        ModelRouterError::NoHealthyAccountForModel | ModelRouterError::AccountNotFound => {
            Status::not_found(error.to_string())
        }
        ModelRouterError::AllAccountsCircuitOpen => Status::unavailable(error.to_string()),
        ModelRouterError::ModelNotFound => Status::not_found(error.to_string()),
        ModelRouterError::Validation(message) => Status::invalid_argument(message),
        ModelRouterError::DependencyUnavailable(message) => Status::unavailable(message),
    }
}

fn account_load(value: DomainAccountLoad) -> AccountLoad {
    AccountLoad {
        account_id: value.account_id.to_string(),
        active_connections: value.active_connections,
        current_tpm: value.current_tpm,
        current_rpm: value.current_rpm,
    }
}

fn circuit_status(value: CircuitStatus) -> GetCircuitStatusResponse {
    GetCircuitStatusResponse {
        account_id: value.account_id.to_string(),
        state: value.state,
        open_until_epoch_seconds: value.open_until_epoch_seconds,
        consecutive_failures: 0,
    }
}

#[tonic::async_trait]
impl AccountSelectionService for ModelRouterGrpcService {
    async fn select_account(
        &self,
        request: Request<SelectAccountRequest>,
    ) -> Result<Response<SelectAccountResponse>, Status> {
        let r = request.into_inner();
        let selected = self
            .app
            .select_account(
                uuid(&r.model_catalog_id, "model_catalog_id")?,
                uuid(&r.request_id, "request_id")?,
                r.estimated_tokens,
            )
            .await
            .map_err(map_error)?;
        let Some(selected) = selected else {
            return Ok(Response::new(SelectAccountResponse {
                account_id: String::new(),
                aws_account_id: String::new(),
                region: String::new(),
                provider_model_id: String::new(),
                found: false,
            }));
        };
        Ok(Response::new(SelectAccountResponse {
            account_id: selected.account_id.to_string(),
            aws_account_id: selected.aws_account_id,
            region: selected.region,
            provider_model_id: selected.provider_model_id,
            found: true,
        }))
    }

    async fn report_outcome(
        &self,
        request: Request<ReportOutcomeRequest>,
    ) -> Result<Response<ReportOutcomeResponse>, Status> {
        let r = request.into_inner();
        self.app
            .report_outcome(
                uuid(&r.account_id, "account_id")?,
                uuid(&r.request_id, "request_id")?,
                r.success,
                r.error_code,
                r.input_tokens,
                r.output_tokens,
            )
            .await
            .map_err(map_error)?;
        Ok(Response::new(ReportOutcomeResponse { acknowledged: true }))
    }

    async fn reset_circuit_breaker(
        &self,
        request: Request<ResetCircuitBreakerRequest>,
    ) -> Result<Response<ResetCircuitBreakerResponse>, Status> {
        require_admin(&request, &self.admin_callers)?;
        let closed = self
            .app
            .reset_circuit(uuid(&request.into_inner().account_id, "account_id")?)
            .await
            .map_err(map_error)?;
        Ok(Response::new(ResetCircuitBreakerResponse { closed }))
    }

    async fn get_circuit_status(
        &self,
        request: Request<GetCircuitStatusRequest>,
    ) -> Result<Response<GetCircuitStatusResponse>, Status> {
        require_admin(&request, &self.admin_callers)?;
        let value = self
            .app
            .circuit_status(uuid(&request.into_inner().account_id, "account_id")?)
            .await
            .map_err(map_error)?;
        Ok(Response::new(circuit_status(value)))
    }

    async fn get_account_load(
        &self,
        request: Request<GetAccountLoadRequest>,
    ) -> Result<Response<GetAccountLoadResponse>, Status> {
        require_admin(&request, &self.admin_callers)?;
        let load = self
            .app
            .get_load(uuid(&request.into_inner().account_id, "account_id")?)
            .await
            .map_err(map_error)?;
        Ok(Response::new(GetAccountLoadResponse {
            load: Some(account_load(load)),
        }))
    }

    async fn list_account_loads(
        &self,
        request: Request<ListAccountLoadsRequest>,
    ) -> Result<Response<ListAccountLoadsResponse>, Status> {
        require_admin(&request, &self.admin_callers)?;
        let loads = self
            .app
            .list_loads(uuid(
                &request.into_inner().model_catalog_id,
                "model_catalog_id",
            )?)
            .await
            .map_err(map_error)?
            .into_iter()
            .map(account_load)
            .collect();
        Ok(Response::new(ListAccountLoadsResponse { loads }))
    }
}
