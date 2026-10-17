use crate::domain::entities::PaymentMethod;
use crate::domain::errors::PaymentError;
use crate::domain::repositories::PaymentMethodRepository;
use std::sync::Arc;
use uuid::Uuid;

pub struct ListPaymentMethods {
    pub repo: Arc<dyn PaymentMethodRepository>,
}

impl ListPaymentMethods {
    pub async fn execute(&self, user_id: Uuid) -> Result<Vec<PaymentMethod>, PaymentError> {
        self.repo.find_by_user(user_id).await
    }
}

pub struct GetPaymentMethod {
    pub repo: Arc<dyn PaymentMethodRepository>,
}

impl GetPaymentMethod {
    pub async fn execute(
        &self,
        user_id: Uuid,
        id: Uuid,
    ) -> Result<Option<PaymentMethod>, PaymentError> {
        match self.repo.find_by_id(id).await? {
            Some(value) if value.user_id == user_id => Ok(Some(value)),
            _ => Ok(None),
        }
    }
}
