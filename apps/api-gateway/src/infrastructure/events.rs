use crate::domain::errors::GatewayError;
use common::RequestId;
use events::outbox::OutboxStore;
use events::{
    EventEnvelope, RequestFailed, UsageRecorded, EVENT_REQUEST_FAILED, EVENT_USAGE_RECORDED,
};

#[derive(Clone)]
pub struct GatewayEventPublisher {
    pub outbox: OutboxStore,
}

impl GatewayEventPublisher {
    pub async fn publish_usage_recorded(
        &self,
        payload: &UsageRecorded,
        request_id: RequestId,
    ) -> Result<(), GatewayError> {
        let event =
            EventEnvelope::with_request_id(EVENT_USAGE_RECORDED, payload, request_id.into())
                .to_json()
                .map_err(|e| GatewayError::Internal(e.to_string()))?;
        self.publish_json(EVENT_USAGE_RECORDED, event).await
    }

    pub async fn publish_request_failed(
        &self,
        payload: &RequestFailed,
        request_id: RequestId,
    ) -> Result<(), GatewayError> {
        let event =
            EventEnvelope::with_request_id(EVENT_REQUEST_FAILED, payload, request_id.into())
                .to_json()
                .map_err(|e| GatewayError::Internal(e.to_string()))?;
        self.publish_json(EVENT_REQUEST_FAILED, event).await
    }

    async fn publish_json(&self, subject: &str, payload: Vec<u8>) -> Result<(), GatewayError> {
        let payload = serde_json::from_slice(&payload)
            .map_err(|error| GatewayError::Internal(error.to_string()))?;
        self.outbox
            .enqueue_value(subject, &payload)
            .await
            .map(|_| ())
            .map_err(|error| GatewayError::UpstreamUnavailable(error.to_string()))
    }
}
