use crate::domain::errors::ModelRouterError;
use events::outbox::OutboxStore;
use events::{
    AccountMarkedUnhealthy, AccountRecovered, EventEnvelope, EVENT_ACCOUNT_MARKED_UNHEALTHY,
    EVENT_ACCOUNT_RECOVERED,
};

#[derive(Clone)]
pub struct RouterEventPublisher {
    pub outbox: OutboxStore,
}

impl RouterEventPublisher {
    pub async fn publish_account_marked_unhealthy(
        &self,
        payload: &AccountMarkedUnhealthy,
    ) -> Result<(), ModelRouterError> {
        let event = EventEnvelope::new(EVENT_ACCOUNT_MARKED_UNHEALTHY, payload)
            .to_json()
            .map_err(|e| ModelRouterError::Validation(e.to_string()))?;
        self.publish_json(EVENT_ACCOUNT_MARKED_UNHEALTHY, event)
            .await
    }

    pub async fn publish_account_recovered(
        &self,
        payload: &AccountRecovered,
    ) -> Result<(), ModelRouterError> {
        let event = EventEnvelope::new(EVENT_ACCOUNT_RECOVERED, payload)
            .to_json()
            .map_err(|e| ModelRouterError::Validation(e.to_string()))?;
        self.publish_json(EVENT_ACCOUNT_RECOVERED, event).await
    }

    async fn publish_json(&self, subject: &str, payload: Vec<u8>) -> Result<(), ModelRouterError> {
        let payload = serde_json::from_slice(&payload)
            .map_err(|error| ModelRouterError::Validation(error.to_string()))?;
        self.outbox
            .enqueue_value(subject, &payload)
            .await
            .map(|_| ())
            .map_err(|error| ModelRouterError::DependencyUnavailable(error.to_string()))
    }
}
