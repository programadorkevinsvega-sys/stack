use crate::application::commands::{
    AttachPaymentMethod, AttachPaymentMethodInput, CapturePayPalOrder, ChargePayPalVault,
    CreatePayPalPaymentToken, CreatePayPalVaultSetupToken, CreateTopUp, CreateTopUpInput,
    RemovePaymentMethod, SetDefaultPaymentMethod,
};
use crate::application::queries::{GetPaymentMethod, ListPaymentMethods};
use crate::domain::entities::PaymentMethod as DomainPaymentMethod;
use crate::domain::errors::PaymentError;
use proto::payment_v1::payment_service_server::PaymentService;
use proto::payment_v1::{
    AttachPaymentMethodRequest, CapturePayPalOrderRequest, CapturePayPalOrderResponse,
    ChargePayPalVaultRequest, ChargePayPalVaultResponse, CreatePayPalPaymentTokenRequest,
    CreatePayPalPaymentTokenResponse, CreatePayPalVaultSetupTokenRequest,
    CreatePayPalVaultSetupTokenResponse, CreateTopUpRequest, CreateTopUpResponse,
    GetPaymentMethodRequest, GetPaymentMethodResponse, ListPaymentMethodsRequest,
    ListPaymentMethodsResponse, PaymentMethod, PaymentMethodResponse, RemovePaymentMethodRequest,
    RemovePaymentMethodResponse, SetDefaultPaymentMethodRequest,
};
use rust_decimal::Decimal;
use tonic::{Request, Response, Status};
use uuid::Uuid;

pub struct PaymentGrpcService {
    pub create_top_up: CreateTopUp,
    pub capture_paypal_order: CapturePayPalOrder,
    pub charge_paypal_vault: ChargePayPalVault,
    pub create_paypal_vault_setup_token: CreatePayPalVaultSetupToken,
    pub create_paypal_payment_token: CreatePayPalPaymentToken,
    pub attach_payment_method: AttachPaymentMethod,
    pub remove_payment_method: RemovePaymentMethod,
    pub set_default_payment_method: SetDefaultPaymentMethod,
    pub list_payment_methods: ListPaymentMethods,
    pub get_payment_method: GetPaymentMethod,
}

fn uuid(raw: &str, field: &str) -> Result<Uuid, Status> {
    Uuid::parse_str(raw).map_err(|e| Status::invalid_argument(format!("invalid {field}: {e}")))
}

fn decimal(raw: &str, field: &str) -> Result<Decimal, Status> {
    raw.parse::<Decimal>()
        .map_err(|e| Status::invalid_argument(format!("invalid {field}: {e}")))
}

fn map_error(error: PaymentError) -> Status {
    match error {
        PaymentError::PaymentMethodNotFound | PaymentError::PaymentProviderEventNotFound => {
            Status::not_found(error.to_string())
        }
        PaymentError::DuplicateProviderEvent => Status::already_exists(error.to_string()),
        PaymentError::InvalidWebhookSignature => Status::unauthenticated(error.to_string()),
        PaymentError::PaymentFailed(message) => Status::failed_precondition(message),
        PaymentError::ProviderError(message) => Status::unavailable(message),
        PaymentError::Validation(message) => Status::invalid_argument(message),
        PaymentError::DependencyUnavailable(message) => Status::unavailable(message),
    }
}

fn payment_method(value: DomainPaymentMethod) -> PaymentMethod {
    PaymentMethod {
        id: value.id.to_string(),
        user_id: value.user_id.to_string(),
        provider: value.provider,
        provider_payment_method_id: value.provider_payment_method_id,
        brand: value.brand,
        last4: value.last4,
        exp_month: value.exp_month.map(i32::from),
        exp_year: value.exp_year.map(i32::from),
        is_default: value.is_default,
        created_at: value.created_at.to_rfc3339(),
    }
}

#[tonic::async_trait]
impl PaymentService for PaymentGrpcService {
    async fn charge_pay_pal_vault(
        &self,
        request: Request<ChargePayPalVaultRequest>,
    ) -> Result<Response<ChargePayPalVaultResponse>, Status> {
        let r = request.into_inner();
        let capture = self
            .charge_paypal_vault
            .execute(
                uuid(&r.user_id, "user_id")?,
                decimal(&r.amount, "amount")?,
                r.currency,
                r.idempotency_key,
                r.reference_id,
            )
            .await
            .map_err(map_error)?;
        Ok(Response::new(ChargePayPalVaultResponse {
            success: true,
            provider_reference: capture.provider_reference,
            status: capture.status,
        }))
    }
    async fn create_top_up(
        &self,
        request: Request<CreateTopUpRequest>,
    ) -> Result<Response<CreateTopUpResponse>, Status> {
        let r = request.into_inner();
        let order = self
            .create_top_up
            .execute(CreateTopUpInput {
                user_id: uuid(&r.user_id, "user_id")?,
                amount: decimal(&r.amount, "amount")?,
                currency: r.currency,
                idempotency_key: r.idempotency_key,
                return_url: r.return_url,
                cancel_url: r.cancel_url,
                kind: if r.kind.trim().is_empty() {
                    "top_up".into()
                } else {
                    r.kind
                },
                reference_id: r.reference_id,
            })
            .await
            .map_err(map_error)?;
        Ok(Response::new(CreateTopUpResponse {
            provider: "paypal".into(),
            provider_reference: order.provider_reference,
            approval_url: order.approval_url,
            status: order.status,
        }))
    }

