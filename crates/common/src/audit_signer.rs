use hmac::{Hmac, Mac};
use sha2::Sha256;

type AuditHmac = Hmac<Sha256>;

#[derive(Clone)]
pub struct AuditSigner {
    key: Vec<u8>,
}

impl AuditSigner {
    pub fn new(secret: impl AsRef<[u8]>) -> Self {
        Self {
            key: secret.as_ref().to_vec(),
        }
    }

    pub fn sign(
        &self,
        actor_type: &str,
        actor_id: Option<uuid::Uuid>,
        action: &str,
        resource_type: &str,
        resource_id: Option<uuid::Uuid>,
        metadata: Option<&str>,
    ) -> Result<String, String> {
        let mut mac =
            AuditHmac::new_from_slice(&self.key).map_err(|e| format!("hmac init failed: {e}"))?;
        mac.update(actor_type.as_bytes());
        mac.update(actor_id.unwrap_or_default().to_string().as_bytes());
        mac.update(action.as_bytes());
        mac.update(resource_type.as_bytes());
        mac.update(resource_id.unwrap_or_default().to_string().as_bytes());
        if let Some(m) = metadata {
            mac.update(m.as_bytes());
        }
        Ok(hex::encode(mac.finalize().into_bytes()))
    }

    pub fn verify(&self, input: AuditSignatureInput<'_>) -> Result<bool, String> {
        let expected = self.sign(
            input.actor_type,
            input.actor_id,
            input.action,
            input.resource_type,
            input.resource_id,
            input.metadata,
        )?;
        Ok(expected == input.signature)
    }
}

pub struct AuditSignatureInput<'a> {
    pub actor_type: &'a str,
    pub actor_id: Option<uuid::Uuid>,
    pub action: &'a str,
    pub resource_type: &'a str,
    pub resource_id: Option<uuid::Uuid>,
    pub metadata: Option<&'a str>,
    pub signature: &'a str,
}