    async fn capture_pay_pal_order(
        &self,
        request: Request<CapturePayPalOrderRequest>,
    ) -> Result<Response<CapturePayPalOrderResponse>, Status> {
        let r = request.into_inner();
        let capture = self
            .capture_paypal_order
            .execute(uuid(&r.user_id, "user_id")?, r.order_id)
            .await
            .map_err(map_error)?;
        Ok(Response::new(CapturePayPalOrderResponse {
            success: true,
            provider_reference: capture.provider_reference,
            status: capture.status,
        }))
    }

    async fn attach_payment_method(
        &self,
        request: Request<AttachPaymentMethodRequest>,
    ) -> Result<Response<PaymentMethodResponse>, Status> {
        let r = request.into_inner();
        let method = self
            .attach_payment_method
            .execute(AttachPaymentMethodInput {
                user_id: uuid(&r.user_id, "user_id")?,
                provider: r.provider,
                provider_payment_method_id: r.provider_payment_method_id,
                brand: r.brand,
                last4: r.last4,
                exp_month: r.exp_month,
                exp_year: r.exp_year,
                set_default: r.set_default,
            })
            .await
            .map_err(map_error)?;
        Ok(Response::new(PaymentMethodResponse {
            payment_method: Some(payment_method(method)),
        }))
    }

    async fn create_pay_pal_vault_setup_token(
        &self,
        request: Request<CreatePayPalVaultSetupTokenRequest>,
    ) -> Result<Response<CreatePayPalVaultSetupTokenResponse>, Status> {
        let r = request.into_inner();
        let setup = self
            .create_paypal_vault_setup_token
            .execute(uuid(&r.user_id, "user_id")?, r.return_url, r.cancel_url)
            .await
            .map_err(map_error)?;
        Ok(Response::new(CreatePayPalVaultSetupTokenResponse {
            setup_token_id: setup.setup_token_id,
            approval_url: setup.approval_url,
            status: setup.status,
        }))
    }

    async fn create_pay_pal_payment_token(
        &self,
        request: Request<CreatePayPalPaymentTokenRequest>,
    ) -> Result<Response<CreatePayPalPaymentTokenResponse>, Status> {
        let r = request.into_inner();
        let (method, token) = self
            .create_paypal_payment_token
            .execute(
                uuid(&r.user_id, "user_id")?,
                r.setup_token_id,
                r.set_default,
            )
            .await
            .map_err(map_error)?;
        Ok(Response::new(CreatePayPalPaymentTokenResponse {
            payment_method: Some(payment_method(method)),
            payment_token_id: token.payment_token_id,
            payer_id: token.payer_id,
            email: token.email,
        }))
    }

    async fn remove_payment_method(
        &self,
        request: Request<RemovePaymentMethodRequest>,
    ) -> Result<Response<RemovePaymentMethodResponse>, Status> {
        let r = request.into_inner();
        self.remove_payment_method
            .execute(
                uuid(&r.user_id, "user_id")?,
                uuid(&r.payment_method_id, "payment_method_id")?,
            )
            .await
            .map_err(map_error)?;
        Ok(Response::new(RemovePaymentMethodResponse { success: true }))
    }

    async fn set_default_payment_method(
        &self,
        request: Request<SetDefaultPaymentMethodRequest>,
    ) -> Result<Response<PaymentMethodResponse>, Status> {
        let r = request.into_inner();
        let method = self
            .set_default_payment_method
            .execute(
                uuid(&r.user_id, "user_id")?,
                uuid(&r.payment_method_id, "payment_method_id")?,
            )
            .await
            .map_err(map_error)?;
        Ok(Response::new(PaymentMethodResponse {
            payment_method: Some(payment_method(method)),
        }))
    }

    async fn list_payment_methods(
        &self,
        request: Request<ListPaymentMethodsRequest>,
    ) -> Result<Response<ListPaymentMethodsResponse>, Status> {
        let values = self
            .list_payment_methods
            .execute(uuid(&request.into_inner().user_id, "user_id")?)
            .await
            .map_err(map_error)?
            .into_iter()
            .map(payment_method)
            .collect();
        Ok(Response::new(ListPaymentMethodsResponse {
            payment_methods: values,
        }))
    }

    async fn get_payment_method(
        &self,
        request: Request<GetPaymentMethodRequest>,
    ) -> Result<Response<GetPaymentMethodResponse>, Status> {
        let r = request.into_inner();
        let value = self
            .get_payment_method
            .execute(
                uuid(&r.user_id, "user_id")?,
                uuid(&r.payment_method_id, "payment_method_id")?,
            )
            .await
            .map_err(map_error)?
            .map(payment_method);
        Ok(Response::new(GetPaymentMethodResponse {
            payment_method: value,
        }))
    }
}
